#![allow(dead_code)]

mod cast;
mod clock;
mod wire;

use std::net::UdpSocket;
use std::sync::Arc;

use gpui::{Context, Entity, EventEmitter, Task};
use music::cast::{CastSink, Feed};
use tokio::sync::mpsc;

use crate::{AppSettings, Io, Playback};
use cast::Broadcast;
use wire::Refusal;

const ROOM: &str = "Sonora";
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
    broadcast: Broadcast,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    io: Io,
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
            let slug = playback.read(cx).current_slug().and_then(known);
            this.broadcast.follow(slug);
        })
        .detach();

        Self {
            role: JamRole::Idle,
            broadcast: Broadcast::new(incoming),
            playback,
            settings,
            io,
            serve: None,
            follow: None,
        }
    }

    pub fn role(&self) -> &JamRole {
        &self.role
    }

    pub fn hosting(&self) -> bool {
        matches!(self.role, JamRole::Hosting { .. })
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

        let slug = self.playback.read(cx).current_slug().and_then(known);
        self.broadcast.follow(slug);
        self.role = JamRole::Hosting {
            room,
            code: format!("{:06}", fastrand::u32(0..1_000_000)),
            addresses: addresses(port),
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
        self.broadcast.follow(None);
        self.role = JamRole::Idle;
        cx.emit(JamEvent::Stopped);
        cx.notify();
    }

    pub fn set_lead(&mut self, lead_ms: u32, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_jam_lead(lead_ms, cx));
        cx.notify();
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
