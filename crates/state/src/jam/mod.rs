mod cast;
mod clock;
mod lead;
mod receiver;
mod server;
mod wire;

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, Entity, EventEmitter, Task};
use music::cast::{CastSink, Feed};
use tokio::sync::{mpsc, oneshot, watch};

type Reply<T> = oneshot::Sender<T>;

use crate::{AppSettings, Cover, Io, Playback, PlaybackState, Queue, Session, join};
use cast::Broadcast;
use receiver::ReceiverEvent;
use server::{Playing, ServerEvent, Serving, Transport};
use wire::{Hit, Refusal};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub name: String,
    pub at: String,
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
    settings: Entity<AppSettings>,
    io: Io,
    now: watch::Sender<Option<Playing>>,
    transport: watch::Sender<Transport>,
    kicks: tokio::sync::broadcast::Sender<String>,
    serve: Option<Task<()>>,
    follow: Option<Task<()>>,
    asks: Vec<Task<()>>,
}

impl EventEmitter<JamEvent> for Jam {}

impl Jam {
    pub fn new(
        playback: Entity<Playback>,
        cover: Entity<Cover>,
        session: Entity<Session>,
        queue: Entity<Queue>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
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
        })
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
            settings,
            io,
            now: watch::channel(None).0,
            transport: watch::channel(Transport::default()).0,
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
            lead: self.settings.read(cx).jam_lead(),
            broadcast: self.broadcast.clone(),
            now: self.now.subscribe(),
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
            lead: self.settings.read(cx).jam_lead(),
        };
        cx.emit(JamEvent::Started);
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.hosting() {
            return;
        }

        self.serve = None;
        self.follow = None;
        self.broadcast.follow(None);
        self.role = JamRole::Idle;
        cx.emit(JamEvent::Stopped);
        cx.notify();
    }

    fn arrived(&mut self, event: ServerEvent, cx: &mut Context<Self>) {
        let event = match event {
            ServerEvent::Find { query, reply } => return self.find(query, reply, cx),
            ServerEvent::Add { id, reply } => return self.add(id, reply, cx),
            event => event,
        };

        let JamRole::Hosting {
            listeners, lead, ..
        } = &mut self.role
        else {
            return;
        };

        match event {
            ServerEvent::Find { .. } | ServerEvent::Add { .. } => {}
            ServerEvent::Joined(listener) => {
                listeners.retain(|held| held.at != listener.at);
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

    fn add(&mut self, id: String, reply: Reply<Option<String>>, cx: &mut Context<Self>) {
        if !self.settings.read(cx).jam_guests_add() {
            reply.send(None).ok();
            return;
        }

        let session = self.session.read(cx);
        let client = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug));
        let Some(client) = client else {
            reply.send(None).ok();
            return;
        };

        let io = self.io.clone();
        let queue = self.queue.clone();
        self.remember(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move { client.track(&id).await })).await;
            let Ok(track) = found else {
                reply.send(None).ok();
                return;
            };

            let title = track.name.clone();
            let queued = this
                .update(cx, |_, cx| {
                    queue.update(cx, |queue, cx| queue.append(track, cx))
                })
                .is_ok();
            reply.send(queued.then_some(title)).ok();
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

    pub fn kick(&mut self, at: &str, cx: &mut Context<Self>) {
        self.kicks.send(at.to_owned()).ok();
        let JamRole::Hosting { listeners, .. } = &mut self.role else {
            return;
        };
        listeners.retain(|held| held.at != at);
        cx.notify();
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
