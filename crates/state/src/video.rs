//! The music video of the playing track: where it is shown, the feed from the provider, the
//! `Movie` that decodes it, and the clock that keeps its picture on the sound. Only the current
//! track ever has a movie, and only while a place shows it (or for a short grace after the last
//! one went away), so a hidden video costs no download and no decoding.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{Context, Entity, Task};
use music::video::{Clock, Movie};

use crate::{
    AppSettings, Codec, CodecState, Io, Playback, PlaybackState, Session, SessionEvent, join,
    playback::PlaybackEvent,
};

/// How long a video with no place to show it keeps loading and decoding, so flicking between
/// sidebar tabs does not restart its download each time.
const GRACE: Duration = Duration::from_secs(5);
/// The stream height a small place (the sidebar slot, the PiP window) asks for.
const SMALL: u32 = 480;
/// The stream height fullscreen asks for.
const LARGE: u32 = 1080;

/// Where the video is shown. It is shown in one place at a time; views report theirs through
/// `Video::set_place`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Place {
    /// Nowhere on screen: after `GRACE` the movie and its download stop.
    #[default]
    None,
    /// The sidebar's Queue tab.
    Slot,
    Fullscreen,
    /// The floating picture-in-picture window.
    Pip,
}

/// What a change of the playing track means for the movie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Keep,
    Drop,
}

/// What `reconcile` does about the movie, given its height, the height loading and the height
/// the place wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    /// Nothing to do.
    Keep,
    /// The movie already plays at the wanted height; drop the load of another one.
    Cancel,
    Load(u32),
}

/// The playback position as last read from `Playback`, run forward on wall time while playing.
/// It is what a `Movie` picks its frames by, read from the decoding thread.
#[derive(Clone, Default)]
struct PlaybackClock(Arc<Mutex<Snapshot>>);

struct Snapshot {
    at: Duration,
    since: Instant,
    /// Playing, as opposed to paused; a paused clock reads `None`.
    playing: bool,
    /// Whether `Playback`'s own clock moves. It is parked while playing too, from a seek until
    /// the engine reports audio from its target, and then reads `at` without advancing.
    running: bool,
}

/// A started movie with the download feeding it. Dropping it stops both.
struct Loaded {
    movie: Movie,
    /// The feed's `stream`, kept only so the download lives as long as the movie reading it.
    /// Boxed because `music` does not export the stream's type.
    _stream: Box<dyn Any>,
    height: u32,
}

/// Follows the playing track and the place a view shows its video in, and owns the one `Movie`
/// that plays there. Nothing here draws; a view reads `movie()` and `generation()` and watches
/// the movie each frame it draws it.
pub struct Video {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    codec: Entity<Codec>,
    io: Io,
    clock: PlaybackClock,
    /// The tagged id of the track this entity is following.
    track: Option<String>,
    place: Place,
    /// Whether each track asked about has a video, so a track without one is asked once.
    known: HashMap<String, bool>,
    loaded: Option<Loaded>,
    /// The height `task` is loading, if a load is in flight.
    loading: Option<u32>,
    /// The track and height whose load failed. Not tried again until either changes or a
    /// decoder arrives, since `Playback` notifies twice a second.
    failed: Option<(String, u32)>,
    /// Grows each time the movie is set or dropped, and on every new track, so a view can tell
    /// a new movie's first frame from the last one of the movie before it.
    generation: u64,
    task: Option<Task<()>>,
    /// Asks whether the current track has a video while no decoder is loaded, which is what
    /// raises the decoder prompt. Only metadata: nothing is downloaded.
    probe: Option<Task<()>>,
    grace: Option<Task<()>>,
}

impl Video {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        codec: Entity<Codec>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&playback, |this, _, cx| this.follow(cx))
            .detach();
        cx.subscribe(&playback, |this, _, event, cx| match event {
            PlaybackEvent::Seeked | PlaybackEvent::Paused => this.sync_clock(cx),
            PlaybackEvent::StartedPlayback | PlaybackEvent::EndedPlayback => {}
        })
        .detach();
        cx.observe(&settings, |this, _, cx| this.reconcile(cx))
            .detach();
        cx.observe(&codec, |this, _, cx| this.retry(cx)).detach();
        cx.subscribe(&session, |this, _, event, cx| match event {
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
            codec,
            io,
            clock: PlaybackClock::default(),
            track: None,
            place: Place::None,
            known: HashMap::new(),
            loaded: None,
            loading: None,
            failed: None,
            generation: 0,
            task: None,
            probe: None,
            grace: None,
        }
    }

    /// The movie for the current track, while the setting allows one, a decoder loaded and a
    /// place shows it (or its grace has not run out).
    pub fn movie(&self) -> Option<&Movie> {
        self.loaded.as_ref().map(|loaded| &loaded.movie)
    }

    /// Which movie `movie()` is; see the field.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the current track has a video: one is playing, loading, or the provider said so
    /// earlier. A track never asked about (the setting off, or no place yet) reads `false`.
    pub fn has_video(&self) -> bool {
        let known = self
            .track
            .as_ref()
            .and_then(|id| self.known.get(id))
            .copied();
        self.loaded.is_some() || self.loading.is_some() || known == Some(true)
    }

    pub fn place(&self) -> Place {
        self.place
    }

    /// Reports where the video is shown. `Place::None` starts the grace period; any other place
    /// cancels it, and a place needing another height reloads at that height. The old movie
    /// keeps playing until the new one has started.
    pub fn set_place(&mut self, place: Place, cx: &mut Context<Self>) {
        if place == self.place {
            return;
        }
        self.place = place;
        match place {
            Place::None => self.wait(cx),
            Place::Slot | Place::Fullscreen | Place::Pip => self.grace = None,
        }
        self.reconcile(cx);
        cx.notify();
    }

    /// Starts the grace timer; when it runs out with still no place, everything stops.
    fn wait(&mut self, cx: &mut Context<Self>) {
        let idle = Instant::now();
        self.grace = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(GRACE).await;
            this.update(cx, |this, cx| {
                this.grace = None;
                if !keep(this.place, idle.elapsed()) {
                    this.stop(cx);
                }
            })
            .ok();
        }));
    }

    /// Reads the position from `Playback` into the clock the movie runs on.
    fn sync_clock(&mut self, cx: &mut Context<Self>) {
        let playback = self.playback.read(cx);
        let playing = *playback.state() == PlaybackState::Playing;
        self.clock
            .set(playback.live_position(), playing, playback.clock_running());
    }

    /// Runs on every playback change, so the common case (same track, nothing to do) has to
    /// return before any work.
    fn follow(&mut self, cx: &mut Context<Self>) {
        self.sync_clock(cx);
        let track = self
            .playback
            .read(cx)
            .track()
            .and_then(|track| track.id.clone());
        if next(self.track.as_deref(), track.as_deref()) == Change::Drop {
            self.track = track;
            self.task = None;
            self.probe = None;
            self.loading = None;
            self.failed = None;
            if self.loaded.take().is_some() {
                log::info!("video: stopped for a new track");
            }
            self.generation += 1;
            cx.notify();
        }
        self.reconcile(cx);
    }

    /// Brings the movie in line with the setting, the place and the decoder: stops it when the
    /// setting is off, and loads one when a place wants a height nothing is playing or loading
    /// at. With no place it changes nothing; the grace timer decides. Without a decoder it only
    /// asks whether the track has a video, and only while the decoder could still be offered.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let on = settings.video();
        let cap = settings.video_max_height();
        if !on {
            self.probe = None;
            self.stop(cx);
            return;
        }
        let Some(id) = self.track.clone() else { return };
        let loaded = self.loaded.as_ref().map(|loaded| loaded.height);
        let height = match plan(loaded, self.loading, height(self.place, cap)) {
            Plan::Keep => return,
            Plan::Cancel => {
                self.settle(cx);
                return;
            }
            Plan::Load(height) => height,
        };
        let known = self.known.get(&id).copied();
        if known == Some(false) || self.failed.as_ref() == Some(&(id.clone(), height)) {
            return;
        }
        let ready = self.codec.update(cx, |codec, _| codec.ready());
        match wanted(on, ready, self.place) {
            true => self.load(id, height, cx),
            false => self.ask(id, known, cx),
        }
    }

    /// No decoder: raises the prompt for a track known to have a video, and otherwise asks the
    /// provider, from metadata alone. Asks nothing once the decoder was declined, failed or
    /// cannot exist here, since no answer could change anything.
    fn ask(&mut self, id: String, known: Option<bool>, cx: &mut Context<Self>) {
        let codec = self.codec.read(cx);
        if codec.state() != CodecState::Missing || !codec.supported() {
            return;
        }
        if known == Some(true) {
            self.codec.update(cx, |codec, cx| codec.want(cx));
            return;
        }
        if self.probe.is_some() {
            return;
        }
        let session = self.session.read(cx);
        let Some(client) = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug))
        else {
            return;
        };
        let io = self.io.clone();
        let asked = id.clone();
        self.probe = Some(cx.spawn(async move |this, cx| {
            let answer = join(io.spawn(async move { client.has_video(&asked).await })).await;
            this.update(cx, |this, cx| {
                this.probe = None;
                match answer {
                    Ok(has) => {
                        this.known.insert(id.clone(), has);
                        cx.notify();
                        if has && this.track.as_deref() == Some(id.as_str()) {
                            this.codec.update(cx, |codec, cx| codec.want(cx));
                        }
                    }
                    Err(error) => log::warn!("video: cannot ask whether {id} has one: {error:#}"),
                }
            })
            .ok();
        }));
    }

    /// The decoder arrived: try the current track again, a failed one included, since a new
    /// decoder is the one thing that could have changed its outcome.
    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.codec.update(cx, |codec, _| codec.ready()) {
            self.failed = None;
            self.reconcile(cx);
        }
    }

    /// Drops the movie, its download and any load in flight.
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.loading = None;
        if self.loaded.take().is_some() {
            log::info!("video: stopped");
            self.generation += 1;
            cx.notify();
        }
    }

    fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.known
            .retain(|id, _| music::tag::slug_of(id) != Some(slug));
        if self.track.as_deref().and_then(music::tag::slug_of) == Some(slug) {
            self.failed = None;
            self.stop(cx);
            cx.notify();
        }
    }

    /// Asks the provider for the feed at `height`, then starts the movie off the ui thread.
    /// Replaces any load in flight. Every way out of the task clears `task` and `loading`.
    fn load(&mut self, id: String, height: u32, cx: &mut Context<Self>) {
        let session = self.session.read(cx);
        let Some(client) = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug))
        else {
            return;
        };
        let io = self.io.clone();
        let clock: Arc<dyn Clock> = Arc::new(self.clock.clone());
        let asked = id.clone();
        self.loading = Some(height);
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let feed = join(io.spawn(async move { client.video(&asked, height).await })).await;
            let feed = this
                .update(cx, |this, cx| this.answered(&id, height, feed, cx))
                .ok()
                .flatten();
            let Some(feed) = feed else { return };
            // held here, so dropping this task ends the download and unblocks a pending read
            let stream = feed.stream;
            let reader = feed.reader;
            let started = join(io.spawn_blocking(move || Movie::start(reader, clock))).await;
            let stream: Box<dyn Any> = Box::new(stream);
            this.update(cx, |this, cx| this.started(id, height, started, stream, cx))
                .ok();
        }));
    }

    /// Takes the provider's answer: remembers whether the track has a video, and hands the feed
    /// on only when a movie can start from it.
    fn answered(
        &mut self,
        id: &str,
        height: u32,
        feed: anyhow::Result<Option<music::VideoFeed>>,
        cx: &mut Context<Self>,
    ) -> Option<music::VideoFeed> {
        let feed = match feed {
            Ok(feed) => feed,
            Err(error) => {
                log::warn!("video: cannot look up {id}: {error:#}");
                self.failed = Some((id.to_owned(), height));
                self.settle(cx);
                return None;
            }
        };
        self.known.insert(id.to_owned(), feed.is_some());
        let ready = self.codec.update(cx, |codec, _| codec.ready());
        // A place lost meanwhile does not stop the load: the grace timer decides that.
        let on = self.settings.read(cx).video();
        let current = self.track.as_deref() == Some(id);
        match (feed, current && on && ready) {
            (Some(feed), true) => Some(feed),
            _ => {
                self.settle(cx);
                None
            }
        }
    }

    fn started(
        &mut self,
        id: String,
        height: u32,
        started: anyhow::Result<Movie>,
        stream: Box<dyn Any>,
        cx: &mut Context<Self>,
    ) {
        self.settle(cx);
        if self.track.as_deref() != Some(id.as_str()) || !self.settings.read(cx).video() {
            return;
        }
        match started {
            Ok(movie) => {
                log::info!("video: playing {id} at {height}p");
                self.loaded = Some(Loaded {
                    movie,
                    _stream: stream,
                    height,
                });
                self.generation += 1;
                cx.notify();
            }
            Err(error) => {
                log::warn!("video: cannot play {id}: {error:#}");
                self.failed = Some((id, height));
            }
        }
    }

    /// Marks the load in flight as over.
    fn settle(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.loading = None;
        cx.notify();
    }
}

impl PlaybackClock {
    /// Restarts the clock at `at`: `None` unless `playing`, and held at `at` unless `running`.
    fn set(&self, at: Duration, playing: bool, running: bool) {
        let Ok(mut snapshot) = self.0.lock() else {
            return;
        };
        *snapshot = Snapshot {
            at,
            since: Instant::now(),
            playing,
            running,
        };
    }
}

impl Clock for PlaybackClock {
    fn now(&self) -> Option<Duration> {
        let snapshot = self.0.lock().ok()?;
        let moved = match snapshot.running {
            true => snapshot.since.elapsed(),
            false => Duration::ZERO,
        };
        snapshot.playing.then(|| snapshot.at + moved)
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            at: Duration::ZERO,
            since: Instant::now(),
            playing: false,
            running: false,
        }
    }
}

/// The stream height a place asks for, never above the user's cap. `None` for no place.
fn height(place: Place, cap: u32) -> Option<u32> {
    let wanted = match place {
        Place::None => return None,
        Place::Slot | Place::Pip => SMALL,
        Place::Fullscreen => LARGE,
    };
    Some(wanted.min(cap))
}

/// Whether a video that has been without a place for `idle` keeps running.
fn keep(place: Place, idle: Duration) -> bool {
    place != Place::None || idle < GRACE
}

/// Whether a movie may start: the setting on, a decoder loaded, and a place to show it.
fn wanted(video_on: bool, codec_ready: bool, place: Place) -> bool {
    video_on && codec_ready && place != Place::None
}

/// What to do about the movie: keep it, drop a load at another height when the movie already
/// plays at the `wanted` one, or load the `wanted` height. No place (`wanted` `None`) keeps
/// everything; the grace timer decides.
fn plan(loaded: Option<u32>, loading: Option<u32>, wanted: Option<u32>) -> Plan {
    let Some(wanted) = wanted else {
        return Plan::Keep;
    };
    match (loaded == Some(wanted), loading) {
        (true, Some(_)) => Plan::Cancel,
        (true, None) => Plan::Keep,
        (false, Some(loading)) if loading == wanted => Plan::Keep,
        (false, _) => Plan::Load(wanted),
    }
}

/// What the playing track changing from `current` to `playing` does to the movie.
fn next(current: Option<&str>, playing: Option<&str>) -> Change {
    match current == playing {
        true => Change::Keep,
        false => Change::Drop,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn height_follows_the_place_and_the_cap() {
        assert_eq!(height(Place::Slot, 1080), Some(480));
        assert_eq!(height(Place::Pip, 1080), Some(480));
        assert_eq!(height(Place::Fullscreen, 1080), Some(1080));
        assert_eq!(height(Place::Fullscreen, 720), Some(720));
        assert_eq!(height(Place::Slot, 360), Some(360));
        assert_eq!(height(Place::None, 1080), None);
    }

    #[test]
    fn video_stops_after_the_grace_period() {
        assert!(keep(Place::None, Duration::from_millis(4900)));
        assert!(!keep(Place::None, Duration::from_secs(5)));
        assert!(keep(Place::Slot, Duration::from_secs(3600)));
    }

    #[test]
    fn a_seek_resets_the_clock() {
        let clock = PlaybackClock::default();
        clock.set(Duration::from_secs(3600), true, true);
        let now = clock.now().unwrap();
        assert!(now >= Duration::from_secs(3600));
        assert!(now < Duration::from_millis(3_600_100));
    }

    #[test]
    fn a_paused_clock_reads_none() {
        let clock = PlaybackClock::default();
        clock.set(Duration::from_secs(12), false, false);
        assert_eq!(clock.now(), None);
    }

    #[test]
    fn a_parked_clock_holds_its_place() {
        let clock = PlaybackClock::default();
        clock.set(Duration::from_secs(42), true, false);
        let first = clock.now();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(first, Some(Duration::from_secs(42)));
        assert_eq!(clock.now(), first);
        clock.set(Duration::from_secs(42), true, true);
        std::thread::sleep(Duration::from_millis(5));
        assert!(clock.now().unwrap() > Duration::from_secs(42));
    }

    #[test]
    fn a_returning_place_cancels_the_other_height() {
        assert_eq!(plan(Some(480), Some(1080), Some(480)), Plan::Cancel);
        assert_eq!(plan(Some(480), None, Some(480)), Plan::Keep);
        assert_eq!(plan(Some(480), None, Some(1080)), Plan::Load(1080));
        assert_eq!(plan(Some(480), Some(1080), Some(1080)), Plan::Keep);
        assert_eq!(plan(None, Some(1080), Some(480)), Plan::Load(480));
        assert_eq!(plan(Some(480), Some(1080), None), Plan::Keep);
        assert_eq!(plan(None, None, Some(480)), Plan::Load(480));
    }

    #[test]
    fn no_codec_means_no_video() {
        assert!(!wanted(true, false, Place::Slot));
        assert!(!wanted(false, true, Place::Slot));
        assert!(!wanted(true, true, Place::None));
        assert!(wanted(true, true, Place::Fullscreen));
    }

    #[test]
    fn a_new_track_drops_the_old_movie() {
        assert_eq!(next(Some("yt:a"), Some("yt:b")), Change::Drop);
        assert_eq!(next(Some("yt:a"), None), Change::Drop);
        assert_eq!(next(None, Some("yt:a")), Change::Drop);
        assert_eq!(next(Some("yt:a"), Some("yt:a")), Change::Keep);
        assert_eq!(next(None, None), Change::Keep);
    }
}
