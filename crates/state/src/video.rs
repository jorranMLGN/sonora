//! The music video of the playing track: where it is shown, the feed from the provider, the
//! `Movie` that decodes it, and the clock that keeps its picture on the sound. Only the current
//! track ever has a movie, and only while a place shows it (or for a short grace after the last
//! one went away), so a hidden video costs no download and no decoding.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{Context, Entity, Task};
use music::video::{Clock, Movie};

use crate::{
    AppSettings, Codec, Io, Playback, PlaybackState, Session, SessionEvent, join,
    playback::PlaybackEvent,
};

/// How long a video with no place to show it keeps loading and decoding, so flicking between
/// sidebar tabs does not restart its download each time.
const GRACE: Duration = Duration::from_secs(5);
/// The stream height a small place (the sidebar slot, the PiP window) asks for.
const SMALL: u32 = 480;
/// The stream height fullscreen asks for.
const LARGE: u32 = 1080;
/// How far behind the clock a reloaded movie's picture may be and still count as caught up:
/// about one frame at 24 fps, plus the poll's own lag.
const CAUGHT_UP: Duration = Duration::from_millis(60);
/// How long, in playing time, a reloaded movie may take to catch up before it is shown anyway,
/// so two movies never decode side by side for long.
const OVERLAP: Duration = Duration::from_secs(3);
/// How often a reloaded movie is checked for having caught up.
const POLL: Duration = Duration::from_millis(16);

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

/// A started movie with the download feeding it. Dropping it stops the movie; the download
/// stops once the movie's decoding thread has exited and let go of its reader.
struct Loaded {
    movie: Movie,
    /// The feed's download, kept so it lives as long as the movie reading it, and asked
    /// whether it broke, which leaves the movie unable to read on.
    download: music::VideoDownload,
    height: u32,
}

/// A movie reloaded at another height for the same track, decoding beside the one on screen
/// until it has caught up with the clock. Dropping it stops the movie and the poll with it.
struct Pending {
    loaded: Loaded,
    _poll: Task<()>,
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
    /// A reload at another height still catching up; `loaded` stays on screen until it has.
    pending: Option<Pending>,
    /// The height `task` is loading, if a load is in flight.
    loading: Option<u32>,
    /// The track and height whose load failed, or whose movie's download broke mid-play. Not
    /// tried again until either changes, a decoder arrives, a place comes into view or the
    /// video is switched off and on, since `Playback` notifies twice a second.
    failed: Option<(String, u32)>,
    /// Grows each time the movie is set or dropped, and on every new track, so a view can tell
    /// a new movie's first frame from the last one of the movie before it.
    generation: u64,
    task: Option<Task<()>>,
    /// Asks whether the current track has a video whenever no load will answer that: no place
    /// shows the video, or no decoder is loaded (which is what raises the decoder prompt). Only
    /// metadata: nothing is downloaded.
    probe: Option<Task<()>>,
    /// The track whose probe failed. It is not asked again until the track, the video setting
    /// or the decoder's state changes, since `Playback` notifies twice a second.
    probe_failed: Option<String>,
    grace: Option<Task<()>>,
    /// Whether the track before this one had a video (`has_video`, not merely offered). Keeps the player bar's video
    /// buttons up while the new track is still being asked about, so a run of videos does not
    /// blink between tracks.
    carried: bool,
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
            codec,
            io,
            clock: PlaybackClock::default(),
            track: None,
            place: Place::None,
            known: HashMap::new(),
            loaded: None,
            pending: None,
            loading: None,
            failed: None,
            generation: 0,
            task: None,
            probe: None,
            probe_failed: None,
            grace: None,
            carried: false,
        }
    }

    /// The movie for the current track, while the setting allows one, a decoder loaded and a
    /// place shows it (or its grace has not run out).
    pub fn movie(&self) -> Option<&Movie> {
        self.loaded.as_ref().map(|loaded| &loaded.movie)
    }

    /// Says a surface draws the video `width` pixels wide this frame. Watches the movie on
    /// screen and a reload catching up behind it, so the reload decodes before it is shown.
    pub fn watch(&self, width: u32) {
        if let Some(loaded) = &self.loaded {
            loaded.movie.watch(width);
        }
        if let Some(pending) = &self.pending {
            pending.loaded.movie.watch(width);
        }
    }

    /// Which movie `movie()` is; see the field.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the current track has a video: one is playing, one is catching up at another
    /// height, or the provider said so. A load still in flight does not count, since the track
    /// may turn out to have none, and a track not yet answered (the setting off, or the probe
    /// still out) reads `false`.
    pub fn has_video(&self) -> bool {
        has(self.loaded.is_some(), self.pending.is_some(), self.known())
    }

    /// Whether the provider has said the current track has no video. Unlike `!has_video()`, a
    /// track not yet asked about reads `false`, so a window waiting on the answer stays.
    pub fn lacks_video(&self) -> bool {
        self.known() == Some(false)
    }

    /// Whether a track is playing that is not known to lack a video, so a place may be offered
    /// for it before the provider has answered.
    pub fn may_have_video(&self) -> bool {
        may_have(self.track.is_some(), self.known())
    }

    /// Whether the player bar offers the video toggle and the PiP for the current track: it has
    /// a video, or it is still being asked about and the track before it had one. A track not
    /// yet answered after one without a video is not offered, so it never flashes.
    pub fn offers_video(&self) -> bool {
        offers(
            self.track.is_some(),
            self.has_video(),
            self.known(),
            self.carried,
        )
    }

    /// What the provider said about the current track's video, if it was asked.
    fn known(&self) -> Option<bool> {
        self.track
            .as_ref()
            .and_then(|id| self.known.get(id))
            .copied()
    }

    pub fn place(&self) -> Place {
        self.place
    }

    /// Reports where the video is shown. `Place::None` starts the grace period; any other place
    /// cancels it, and a place needing another height reloads at that height. The old movie
    /// stays on screen until the new one has caught up with the clock.
    ///
    /// A place coming into view also tries a failed or broken movie once more.
    pub fn set_place(&mut self, place: Place, cx: &mut Context<Self>) {
        if place == self.place {
            return;
        }
        // a movie that broke unseen counts as failed before this change forgives it
        self.drop_broken(cx);
        if retries(self.place, place) {
            self.failed = None;
        }
        self.place = place;
        match place {
            Place::None => self.wait(cx),
            Place::Slot | Place::Fullscreen | Place::Pip => self.grace = None,
        }
        self.reconcile(cx);
        cx.notify();
    }

    /// Says `from` lets go of the video, as a closing PiP window, mini player or fullscreen
    /// view does. A video placed there goes to no place, and a video some other place has taken
    /// since stays. Every observer hears of it either way, so the sidebar slot claims the video
    /// when it is on screen, and nothing loads for one that is hidden.
    pub fn release(&mut self, from: Place, cx: &mut Context<Self>) {
        if let Some(place) = released(self.place, from) {
            self.set_place(place, cx);
        }
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
            // from the answer alone, so a track never answered cannot pass the offer on
            self.carried = self.has_video();
            self.track = track;
            self.task = None;
            self.probe = None;
            self.probe_failed = None;
            self.loading = None;
            self.failed = None;
            self.pending = None;
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
    /// at. With no place it changes nothing but asks, from metadata alone, whether the track has
    /// a video, so the player bar knows whether to offer one; the grace timer decides the rest.
    /// Without a decoder it only asks.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let on = settings.video();
        let cap = settings.video_max_height();
        if !on {
            self.probe = None;
            self.probe_failed = None;
            // switching the video back on is a fresh try
            self.failed = None;
            self.stop(cx);
            return;
        }
        self.drop_broken(cx);
        let Some(id) = self.track.clone() else { return };
        let loaded = self.loaded.as_ref().map(|loaded| loaded.height);
        // a reload catching up counts as in flight
        let pending = self.pending.as_ref().map(|pending| pending.loaded.height);
        let Some(wanted_height) = height(self.place, cap) else {
            self.probe(id, cx);
            return;
        };
        let height = match plan(loaded, self.loading.or(pending), Some(wanted_height)) {
            Plan::Keep => return,
            Plan::Cancel => {
                self.pending = None;
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

    /// No decoder while a place shows the video: raises the prompt for a track known to have
    /// one, and otherwise asks the provider.
    fn ask(&mut self, id: String, known: Option<bool>, cx: &mut Context<Self>) {
        match known {
            Some(true) => self.codec.update(cx, |codec, cx| codec.want(cx)),
            _ => self.probe(id, cx),
        }
    }

    /// Asks the provider, from metadata alone, whether `id` has a video, unless that is known,
    /// already being asked, about to be answered by a load in flight, or failed for this track.
    /// A yes raises the decoder prompt only while a place shows the video, as a load would.
    fn probe(&mut self, id: String, cx: &mut Context<Self>) {
        let known = self.known.get(&id).copied();
        let failed = self.probe_failed.as_deref() == Some(id.as_str());
        if !should_probe(known, self.probe.is_some(), self.loading.is_some(), failed) {
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
                        let current = this.track.as_deref() == Some(id.as_str());
                        if has && current && this.place != Place::None {
                            this.codec.update(cx, |codec, cx| codec.want(cx));
                        }
                    }
                    Err(error) => {
                        log::warn!("video: cannot ask whether {id} has one: {error:#}");
                        this.probe_failed = Some(id.clone());
                    }
                }
            })
            .ok();
        }));
    }

    /// The decoder arrived: try the current track again, a failed one included, since a new
    /// decoder is the one thing that could have changed its outcome.
    /// Any change of the decoder's state also lets a failed probe be asked again.
    fn retry(&mut self, cx: &mut Context<Self>) {
        self.probe_failed = None;
        if self.codec.update(cx, |codec, _| codec.ready()) {
            self.failed = None;
            self.reconcile(cx);
        }
    }

    /// Drops a movie, or a reload catching up, whose download broke: it cannot read on, and
    /// left on screen it would hold its last picture and keep `plan` from loading another.
    /// Marks its height failed so it is not reloaded on every playback tick; `has_video` stays
    /// true, since the track still has a video.
    fn drop_broken(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.track.clone() else { return };
        if let Some(pending) = self
            .pending
            .take_if(|pending| pending.loaded.download.broken())
        {
            log::warn!(
                "video: the {}p download of {id} broke, dropping its reload",
                pending.loaded.height
            );
            self.failed = Some((id.clone(), pending.loaded.height));
        }
        if let Some(loaded) = self.loaded.take_if(|loaded| loaded.download.broken()) {
            log::warn!(
                "video: the {}p download of {id} broke, dropping the movie",
                loaded.height
            );
            self.failed = Some((id, loaded.height));
            self.generation += 1;
            cx.notify();
        }
    }

    /// Drops the movie, its download, any load in flight and any reload catching up.
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.loading = None;
        self.pending = None;
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
    /// Replaces any load in flight and any reload catching up. Every way out of the task clears
    /// `task` and `loading`.
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
        self.pending = None;
        self.loading = Some(height);
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let feed = join(io.spawn(async move { client.video(&asked, height).await })).await;
            let feed = this
                .update(cx, |this, cx| this.answered(&id, height, feed, cx))
                .ok()
                .flatten();
            let Some(feed) = feed else { return };
            // kept for `Loaded`, which asks it whether the download broke; dropping this task
            // does not end the download until `Movie::start` returns and lets go of the reader
            let stream = feed.stream;
            let reader = feed.reader;
            let started = join(io.spawn_blocking(move || Movie::start(reader, clock))).await;
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
        download: music::VideoDownload,
        cx: &mut Context<Self>,
    ) {
        self.settle(cx);
        if self.track.as_deref() != Some(id.as_str()) || !self.settings.read(cx).video() {
            return;
        }
        match started {
            Ok(movie) => {
                let loaded = Loaded {
                    movie,
                    download,
                    height,
                };
                match self.loaded.is_some() {
                    // a reload: the movie on screen stays until this one catches up
                    true => {
                        let poll = self.catch_up(cx);
                        self.pending = Some(Pending {
                            loaded,
                            _poll: poll,
                        });
                    }
                    false => {
                        log::info!("video: playing {id} at {height}p");
                        self.loaded = Some(loaded);
                        self.generation += 1;
                        cx.notify();
                    }
                }
            }
            Err(error) => {
                log::warn!("video: cannot play {id}: {error:#}");
                self.failed = Some((id, height));
            }
        }
    }

    /// Polls the pending reload until it has caught up with the clock, or has been given
    /// `OVERLAP` of playing time, then puts it on screen in place of the movie there. A paused
    /// clock decodes nothing, so time paused does not count against it. Ends with the pending
    /// reload, which owns the returned task.
    fn catch_up(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let clock = self.clock.clone();
        cx.spawn(async move |this, cx| {
            let mut waited = Duration::ZERO;
            loop {
                let tick = Instant::now();
                cx.background_executor().timer(POLL).await;
                let now = clock.now();
                if now.is_some() {
                    waited += tick.elapsed();
                }
                let done = this
                    .update(cx, |this, cx| {
                        let Some(pending) = &this.pending else {
                            return true;
                        };
                        let pts = pending.loaded.movie.pts();
                        if !swap_now(pts, now, waited) {
                            return false;
                        }
                        this.promote(waited, cx);
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
    }

    /// Puts the pending reload on screen and drops the movie it replaces.
    fn promote(&mut self, waited: Duration, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let height = pending.loaded.height;
        match waited >= OVERLAP {
            true => log::info!("video: switched to {height}p before it caught up"),
            false => log::info!("video: switched to {height}p after {waited:?}"),
        }
        self.loaded = Some(pending.loaded);
        self.generation += 1;
        cx.notify();
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

/// Whether moving the video from `from` to `to` tries a failed movie again: only a place
/// coming into view does, once per change, so a movie that keeps breaking is not reloaded in
/// a loop.
fn retries(from: Place, to: Place) -> bool {
    to != Place::None && to != from
}

/// Whether the track has a video, given a movie on screen (`loaded`), one catching up
/// (`pending`) and the provider's answer (`known`).
fn has(loaded: bool, pending: bool, known: Option<bool>) -> bool {
    loaded || pending || known == Some(true)
}

/// Whether the player bar offers the video for the current `track`: it `has` one, or the
/// provider has not answered yet (`known`) and the track before had one (`carried`).
fn offers(track: bool, has: bool, known: Option<bool>, carried: bool) -> bool {
    track && (has || (known.is_none() && carried))
}

/// Whether a place may be offered for the video: a track is playing and the provider has not
/// said it has none.
fn may_have(track: bool, known: Option<bool>) -> bool {
    track && known != Some(false)
}

/// The place the video moves to when `from` lets go of it while it is at `current`: no place,
/// only if `from` still holds it.
fn released(current: Place, from: Place) -> Option<Place> {
    (current == from && current != Place::None).then_some(Place::None)
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

/// Whether a reload whose newest picture plays at `pts` may replace the movie on screen, the
/// clock reading `now` (`None` while paused) after `waited` of playing time. It may once its
/// picture is within `CAUGHT_UP` of the clock, or once it has had `OVERLAP` to get there.
fn swap_now(pts: Option<Duration>, now: Option<Duration>, waited: Duration) -> bool {
    if waited >= OVERLAP {
        return true;
    }
    match (pts, now) {
        (Some(pts), Some(now)) => now.saturating_sub(pts) <= CAUGHT_UP,
        _ => false,
    }
}

/// Whether to ask the provider if the track has a video: for a track not yet answered, with no
/// probe in flight, no load in flight (`loading`, whose answer counts too), and not again after
/// one failed for this track.
fn should_probe(known: Option<bool>, probing: bool, loading: bool, failed: bool) -> bool {
    known.is_none() && !probing && !loading && !failed
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
    fn a_reload_waits_until_it_has_caught_up() {
        let at = Duration::from_secs(30);
        let short = Duration::from_millis(500);
        // no picture yet, or one from the keyframe well before the clock
        assert!(!swap_now(None, Some(at), short));
        assert!(!swap_now(
            Some(at - Duration::from_millis(400)),
            Some(at),
            short
        ));
        // within about a frame of the clock, or ahead of it
        assert!(swap_now(
            Some(at - Duration::from_millis(40)),
            Some(at),
            short
        ));
        assert!(swap_now(
            Some(at + Duration::from_millis(10)),
            Some(at),
            short
        ));
        // paused: nothing decodes, so it waits
        assert!(!swap_now(Some(at - Duration::from_secs(1)), None, short));
    }

    #[test]
    fn a_reload_that_never_catches_up_is_shown_anyway() {
        let at = Duration::from_secs(30);
        assert!(swap_now(None, Some(at), OVERLAP));
        assert!(swap_now(
            Some(at - Duration::from_secs(2)),
            Some(at),
            OVERLAP
        ));
        assert!(swap_now(None, None, OVERLAP + Duration::from_millis(1)));
    }

    #[test]
    fn a_failed_probe_is_not_repeated() {
        assert!(should_probe(None, false, false, false));
        assert!(!should_probe(None, false, false, true));
        assert!(!should_probe(None, true, false, false));
        assert!(!should_probe(Some(true), false, false, false));
        assert!(!should_probe(Some(false), false, false, false));
    }

    #[test]
    fn a_load_in_flight_answers_in_place_of_a_probe() {
        assert!(!should_probe(None, false, true, false));
    }

    #[test]
    fn the_player_bar_offers_video_once_the_provider_said_so() {
        // a track not yet answered is not offered on its own, so a track without one never flashes
        assert!(!offers(true, false, None, false));
        assert!(offers(true, true, Some(true), false));
        assert!(!offers(true, false, Some(false), false));
        assert!(!offers(false, false, None, true));
    }

    #[test]
    fn a_video_carries_over_until_the_next_track_answers() {
        // after a track with a video the offer stays up while the next one is still asked about
        assert!(offers(true, false, None, true));
        assert!(!offers(true, false, Some(false), true));
        assert!(offers(true, true, Some(true), true));
    }

    #[test]
    fn an_unanswered_track_does_not_pass_the_offer_on() {
        // the carry is the answer alone, so a run of tracks never answered ends the offer
        let first = has(true, false, Some(true));
        assert!(offers(true, false, None, first));
        let second = has(false, false, None);
        assert!(!offers(true, false, None, second));
    }

    #[test]
    fn no_codec_means_no_video() {
        assert!(!wanted(true, false, Place::Slot));
        assert!(!wanted(false, true, Place::Slot));
        assert!(!wanted(true, true, Place::None));
        assert!(wanted(true, true, Place::Fullscreen));
    }

    #[test]
    fn a_place_coming_into_view_tries_a_broken_movie_again() {
        // collapsing the sidebar forgives nothing; expanding it again does
        assert!(!retries(Place::Slot, Place::None));
        assert!(retries(Place::None, Place::Slot));
        assert!(retries(Place::Slot, Place::Fullscreen));
        assert!(retries(Place::Fullscreen, Place::Pip));
        assert!(!retries(Place::Slot, Place::Slot));
        assert!(!retries(Place::None, Place::None));
    }

    #[test]
    fn a_load_in_flight_is_not_a_video() {
        // a track still being looked up may have none, so it counts for nothing yet
        assert!(!has(false, false, None));
        assert!(has(true, false, None));
        assert!(has(false, true, None));
        assert!(has(false, false, Some(true)));
        assert!(!has(false, false, Some(false)));
    }

    #[test]
    fn a_track_not_known_to_lack_a_video_may_have_one() {
        assert!(may_have(true, None));
        assert!(may_have(true, Some(true)));
        assert!(!may_have(true, Some(false)));
        assert!(!may_have(false, None));
    }

    #[test]
    fn a_place_letting_go_hands_the_video_to_no_place() {
        // the slot claims it from there when it is on screen; nothing loads for a hidden one
        assert_eq!(released(Place::Pip, Place::Pip), Some(Place::None));
        assert_eq!(
            released(Place::Fullscreen, Place::Fullscreen),
            Some(Place::None)
        );
        // a place something else has taken since stays
        assert_eq!(released(Place::Fullscreen, Place::Pip), None);
        assert_eq!(released(Place::Slot, Place::Fullscreen), None);
        assert_eq!(released(Place::None, Place::None), None);
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
