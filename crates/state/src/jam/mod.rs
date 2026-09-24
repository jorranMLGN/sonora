mod cast;
mod clock;
mod lead;
mod receiver;
mod server;
mod snapshot;
mod wire;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, Entity, EventEmitter, Task};
use music::cast::{CastSink, Feed};
use tokio::sync::{mpsc, oneshot, watch};

type Reply<T> = oneshot::Sender<T>;
type Waiting = Vec<Reply<Option<Arc<Sheet>>>>;

use crate::{
    AppSettings, Cover, Io, Library, LibraryState, Lyrics, Playback, PlaybackState, Queue, Session,
    join,
};
use cast::Broadcast;
use receiver::ReceiverEvent;
use server::{Playing, ServerEvent, Serving, Sheet, Transport};
use snapshot::{Controls, Lineup, Seat, Snapshot, Words};
use wire::{Act, Denial, Hit, Line, Pack, Refusal};

pub use wire::{Cap, Caps};

const BEHIND: usize = 3;
const AHEAD: usize = 50;
const PACKS: usize = 100;
const OPENED: usize = 100;
const LINES: usize = 400;

const ROOM: &str = "Sonora";
const AUTOSTART: &str = "SONORA_JAM_AUTOSTART";
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(15),
];
const PROBE: [&str; 2] = ["8.8.8.8:80", "192.168.1.1:9"];
const PER_PROVIDER: usize = 6;
const ASKS: usize = 8;

pub struct Parts {
    pub playback: Entity<Playback>,
    pub cover: Entity<Cover>,
    pub session: Entity<Session>,
    pub queue: Entity<Queue>,
    pub library: Entity<Library>,
    pub lyrics: Entity<Lyrics>,
    pub settings: Entity<AppSettings>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub name: String,
    pub at: String,
    pub device: String,
    pub native: bool,
    pub need: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum JamRole {
    #[default]
    Idle,
    Hosting {
        room: String,
        code: String,
        addresses: Vec<String>,
        listeners: Vec<Listener>,
        lead: u32,
    },
    Joining {
        at: String,
    },
    Listening {
        room: String,
        host: String,
        at: String,
    },
    Lost {
        at: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JamEvent {
    Started,
    Stopped,
    Joined,
    Left,
    Refused(Refusal),
}

pub struct Jam {
    role: JamRole,
    broadcast: Arc<Broadcast>,
    playback: Entity<Playback>,
    cover: Entity<Cover>,
    session: Entity<Session>,
    queue: Entity<Queue>,
    library: Entity<Library>,
    lyrics: Entity<Lyrics>,
    settings: Entity<AppSettings>,
    io: Io,
    opened: HashMap<String, Arc<Sheet>>,
    opening: HashMap<String, Waiting>,
    now: watch::Sender<Option<Playing>>,
    transport: watch::Sender<Transport>,
    snapshot: watch::Sender<Arc<Snapshot>>,
    grants: HashMap<String, Caps>,
    kicks: tokio::sync::broadcast::Sender<String>,
    serve: Option<Task<()>>,
    follow: Option<Task<()>>,
    asks: Vec<Task<()>>,
}

impl EventEmitter<JamEvent> for Jam {}

impl Jam {
    pub fn new(parts: Parts, io: Io, cx: &mut Context<Self>) -> Self {
        let Parts {
            playback,
            cover,
            session,
            queue,
            library,
            lyrics,
            settings,
        } = parts;
        let (feeds, incoming) = mpsc::unbounded_channel();
        let sink: CastSink = Arc::new(move |feed: Feed| {
            feeds.send(feed).ok();
        });
        playback.update(cx, |playback, _| playback.set_cast(sink));

        cx.observe(&playback, |this: &mut Self, playback, cx| {
            if this.listening() && matches!(playback.read(cx).state(), PlaybackState::Playing) {
                return this.leave(cx);
            }

            let large = this.cover.read(cx).max().map(str::to_owned);
            let playback = playback.read(cx);
            this.broadcast
                .follow(playback.current_slug().and_then(known));
            this.now
                .send_replace(playback.track().map(|track| playing(track, large)));
            this.transport.send_replace(Transport {
                playing: matches!(playback.state(), PlaybackState::Playing),
                position_ms: playback.live_position().as_millis() as u64,
            });
            this.publish(cx);
        })
        .detach();

        cx.observe(&queue, |this: &mut Self, _, cx| this.publish(cx))
            .detach();

        cx.observe(&library, |this: &mut Self, _, cx| this.publish(cx))
            .detach();

        cx.observe(&lyrics, |this: &mut Self, _, cx| this.publish(cx))
            .detach();

        cx.observe(&cover, |this: &mut Self, cover, cx| {
            let large = cover.read(cx).max().map(str::to_owned);
            let track = this.playback.read(cx).track().cloned();
            this.now
                .send_replace(track.map(|track| playing(&track, large)));
        })
        .detach();

        Self {
            role: JamRole::Idle,
            broadcast: Arc::new(Broadcast::new(incoming)),
            playback,
            cover,
            session,
            queue,
            library,
            lyrics,
            settings,
            io,
            opened: HashMap::new(),
            opening: HashMap::new(),
            now: watch::channel(None).0,
            transport: watch::channel(Transport::default()).0,
            snapshot: watch::channel(Arc::new(Snapshot::default())).0,
            grants: HashMap::new(),
            kicks: tokio::sync::broadcast::channel(8).0,
            serve: None,
            follow: None,
            asks: Vec::new(),
        }
    }

    pub fn autostart(&mut self, cx: &mut Context<Self>) {
        match std::env::var(AUTOSTART) {
            Err(_) => return,
            Ok(value) if value == "0" => return,
            Ok(_) => {}
        }

        self.start(cx);
        let JamRole::Hosting {
            code, addresses, ..
        } = &self.role
        else {
            return;
        };
        for address in addresses {
            log::info!("jam: hosting at http://{address}/c/{code}");
        }
    }

    pub fn role(&self) -> &JamRole {
        &self.role
    }

    pub fn hosting(&self) -> bool {
        matches!(self.role, JamRole::Hosting { .. })
    }

    pub fn listening(&self) -> bool {
        matches!(
            self.role,
            JamRole::Listening { .. } | JamRole::Joining { .. } | JamRole::Lost { .. }
        )
    }

    pub fn join(&mut self, at: String, code: String, cx: &mut Context<Self>) {
        if self.hosting() || self.listening() {
            return;
        }

        self.playback.update(cx, |playback, cx| playback.pause(cx));
        self.role = JamRole::Joining { at: at.clone() };
        cx.notify();

        let io = self.io.clone();
        let name = match self.settings.read(cx).jam_name() {
            "" => ROOM.to_owned(),
            name => name.to_owned(),
        };

        self.follow = Some(cx.spawn(async move |this, cx| {
            let mut failures = 0;
            loop {
                let (events, mut arriving) = mpsc::unbounded_channel();
                let listening = io.spawn(receiver::listen(
                    at.clone(),
                    code.clone(),
                    name.clone(),
                    events,
                ));

                let mut welcomed = false;
                let mut give_up = false;
                while let Some(event) = arriving.recv().await {
                    welcomed |= matches!(event, ReceiverEvent::Welcomed(_));
                    give_up |= matches!(event, ReceiverEvent::Refused(_) | ReceiverEvent::Ended(_));
                    if this
                        .update(cx, |this, cx| this.heard(event, &at, cx))
                        .is_err()
                    {
                        return;
                    }
                }
                listening.await.ok();

                if give_up {
                    break;
                }
                if welcomed {
                    failures = 0;
                }
                let Some(wait) = BACKOFF.get(failures) else {
                    break;
                };
                failures += 1;
                cx.background_executor().timer(*wait).await;
            }

            this.update(cx, |this, cx| {
                this.role = JamRole::Idle;
                cx.emit(JamEvent::Left);
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn leave(&mut self, cx: &mut Context<Self>) {
        if !self.listening() {
            return;
        }

        self.follow = None;
        self.role = JamRole::Idle;
        cx.emit(JamEvent::Left);
        cx.notify();
    }

    fn heard(&mut self, event: ReceiverEvent, at: &str, cx: &mut Context<Self>) {
        match event {
            ReceiverEvent::Welcomed(room) => {
                self.role = JamRole::Listening {
                    room,
                    host: at.to_owned(),
                    at: at.to_owned(),
                };
                cx.emit(JamEvent::Joined);
            }
            ReceiverEvent::Now(playing) => {
                self.now.send_replace(Some(playing));
            }
            ReceiverEvent::Refused(reason) => {
                self.role = JamRole::Idle;
                cx.emit(JamEvent::Refused(reason));
            }
            ReceiverEvent::Ended(reason) => {
                log::info!("jam: the host ended the session: {reason:?}");
                self.role = JamRole::Idle;
            }
            ReceiverEvent::Lost => self.role = JamRole::Lost { at: at.to_owned() },
        }
        cx.notify();
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.hosting() {
            return;
        }

        let settings = self.settings.read(cx);
        let port = settings.jam_port();
        let name = settings.jam_name();
        let room = match name.is_empty() {
            true => ROOM.to_owned(),
            false => name.to_owned(),
        };

        let large = self.cover.read(cx).max().map(str::to_owned);
        let playback = self.playback.read(cx);
        self.broadcast
            .follow(playback.current_slug().and_then(known));
        self.now
            .send_replace(playback.track().map(|track| playing(track, large)));
        self.transport.send_replace(Transport {
            playing: matches!(playback.state(), PlaybackState::Playing),
            position_ms: playback.live_position().as_millis() as u64,
        });

        let code = format!("{:06}", fastrand::u32(0..1_000_000));
        let (listener, bound) = match server::bind(port) {
            Ok(bound) => bound,
            Err(error) => return log::error!("jam: cannot start hosting: {error:#}"),
        };

        let (events, mut arriving) = mpsc::unbounded_channel();
        let serving = Serving {
            kicks: self.kicks.clone(),
            transport: self.transport.subscribe(),
            code: code.clone(),
            room: room.clone(),
            broadcast: self.broadcast.clone(),
            now: self.now.subscribe(),
            snapshot: self.snapshot.subscribe(),
            events,
        };

        let served = self.io.spawn(server::run(listener, serving));
        self.serve = Some(cx.spawn(async move |_, _| {
            served.await.ok();
        }));
        self.follow = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = arriving.recv().await {
                this.update(cx, |this, cx| this.arrived(event, cx)).ok();
            }
        }));

        self.role = JamRole::Hosting {
            room,
            code,
            addresses: addresses(bound),
            listeners: Vec::new(),
            lead: lead::FLOOR,
        };
        cx.emit(JamEvent::Started);
        cx.notify();
        self.publish(cx);
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.hosting() {
            return;
        }

        self.serve = None;
        self.follow = None;
        self.broadcast.follow(None);
        self.grants.clear();
        self.snapshot.send_replace(Arc::new(Snapshot::default()));
        self.role = JamRole::Idle;
        cx.emit(JamEvent::Stopped);
        cx.notify();
    }

    fn arrived(&mut self, event: ServerEvent, cx: &mut Context<Self>) {
        let event = match event {
            ServerEvent::Find { query, reply } => return self.find(query, reply, cx),
            ServerEvent::Add { id, device, reply } => return self.add(id, device, reply, cx),
            ServerEvent::Do { act, device, reply } => return self.act(act, device, reply, cx),
            ServerEvent::Open {
                pack,
                device,
                reply,
            } => return self.open(pack, device, reply, cx),
            ServerEvent::Pick {
                pack,
                next,
                device,
                reply,
            } => return self.pick(pack, next, device, reply, cx),
            ServerEvent::Love {
                id,
                on,
                device,
                reply,
            } => return self.love(id, on, device, reply, cx),
            event => event,
        };

        let mut seeded = None;
        let JamRole::Hosting {
            listeners, lead, ..
        } = &mut self.role
        else {
            return;
        };

        match event {
            ServerEvent::Find { .. }
            | ServerEvent::Add { .. }
            | ServerEvent::Do { .. }
            | ServerEvent::Open { .. }
            | ServerEvent::Pick { .. }
            | ServerEvent::Love { .. } => {}
            ServerEvent::Joined(listener) => {
                listeners.retain(|held| held.at != listener.at && held.device != listener.device);
                seeded = Some(listener.device.clone());
                listeners.push(listener);
                cx.emit(JamEvent::Joined);
            }
            ServerEvent::Left(at) => {
                listeners.retain(|held| held.at != at);
                cx.emit(JamEvent::Left);
            }
            ServerEvent::Lead { lead_ms, needs } => {
                *lead = lead_ms;
                for listener in listeners.iter_mut() {
                    listener.need = needs
                        .iter()
                        .find(|(at, _)| *at == listener.at)
                        .map(|(_, need)| *need)
                        .unwrap_or(0);
                }
            }
        }
        cx.notify();

        if let Some(device) = seeded {
            let guests = self.settings.read(cx).jam_guests();
            self.grants.entry(device).or_insert(guests);
        }
        self.publish(cx);
    }

    fn act(&mut self, act: Act, device: String, reply: Reply<bool>, cx: &mut Context<Self>) {
        if !self.can(&device, Cap::Control) {
            reply.send(false).ok();
            return;
        }

        match act {
            Act::Play => self.playback.update(cx, |this, cx| this.resume(cx)),
            Act::Pause => self.playback.update(cx, |this, cx| this.pause(cx)),
            Act::Next => self.playback.update(cx, |this, cx| this.next(cx)),
            Act::Previous => self.playback.update(cx, |this, cx| this.previous(cx)),
            Act::Repeat => self.playback.update(cx, |this, cx| this.cycle_repeat(cx)),
            Act::Seek { ms } => self
                .playback
                .update(cx, |this, cx| this.seek(Duration::from_millis(ms), cx)),
            Act::Volume { level } => self.playback.update(cx, |this, cx| {
                this.set_volume(level.min(100) as f32 / 100., cx)
            }),
            Act::Shuffle { on } => self.queue.update(cx, |this, cx| this.set_shuffle(on, cx)),
            Act::Drop { index } => self
                .queue
                .update(cx, |this, cx| this.remove_upcoming(index as usize, cx)),
            Act::Jump { index } => {
                let behind = self.queue.read(cx).past().len();
                match index.cmp(&0) {
                    Ordering::Equal => {}
                    Ordering::Greater => self
                        .playback
                        .update(cx, |this, cx| this.play_upcoming(index as usize - 1, cx)),
                    Ordering::Less => {
                        let Some(back) = behind.checked_add_signed(index as isize) else {
                            reply.send(false).ok();
                            return;
                        };
                        self.playback
                            .update(cx, |this, cx| this.play_past(back, cx));
                    }
                }
            }
        }

        reply.send(true).ok();
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        let JamRole::Hosting { listeners, .. } = &self.role else {
            return;
        };

        let room = listeners
            .iter()
            .map(|listener| Seat {
                name: listener.name.clone(),
                native: listener.native,
                device: listener.device.clone(),
            })
            .collect();

        let queue = self.queue.read(cx);
        let playback = self.playback.read(cx);
        let behind = queue.past().len().min(BEHIND);
        let mut rows: Vec<Hit> = queue
            .past()
            .skip(queue.past().len() - behind)
            .map(row)
            .collect();
        let at = match queue.current() {
            Some(track) => {
                rows.push(row(track));
                rows.len() as i32 - 1
            }
            None => -1,
        };
        rows.extend(queue.upcoming().take(AHEAD).map(row));

        let favorite = playback
            .track()
            .and_then(|track| track.id.as_deref())
            .is_some_and(|id| self.library.read(cx).saved(id));

        let fresh = Snapshot {
            controls: Controls {
                volume: (playback.volume().clamp(0., 1.) * 100.).round() as u8,
                shuffle: queue.shuffle(),
                repeat: spin(playback.repeat()),
                can_next: queue.has_next(),
                can_previous: playback.has_previous(cx),
                favorite,
            },
            lineup: Lineup {
                revision: queue.revision(),
                total: queue.len() as u32,
                at,
                rows,
            },
            packs: Arc::new(self.shelves(cx)),
            words: Arc::new(self.words(cx)),
            room,
            grants: self.grants.clone(),
        };

        let same = **self.snapshot.borrow() == fresh;
        if !same {
            self.snapshot.send_replace(Arc::new(fresh));
        }
    }

    fn find(&mut self, query: String, reply: Reply<Vec<Hit>>, cx: &mut Context<Self>) {
        let session = self.session.read(cx);
        let clients: Vec<_> = session
            .active_slugs()
            .into_iter()
            .filter_map(|slug| session.client_for_slug(slug).map(|client| (slug, client)))
            .collect();

        let io = self.io.clone();
        self.remember(cx.spawn(async move |_, _| {
            let mut hits = Vec::new();
            for (slug, client) in clients {
                let wanted = query.clone();
                let found = join(io.spawn(async move { client.search(&wanted).await })).await;
                match found {
                    Ok(tracks) => hits.extend(
                        tracks
                            .into_iter()
                            .filter(|track| track.playable && track.id.is_some())
                            .take(PER_PROVIDER)
                            .map(|track| hit(track, slug)),
                    ),
                    Err(error) => log::warn!("jam: cannot search {slug}: {error:#}"),
                }
            }
            reply.send(hits).ok();
        }));
    }

    fn add(
        &mut self,
        id: String,
        device: String,
        reply: Reply<Result<String, Denial>>,
        cx: &mut Context<Self>,
    ) {
        if !self.can(&device, Cap::Add) {
            reply.send(Err(Denial::Forbidden)).ok();
            return;
        }

        let session = self.session.read(cx);
        let client = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug));
        let Some(client) = client else {
            reply.send(Err(Denial::Unknown)).ok();
            return;
        };

        let io = self.io.clone();
        let queue = self.queue.clone();
        self.remember(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move { client.track(&id).await })).await;
            let Ok(track) = found else {
                reply.send(Err(Denial::Unknown)).ok();
                return;
            };

            let title = track.name.clone();
            let queued = this
                .update(cx, |_, cx| {
                    queue.update(cx, |queue, cx| queue.append(track, cx))
                })
                .is_ok();
            reply
                .send(queued.then_some(title).ok_or(Denial::Unknown))
                .ok();
        }));
    }

    fn remember(&mut self, task: Task<()>) {
        if self.asks.len() >= ASKS {
            drop(self.asks.remove(0));
        }
        self.asks.push(task);
    }

    pub fn lead(&self) -> u32 {
        match &self.role {
            JamRole::Hosting { lead, .. } => *lead,
            _ => 0,
        }
    }

    fn shelves(&self, cx: &Context<Self>) -> Vec<Pack> {
        let library = self.library.read(cx);
        let mut packs = Vec::new();
        for slug in self.session.read(cx).active_slugs() {
            let Some(shelf) = library.shelf(slug) else {
                continue;
            };
            let LibraryState::Ready { playlists, .. } = &shelf.state else {
                continue;
            };
            packs.extend(playlists.iter().map(|playlist| Pack {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                owner: playlist.owner.clone(),
                cover: playlist.cover.clone(),
                tracks: playlist.track_count,
                provider: slug.to_owned(),
            }));
            if packs.len() >= PACKS {
                break;
            }
        }
        packs.truncate(PACKS);
        packs
    }

    fn words(&self, cx: &Context<Self>) -> Words {
        let lyrics = self.lyrics.read(cx);
        let Some(hit) = lyrics.current() else {
            return Words::default();
        };

        match &hit.lyrics {
            music::Lyrics::Plain { text, .. } => Words {
                synced: false,
                lines: text
                    .lines()
                    .take(LINES)
                    .map(|text| Line {
                        at: 0,
                        text: text.to_owned(),
                    })
                    .collect(),
            },
            music::Lyrics::Synced { lines } => Words {
                synced: true,
                lines: lines
                    .iter()
                    .take(LINES)
                    .map(|line| Line {
                        at: line.start.as_millis() as u64,
                        text: line.text.clone(),
                    })
                    .collect(),
            },
        }
    }

    fn open(
        &mut self,
        pack: String,
        device: String,
        reply: Reply<Option<Arc<Sheet>>>,
        cx: &mut Context<Self>,
    ) {
        if !self.can(&device, Cap::Browse) {
            reply.send(None).ok();
            return;
        }
        if let Some(held) = self.opened.get(&pack) {
            reply.send(Some(held.clone())).ok();
            return;
        }
        if let Some(waiting) = self.opening.get_mut(&pack) {
            waiting.push(reply);
            return;
        }

        let session = self.session.read(cx);
        let client = session
            .slug_for(&pack)
            .and_then(|slug| session.client_for_slug(slug));
        let Some(client) = client else {
            reply.send(None).ok();
            return;
        };

        self.opening.insert(pack.clone(), vec![reply]);
        let io = self.io.clone();
        self.remember(cx.spawn(async move |this, cx| {
            let asked = pack.clone();
            let found = join(io.spawn(async move { client.playlist_tracks(&asked).await })).await;
            this.update(cx, |this, _| {
                let held = found.ok().map(|tracks| {
                    Arc::new(Sheet {
                        total: tracks.len() as u32,
                        rows: tracks.iter().take(OPENED).map(row).collect(),
                    })
                });
                if let Some(held) = &held {
                    this.opened.insert(pack.clone(), held.clone());
                }
                for waiting in this.opening.remove(&pack).unwrap_or_default() {
                    waiting.send(held.clone()).ok();
                }
            })
            .ok();
        }));
    }

    fn pick(
        &mut self,
        pack: String,
        next: bool,
        device: String,
        reply: Reply<Result<String, Denial>>,
        cx: &mut Context<Self>,
    ) {
        if !self.can(&device, Cap::Browse) || !self.can(&device, Cap::Add) {
            reply.send(Err(Denial::Forbidden)).ok();
            return;
        }
        let Some(name) = self
            .library
            .read(cx)
            .playlist(&pack)
            .map(|playlist| playlist.name.clone())
        else {
            reply.send(Err(Denial::Unknown)).ok();
            return;
        };

        self.playback.update(cx, |this, cx| match next {
            true => this.play_playlist_next(&pack, cx),
            false => this.enqueue_playlist(&pack, cx),
        });
        reply.send(Ok(name)).ok();
    }

    fn love(
        &mut self,
        id: String,
        on: bool,
        device: String,
        reply: Reply<Result<String, Denial>>,
        cx: &mut Context<Self>,
    ) {
        if !self.can(&device, Cap::Favorite) {
            reply.send(Err(Denial::Forbidden)).ok();
            return;
        }

        let track = self
            .playback
            .read(cx)
            .track()
            .filter(|track| track.id.as_deref() == Some(id.as_str()))
            .cloned();
        let Some(track) = track else {
            reply.send(Err(Denial::Unknown)).ok();
            return;
        };

        let title = track.name.clone();
        let saved = self.library.read(cx).saved(&id);
        if saved != on {
            self.library
                .update(cx, |library, cx| library.toggle(track, cx));
        }
        reply.send(Ok(title)).ok();
    }

    pub fn grants(&self, device: &str) -> Caps {
        self.grants.get(device).copied().unwrap_or_default()
    }

    pub fn can(&self, device: &str, cap: Cap) -> bool {
        cap.of(&self.grants(device))
    }

    pub fn grant(&mut self, device: &str, cap: Cap, on: bool, cx: &mut Context<Self>) {
        let mut caps = self.grants(device);
        cap.set(&mut caps, on);
        self.grants.insert(device.to_owned(), caps);
        cx.notify();
        self.publish(cx);
    }

    pub fn demote_all(&mut self, cx: &mut Context<Self>) {
        let guests = self.settings.read(cx).jam_guests();
        for caps in self.grants.values_mut() {
            *caps = guests;
        }
        cx.notify();
        self.publish(cx);
    }

    pub fn kick(&mut self, device: &str, cx: &mut Context<Self>) {
        self.kicks.send(device.to_owned()).ok();
        self.grants.remove(device);
        if let JamRole::Hosting { listeners, .. } = &mut self.role {
            listeners.retain(|held| held.device != device);
        }
        cx.notify();
        self.publish(cx);
    }
}

fn hit(track: music::Track, slug: &'static str) -> Hit {
    Hit {
        id: track.id.unwrap_or_default(),
        title: track.name,
        artist: track.artists,
        album: track.album,
        cover: track.cover,
        duration_ms: track.duration.as_millis() as u64,
        provider: slug.to_owned(),
    }
}

fn playing(track: &music::Track, large: Option<String>) -> Playing {
    log::debug!(
        "jam: artwork for {} is {:?}",
        track.name,
        large.as_deref().or(track.cover.as_deref())
    );

    let provider = track
        .id
        .as_deref()
        .and_then(music::tag::slug_of)
        .unwrap_or_default();

    Playing {
        title: track.name.clone(),
        artist: track.artists.clone(),
        album: track.album.clone(),
        cover: large.or_else(|| track.cover.clone()),
        duration_ms: track.duration.as_millis() as u64,
        provider: provider.to_owned(),
    }
}

fn row(track: &music::Track) -> Hit {
    let provider = track
        .id
        .as_deref()
        .and_then(music::tag::slug_of)
        .unwrap_or_default();

    Hit {
        id: track.id.clone().unwrap_or_default(),
        title: track.name.clone(),
        artist: track.artists.clone(),
        album: track.album.clone(),
        cover: track.cover.clone(),
        duration_ms: track.duration.as_millis() as u64,
        provider: provider.to_owned(),
    }
}

fn spin(repeat: crate::Repeat) -> wire::Repeat {
    match repeat {
        crate::Repeat::Off => wire::Repeat::Off,
        crate::Repeat::All => wire::Repeat::All,
        crate::Repeat::One => wire::Repeat::One,
    }
}

fn known(slug: &str) -> Option<&'static str> {
    music::tag::SLUGS.into_iter().find(|known| *known == slug)
}

fn addresses(port: u16) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for probe in PROBE {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(probe).is_err() {
            continue;
        }
        let Ok(local) = socket.local_addr() else {
            continue;
        };
        let address = format!("{}:{port}", local.ip());
        if !found.contains(&address) {
            found.push(address);
        }
    }

    if found.is_empty() {
        log::warn!("jam: cannot read a local address to show");
    }
    found
}
