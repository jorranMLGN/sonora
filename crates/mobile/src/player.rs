//! The deck: one task that owns the queue and the providers' engines, takes every intent and
//! every engine event in turn, and reports what happened.
//!
//! One engine sounds at a time. Starting a track of another provider pauses the one before,
//! and an engine that loses its output is dropped and rebuilt on the next play.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use music::equalizer::{Curve, EqualizerWriter};
use music::{
    Analyzer, Measuring, Meter, PlaybackConfig, PlaybackEvent, Player, Quality, Transcode, tag,
};
use tokio::sync::mpsc;

use crate::core::{Events, Sessions};
use crate::queue::Queue;
use crate::store::{self, Store};
use crate::{Item, PlayerState, QueueSnapshot, SessionState, Status};

/// How often an engine reports where it is.
const TICK: Duration = Duration::from_millis(500);
/// How long before the end the next track is fetched.
const PRELOAD: Duration = Duration::from_secs(20);
/// How far into a track `previous` restarts it rather than going back.
const RESTART: Duration = Duration::from_secs(3);
/// How often the position is saved while playing.
const SAVE: Duration = Duration::from_secs(5);
/// How many tracks in a row may fail before playback stops.
const FAILURES: u32 = 3;

/// An intent from the app, or an event from an engine.
pub enum Command {
    Play {
        items: Vec<Item>,
        index: usize,
        origin: Option<String>,
    },
    Toggle,
    /// Plays what is paused; a remote play command.
    Resume,
    /// Pauses what plays; a remote pause command.
    Pause,
    /// The system took the audio away (a call, an alarm, Siri).
    Interrupted,
    /// The system gave the audio back; `true` when it says playback may go on.
    Resumed(bool),
    /// The output went away (headphones out) or the media services were reset.
    RouteLost,
    Next,
    Previous,
    Seek(Duration),
    Shuffle(bool),
    CycleRepeat,
    EnqueueNext(Vec<Item>),
    EnqueueLast(Vec<Item>),
    Remove(usize),
    /// The session of `slug` changed or ended: its engine goes, and with `tracks` its tracks
    /// leave the queue and its snapshots the store.
    Forget {
        slug: &'static str,
        tracks: bool,
    },
    Engine {
        slug: &'static str,
        generation: u64,
        event: PlaybackEvent,
    },
}

/// One provider's running engine. `generation` tells its events from those of an engine it
/// replaced.
struct Engine {
    player: Box<dyn Player>,
    generation: u64,
}

/// The state the deck task owns.
pub struct Deck {
    queue: Queue,
    store: Store,
    sessions: Arc<Sessions>,
    events: Arc<Events>,
    commands: mpsc::UnboundedSender<Command>,
    engines: HashMap<&'static str, Engine>,
    generations: u64,
    /// The provider whose engine has the current track, if any engine has it.
    active: Option<&'static str>,
    status: Status,
    position: Duration,
    duration: Duration,
    preloaded: Option<String>,
    failures: u32,
    /// Set when an interruption paused playback, so its end knows to resume.
    interrupted: bool,
    saved_at: Instant,
    published: Arc<Mutex<(PlayerState, QueueSnapshot)>>,
}

impl Deck {
    /// A deck holding the queue saved last time, paused where it was. Nothing is fetched until
    /// the listener presses play.
    pub fn restore(
        store: Store,
        sessions: Arc<Sessions>,
        events: Arc<Events>,
        commands: mpsc::UnboundedSender<Command>,
    ) -> Self {
        let queue: Queue = store.load(store::QUEUE).unwrap_or_default();
        let position = Duration::from_millis(store.load(store::POSITION).unwrap_or(0));
        let duration = queue.current().map_or(Duration::ZERO, |item| {
            Duration::from_millis(item.duration_ms)
        });
        let status = match queue.current() {
            Some(_) => Status::Paused,
            None => Status::Idle,
        };
        let deck = Self {
            published: Arc::new(Mutex::new((empty_state(), queue.snapshot()))),
            queue,
            store,
            sessions,
            events,
            commands,
            engines: HashMap::new(),
            generations: 0,
            active: None,
            status,
            position,
            duration,
            preloaded: None,
            failures: 0,
            interrupted: false,
            saved_at: Instant::now(),
        };
        let state = deck.state();
        if let Ok(mut published) = deck.published.lock() {
            published.0 = state;
        }
        deck
    }

    /// What was last reported, shared with the core for its synchronous getters.
    pub fn published(&self) -> Arc<Mutex<(PlayerState, QueueSnapshot)>> {
        self.published.clone()
    }

    /// Takes commands until the core goes away.
    pub async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        while let Some(command) = commands.recv().await {
            self.handle(command);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Play {
                items,
                index,
                origin,
            } => {
                if items.is_empty() {
                    self.events
                        .error("nothing in that list can be played".into());
                    return;
                }
                self.queue.replace(items, index, origin);
                self.failures = 0;
                self.queue_changed();
                self.start(Duration::ZERO, true);
            }
            Command::Toggle => {
                self.interrupted = false;
                self.toggle();
            }
            Command::Resume => {
                self.interrupted = false;
                self.resume();
            }
            Command::Pause => {
                self.interrupted = false;
                self.pause();
            }
            Command::Interrupted => {
                self.interrupted = matches!(self.status, Status::Playing | Status::Loading);
                self.pause();
            }
            Command::Resumed(should_resume) => {
                if std::mem::take(&mut self.interrupted) && should_resume {
                    self.resume();
                }
            }
            Command::RouteLost => self.route_lost(),
            Command::Next => match self.forward(false) {
                true => {
                    self.queue_changed();
                    self.start(Duration::ZERO, true);
                }
                false => self.stop(),
            },
            Command::Previous => self.previous(),
            Command::Seek(at) => self.seek(at),
            Command::Shuffle(on) => {
                self.queue.set_shuffle(on);
                self.preloaded = None;
                self.queue_changed();
                self.publish();
            }
            Command::CycleRepeat => {
                self.queue.cycle_repeat();
                self.queue_changed();
                self.publish();
            }
            Command::EnqueueNext(items) => {
                let was_empty = self.queue.current().is_none();
                self.queue.insert_next(items);
                self.preloaded = None;
                self.queue_changed();
                if was_empty {
                    self.start(Duration::ZERO, false);
                }
            }
            Command::EnqueueLast(items) => {
                let was_empty = self.queue.current().is_none();
                self.queue.push(items);
                self.queue_changed();
                if was_empty {
                    self.start(Duration::ZERO, false);
                }
            }
            Command::Remove(index) => {
                if self.queue.remove(index) {
                    self.preloaded = None;
                    self.queue_changed();
                }
            }
            Command::Forget { slug, tracks } => self.forget(slug, tracks),
            Command::Engine {
                slug,
                generation,
                event,
            } => self.engine_event(slug, generation, event),
        }
    }

    /// Loads the current track into its provider's engine at `at`, playing it or leaving it
    /// paused there.
    fn start(&mut self, at: Duration, play: bool) {
        let Some(item) = self.queue.current().cloned() else {
            self.status = Status::Idle;
            self.publish();
            return;
        };
        if !item.playable {
            // Started on a track the provider will not play: go on to the next one that it will.
            match self.forward(false) {
                true => {
                    self.queue_changed();
                    self.start(Duration::ZERO, play);
                }
                false => self.stop(),
            }
            return;
        }
        self.position = at;
        self.duration = Duration::from_millis(item.duration_ms);
        self.preloaded = None;
        self.save_position();
        let Some(slug) = tag::slug_of(&item.id).and_then(slug) else {
            self.failed(&item, "it names no provider this app knows");
            return;
        };
        if let Some(active) = self.active
            && active != slug
            && let Some(engine) = self.engines.get(active)
        {
            engine.player.pause();
        }
        if self.sessions.state(slug) == SessionState::Connecting {
            // The session is still being restored: stay put rather than skip, and let the
            // listener press play again in a moment.
            self.status = Status::Paused;
            self.publish();
            self.events
                .error(format!("still connecting to {slug}, try again in a moment"));
            return;
        }
        let Some(engine) = self.engine(slug) else {
            self.failed(&item, "its provider is not signed in");
            return;
        };
        let bare = tag::untag(&item.id);
        let loaded = match play {
            true => engine.player.load(bare, at, false),
            false => engine.player.load_paused_at(bare, at),
        };
        if let Err(error) = loaded {
            self.failed(&item, &format!("{error:#}"));
            return;
        }
        self.active = Some(slug);
        self.status = match play {
            true => Status::Loading,
            false => Status::Paused,
        };
        self.publish();
    }

    fn toggle(&mut self) {
        match self.status {
            Status::Playing | Status::Loading => self.pause(),
            Status::Paused | Status::Idle => self.resume(),
        }
    }

    /// Pauses what plays or loads; anything else stays as it is.
    fn pause(&mut self) {
        if !matches!(self.status, Status::Playing | Status::Loading) {
            return;
        }
        if let Some(engine) = self.current_engine() {
            engine.player.pause();
        }
        self.status = Status::Paused;
        self.save_position();
        self.publish();
    }

    /// Plays what is paused, loading it first when no engine holds it: a restored queue, a lost
    /// output or a finished queue.
    fn resume(&mut self) {
        if matches!(self.status, Status::Playing | Status::Loading) {
            return;
        }
        match self.current_engine() {
            Some(engine) if self.status == Status::Paused => {
                engine.player.play();
                self.status = Status::Loading;
                self.publish();
            }
            Some(_) => {}
            None => {
                if self.queue.current().is_some() {
                    let at = self.position;
                    self.start(at, true);
                }
            }
        }
    }

    /// Pauses and drops every engine, so the next play opens the output that is there now.
    fn route_lost(&mut self) {
        self.interrupted = false;
        if let Some(engine) = self.current_engine() {
            engine.player.pause();
        }
        if !self.engines.is_empty() {
            log::info!("player: the audio route was lost, dropping every engine");
        }
        self.engines.clear();
        self.active = None;
        self.preloaded = None;
        if self.status != Status::Idle {
            self.status = match self.queue.current() {
                Some(_) => Status::Paused,
                None => Status::Idle,
            };
        }
        self.save_position();
        self.publish();
    }

    /// Drops the engine of `slug`, built on a session that is gone, so the next play builds one
    /// on the session there is now. With `tracks`, also takes the provider's tracks out of the
    /// queue; a current track that goes leaves the next one kept up, paused at its start.
    fn forget(&mut self, slug: &'static str, tracks: bool) {
        if let Some(engine) = self.engines.remove(slug) {
            engine.player.pause();
        }
        if self.active == Some(slug) {
            self.active = None;
            self.preloaded = None;
            if matches!(self.status, Status::Playing | Status::Loading) {
                self.status = Status::Paused;
            }
            self.save_position();
        }
        if tracks {
            let current = self.queue.current().map(|item| item.id.clone());
            if self.queue.drop_provider(slug) {
                self.preloaded = None;
                if self.queue.current().map(|item| item.id.as_str()) != current.as_deref() {
                    self.position = Duration::ZERO;
                    self.duration = self.queue.current().map_or(Duration::ZERO, |item| {
                        Duration::from_millis(item.duration_ms)
                    });
                    self.status = match self.queue.current() {
                        Some(_) => Status::Paused,
                        None => Status::Idle,
                    };
                    self.save_position();
                }
                self.queue_changed();
            }
        }
        self.publish();
    }

    fn previous(&mut self) {
        if self.position > RESTART || self.queue.back().is_none() {
            self.seek(Duration::ZERO);
            return;
        }
        // Pass back over tracks the provider cannot play.
        for _ in 0..self.queue.count() {
            if self.queue.current().is_none_or(|item| item.playable) || self.queue.back().is_none()
            {
                break;
            }
        }
        self.queue_changed();
        self.start(Duration::ZERO, true);
    }

    fn seek(&mut self, at: Duration) {
        self.position = at;
        match self.current_engine() {
            Some(engine) => engine.player.seek(at),
            None => self.save_position(),
        }
        self.events.position(at.as_millis() as u64);
    }

    /// Pauses at the end of the queue, keeping the last track up.
    fn stop(&mut self) {
        if let Some(engine) = self.current_engine() {
            engine.player.pause();
        }
        self.active = None;
        self.status = match self.queue.current() {
            Some(_) => Status::Paused,
            None => Status::Idle,
        };
        self.position = Duration::ZERO;
        self.save_position();
        self.publish();
    }

    fn engine_event(&mut self, slug: &'static str, generation: u64, event: PlaybackEvent) {
        let current = self
            .engines
            .get(slug)
            .is_some_and(|engine| engine.generation == generation);
        if !current || self.active != Some(slug) {
            return;
        }
        let playing = self
            .queue
            .current()
            .map(|item| tag::untag(&item.id).to_owned());
        if let Some(id) = event.id()
            && Some(id) != playing.as_deref()
        {
            return;
        }
        match event {
            PlaybackEvent::Signal { .. } => {}
            PlaybackEvent::Loading { .. } => {
                self.status = Status::Loading;
                self.publish();
            }
            PlaybackEvent::Playing { at, .. } | PlaybackEvent::Seeked { at, .. } => {
                self.failures = 0;
                self.position = at;
                if self.status != Status::Playing {
                    self.status = Status::Playing;
                    self.publish();
                }
                self.events.position(at.as_millis() as u64);
            }
            PlaybackEvent::Paused { at, .. } => {
                self.position = at;
                self.status = Status::Paused;
                self.save_position();
                self.publish();
            }
            PlaybackEvent::Position { at, .. } => {
                self.position = at;
                self.events.position(at.as_millis() as u64);
                self.preload();
                if self.saved_at.elapsed() >= SAVE {
                    self.save_position();
                }
            }
            PlaybackEvent::Length { duration, .. } => {
                self.duration = duration;
                self.publish();
            }
            PlaybackEvent::Ended { .. } => match self.forward(true) {
                true => {
                    self.queue_changed();
                    self.start(Duration::ZERO, true);
                }
                false => self.stop(),
            },
            PlaybackEvent::Unavailable { .. }
            | PlaybackEvent::Throttled { .. }
            | PlaybackEvent::Refused
            | PlaybackEvent::Gated => {
                // Each reason says something different about what went wrong, so name it.
                let why = match event {
                    PlaybackEvent::Refused => "the provider refused the audio key",
                    PlaybackEvent::Throttled { .. } => "the provider is throttling playback",
                    PlaybackEvent::Gated => "the provider gates it behind a check",
                    _ => "the provider has no playable file for it",
                };
                if let Some(item) = self.queue.current().cloned() {
                    self.failed(&item, why);
                }
            }
            PlaybackEvent::OutputChanged => {
                log::info!("player: the audio output changed, dropping the {slug} engine");
                self.engines.remove(slug);
                self.active = None;
                self.status = Status::Paused;
                self.save_position();
                self.publish();
            }
            PlaybackEvent::Downloaded { .. } => {}
        }
    }

    /// Skips a track that cannot be played, and stops after `FAILURES` in a row.
    fn failed(&mut self, item: &Item, why: &str) {
        self.failures += 1;
        self.events
            .error(format!("cannot play {}: {why}", item.title));
        if self.failures >= FAILURES {
            self.failures = 0;
            self.stop();
            return;
        }
        match self.forward(false) {
            true => {
                self.queue_changed();
                self.start(Duration::ZERO, true);
            }
            false => self.stop(),
        }
    }

    /// Moves to the next track the provider can play, passing over unplayable ones. False when
    /// none is left; the queue may then rest on an unplayable track.
    fn forward(&mut self, ended: bool) -> bool {
        if self.queue.advance(ended).is_none() {
            return false;
        }
        for _ in 0..self.queue.count() {
            match self.queue.current() {
                Some(item) if item.playable => return true,
                Some(_) => {
                    if self.queue.advance(false).is_none() {
                        return false;
                    }
                }
                None => return false,
            }
        }
        false
    }

    /// Fetches the next track once the current one nears its end, on its own provider's engine.
    fn preload(&mut self) {
        if self.duration.is_zero() || self.duration.saturating_sub(self.position) > PRELOAD {
            return;
        }
        let Some(next) = self.queue.upcoming().map(|item| item.id.clone()) else {
            return;
        };
        if self.preloaded.as_deref() == Some(next.as_str()) {
            return;
        }
        self.preloaded = Some(next.clone());
        let Some(slug) = tag::slug_of(&next).and_then(slug) else {
            return;
        };
        let Some(engine) = self.engine(slug) else {
            return;
        };
        if let Err(error) = engine.player.preload(tag::untag(&next), true) {
            log::warn!("player: cannot preload {next}: {error:#}");
        }
    }

    /// The engine of `slug`, started on first use. `None` when the provider has no session.
    fn engine(&mut self, slug: &'static str) -> Option<&Engine> {
        if !self.engines.contains_key(slug) {
            let session = self.sessions.session(slug)?;
            self.generations += 1;
            let generation = self.generations;
            let (player, mut events) = session.playback.start(config(), None);
            let commands = self.commands.clone();
            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    let command = Command::Engine {
                        slug,
                        generation,
                        event,
                    };
                    if commands.send(command).is_err() {
                        break;
                    }
                }
                log::debug!("player: the {slug} engine stopped reporting");
            });
            self.engines.insert(slug, Engine { player, generation });
        }
        self.engines.get(slug)
    }

    /// The engine holding the current track, if one does.
    fn current_engine(&self) -> Option<&Engine> {
        self.engines.get(self.active?)
    }

    fn state(&self) -> PlayerState {
        PlayerState {
            track: self.queue.current().cloned(),
            status: self.status,
            position_ms: self.position.as_millis() as u64,
            duration_ms: self.duration.as_millis() as u64,
            shuffle: self.queue.shuffle(),
            repeat: self.queue.repeat(),
        }
    }

    /// Reports the player state, unless nothing in it changed.
    fn publish(&mut self) {
        let state = self.state();
        if let Ok(mut published) = self.published.lock() {
            if published.0 == state {
                return;
            }
            published.0 = state.clone();
        }
        self.events.state(state);
    }

    /// Saves and reports the queue after any change to it.
    fn queue_changed(&mut self) {
        self.store.save(store::QUEUE, &self.queue);
        let snapshot = self.queue.snapshot();
        if let Ok(mut published) = self.published.lock() {
            published.1 = snapshot.clone();
        }
        self.events.queue(snapshot);
    }

    fn save_position(&mut self) {
        self.saved_at = Instant::now();
        self.store
            .save(store::POSITION, &(self.position.as_millis() as u64));
    }
}

fn config() -> PlaybackConfig {
    PlaybackConfig {
        normalisation: true,
        gapless: false,
        position_interval: TICK,
        gain: 1.0,
        equalizer: EqualizerWriter::new(false, Curve::bands(&[0.; 10])).reader(),
        // The app draws no waveform, so a finished track is never decoded a second time.
        measuring: Measuring::new(false),
        transcode: Transcode::default(),
        quality: Quality::default(),
        meter: Meter::default(),
        analyzer: Analyzer::default(),
        vault: None,
    }
}

/// The state reported before the deck has said anything.
fn empty_state() -> PlayerState {
    PlayerState {
        track: None,
        status: Status::Idle,
        position_ms: 0,
        duration_ms: 0,
        shuffle: false,
        repeat: crate::Repeat::Off,
    }
}

/// The static slug of a known provider, so engines can be keyed without allocating.
fn slug(name: &str) -> Option<&'static str> {
    tag::SLUGS.iter().copied().find(|known| *known == name)
}
