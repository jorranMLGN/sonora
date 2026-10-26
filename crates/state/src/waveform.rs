//! The waveform of the playing track, for the seek bar to draw. It comes from the provider when
//! the provider publishes one (SoundCloud), and otherwise from the engine, which measures a
//! track from its own audio once the whole of it has arrived, unless measuring is switched off.

use std::collections::{HashMap, HashSet, VecDeque};

use gpui::{App, Context, Entity, Task};
use music::Waveform;

use crate::{AppSettings, Io, Playback, Session, SessionEvent, join};

/// How many tracks' waveforms are held. Each is at most `music::WAVEFORM_BUCKETS` floats, so a
/// full cache is about a hundred kilobytes.
const KEPT: usize = 48;

/// Follows the playing track and holds its waveform once one is known. Nothing here draws; a
/// view reads `current()`.
pub struct Waveforms {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    /// The tagged id of the track this entity is following.
    track: Option<String>,
    known: HashMap<String, Waveform>,
    /// The order `known` was filled in, oldest first, so the oldest is the one dropped.
    order: VecDeque<String>,
    /// Tracks the provider was asked about and had nothing for, or failed on. They are not
    /// asked again; the engine's measurement can still fill them in.
    asked: HashSet<String>,
    /// The tracks in `known` whose waveform the engine measured rather than the provider
    /// published. They are hidden while measuring is switched off.
    measured: HashSet<String>,
    io: Io,
    task: Option<Task<()>>,
}

impl Waveforms {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&playback, |this, _, cx| this.follow(cx))
            .detach();
        cx.observe(&settings, |this, _, cx| this.follow(cx))
            .detach();
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::WentOffline => {}
            SessionEvent::SignedOut(slug) => this.forget(slug, cx),
            SessionEvent::SignedIn(_)
            | SessionEvent::Reconnected(_)
            | SessionEvent::LocalChanged => {}
        })
        .detach();
        Self {
            session,
            playback,
            settings,
            track: None,
            known: HashMap::new(),
            order: VecDeque::new(),
            asked: HashSet::new(),
            measured: HashSet::new(),
            io,
            task: None,
        }
    }

    /// The playing track's waveform, while the setting is on and one is known.
    pub fn current(&self, cx: &App) -> Option<&Waveform> {
        if !self.settings.read(cx).waveform() {
            return None;
        }
        let track = self.track.as_deref()?;
        self.usable(track, cx).then(|| self.known.get(track))?
    }

    /// Whether the waveform held for `id` may be shown: one the engine measured only while
    /// measuring is on.
    fn usable(&self, id: &str, cx: &App) -> bool {
        self.known.contains_key(id)
            && (!self.measured.contains(id) || self.settings.read(cx).measure_waveforms())
    }

    /// Runs on every playback and settings change, so the common case (same track, already
    /// known) has to return before any work.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let track = self
            .playback
            .read(cx)
            .track()
            .and_then(|track| track.id.clone());
        if track != self.track {
            self.task = None;
            self.track = track.clone();
            cx.notify();
        }
        let Some(id) = track else { return };
        if !self.settings.read(cx).waveform() || self.usable(&id, cx) {
            return;
        }
        if self.settings.read(cx).measure_waveforms()
            && let Some(measured) = self.playback.read(cx).measured(&id)
        {
            self.task = None;
            self.keep(id, measured, true);
            cx.notify();
            return;
        }
        if self.task.is_none() && !self.asked.contains(&id) {
            self.ask(id, cx);
        }
    }

    /// Asks the track's provider for the waveform it publishes. Every way out clears `task`.
    fn ask(&mut self, id: String, cx: &mut Context<Self>) {
        let session = self.session.read(cx);
        let Some(client) = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug))
        else {
            return;
        };
        let io = self.io.clone();
        let wanted = id.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let answer = join(io.spawn(async move { client.waveform(&wanted).await })).await;
            this.update(cx, |this, cx| {
                this.task = None;
                match answer {
                    Ok(Some(waveform)) => {
                        this.keep(id, waveform, false);
                        cx.notify();
                    }
                    Ok(None) => {
                        this.asked.insert(id);
                    }
                    Err(error) => {
                        log::warn!("waveform: cannot fetch {id}: {error:#}");
                        this.asked.insert(id);
                    }
                }
            })
            .ok();
        }));
    }

    /// Holds a waveform for `id`, `measured` when the engine made it rather than the provider.
    fn keep(&mut self, id: String, waveform: Waveform, measured: bool) {
        match measured {
            true => self.measured.insert(id.clone()),
            false => self.measured.remove(&id),
        };
        if self.known.insert(id.clone(), waveform).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > KEPT {
            if let Some(oldest) = self.order.pop_front() {
                self.known.remove(&oldest);
                self.measured.remove(&oldest);
            }
        }
    }

    fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        let theirs = |id: &String| music::tag::slug_of(id) == Some(slug);
        self.known.retain(|id, _| !theirs(id));
        self.order.retain(|id| !theirs(id));
        self.asked.retain(|id| !theirs(id));
        self.measured.retain(|id| !theirs(id));
        if self.track.as_ref().is_some_and(theirs) {
            self.task = None;
        }
        cx.notify();
    }
}
