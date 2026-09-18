#![allow(dead_code)]

mod cast;
mod clock;
mod receiver;
mod server;
mod wire;

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, Entity, EventEmitter, Task};
use music::cast::{CastSink, Feed};
use tokio::sync::{mpsc, watch};

use crate::{AppSettings, Io, Playback, PlaybackState};
use cast::Broadcast;
use receiver::ReceiverEvent;
use server::{Playing, ServerEvent, Serving};
use wire::Refusal;

const ROOM: &str = "Sonora";
const AUTOSTART: &str = "SONORA_JAM_AUTOSTART";
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(15),
];
const PROBE: [&str; 2] = ["8.8.8.8:80", "192.168.1.1:9"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub name: String,
    pub at: String,
    pub native: bool,
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
    settings: Entity<AppSettings>,
    io: Io,
    now: watch::Sender<Option<Playing>>,
    serve: Option<Task<()>>,
    follow: Option<Task<()>>,
}

impl EventEmitter<JamEvent> for Jam {}

impl Jam {
    pub fn new(
        playback: Entity<Playback>,
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

            let playback = playback.read(cx);
            this.broadcast
                .follow(playback.current_slug().and_then(known));
            this.now.send_replace(playback.track().map(playing));
        })
        .detach();

        Self {
            role: JamRole::Idle,
            broadcast: Arc::new(Broadcast::new(incoming)),
            playback,
            settings,
            io,
            now: watch::channel(None).0,
            serve: None,
            follow: None,
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
            ReceiverEvent::Ended(_) => self.role = JamRole::Idle,
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

        let playback = self.playback.read(cx);
        self.broadcast
            .follow(playback.current_slug().and_then(known));
        self.now.send_replace(playback.track().map(playing));

        let code = format!("{:06}", fastrand::u32(0..1_000_000));
        let (listener, bound) = match server::bind(port) {
            Ok(bound) => bound,
            Err(error) => return log::error!("jam: cannot start hosting: {error:#}"),
        };

        let (events, mut arriving) = mpsc::unbounded_channel();
        let serving = Serving {
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
        let JamRole::Hosting { listeners, .. } = &mut self.role else {
            return;
        };

        match event {
            ServerEvent::Joined(listener) => {
                listeners.retain(|held| held.at != listener.at);
                listeners.push(listener);
                cx.emit(JamEvent::Joined);
            }
            ServerEvent::Left(at) => {
                listeners.retain(|held| held.at != at);
                cx.emit(JamEvent::Left);
            }
        }
        cx.notify();
    }

    pub fn set_lead(&mut self, lead_ms: u32, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_jam_lead(lead_ms, cx));
        cx.notify();
    }
}

fn playing(track: &music::Track) -> Playing {
    Playing {
        title: track.name.clone(),
        artist: track.artists.clone(),
        album: track.album.clone(),
        cover: track.cover.clone(),
        duration_ms: track.duration.as_millis() as u64,
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
