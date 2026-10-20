//! Plays a streamed DASH video against a clock kept by someone else, on a thread of its own.
//! The thread reads a fragment ahead, decodes at most `BUFFER` frames past the clock and shows
//! the latest frame whose presentation time has come. A jump of the clock is a seek: back to
//! the keyframe that opens the fragment holding the new time, then forward to it.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::io::{Read, Seek};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use openh264::decoder::{Decoder, DecoderConfig, Flush};

use crate::Frame;
use crate::annexb::Mp4BitstreamConverter;
use crate::convert;
use crate::fmp4::{Sample, Stream};

/// How far past the clock the thread decodes at most, and how far the clock may run past the
/// frame on screen before the thread seeks rather than decoding its way there. `BUFFER` binds
/// first above 8 fps, so in practice this is the seek threshold.
const AHEAD: Duration = Duration::from_secs(1);
/// How far the clock may step back before the thread seeks rather than waiting for it.
const BEHIND: Duration = Duration::from_millis(500);
/// The most decoded frames held ahead of the clock: 267 ms at 30 fps. This is the bound that
/// binds, so a 1080p stream does not keep a full `AHEAD` of BGRA pictures (some 250 MB).
const BUFFER: usize = 8;
/// How little of the loaded fragments may be left to feed before the next one is read.
const PREFETCH: Duration = Duration::from_secs(1);
/// How long after the last `watch` the thread keeps decoding.
const UNWATCHED: Duration = Duration::from_millis(500);
/// How often an idle or paused thread looks again, and the longest it sleeps between frames.
const IDLE: Duration = Duration::from_millis(20);
/// The shortest sleep between two looks, so a frame that is due keeps the loop from spinning.
const NAP: Duration = Duration::from_millis(2);
/// How many pictures out of a fresh decoder teach `Stamps` how far it reorders: one GOP of
/// the fixture, and more than the reorder window of any H.264 stream.
const LEARN: usize = 24;
/// The most pictures H.264 lets a decoder hold back for reordering.
const REORDER: usize = 16;
/// The width a movie decodes at before anything has asked for one.
const FIRST_WIDTH: u32 = 360;
/// `seen` before the first `watch`, so a fresh movie counts as unwatched.
const NEVER: u64 = u64::MAX;

/// Where playback is, read by the decoder thread. `None` while paused or not yet playing.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Option<Duration>;
}

/// The frame on screen and when it plays.
struct Shown {
    frame: Arc<Frame>,
    pts: Duration,
}

struct Shared {
    shown: Mutex<Option<Shown>>,
    width: AtomicU32,
    stop: AtomicBool,
    /// Milliseconds since `born` at the last `watch`.
    seen: AtomicU64,
    born: Instant,
}

/// A streamed video following an outside clock. Dropping it stops the thread without waiting:
/// the thread sees `stop` within one `IDLE`, or once a blocked read of the source returns.
pub struct Movie {
    shared: Arc<Shared>,
}

/// A picture out of the decoder: its time, and its pixels when it was worth converting.
struct Decoded {
    pts: Duration,
    frame: Option<Picture>,
}

/// BGRA pixels not yet numbered as a `Frame`.
struct Picture {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

/// The decoding half of the thread: the stream, the decoder and what is fed but not yet out.
struct Decoding<R> {
    stream: Stream<R>,
    decoder: Decoder,
    convert: Mp4BitstreamConverter,
    /// Pictures of the loaded fragments not yet fed to the decoder, in decode order, each with
    /// the fragment it came from.
    packets: VecDeque<(usize, Sample)>,
    stamps: Stamps,
    /// Pictures the decoder gave up at the end of the stream.
    drained: VecDeque<Decoded>,
    /// The fragment of the picture last fed, and the next fragment to read.
    current: usize,
    next: usize,
    /// Reading `next` ahead failed; it is tried again once `packets` runs dry.
    failed: bool,
    flushed: bool,
    /// Whether `decoder` has seen a packet since it was opened.
    fed: bool,
    annexb: Vec<u8>,
}

/// Pairs the pictures coming out of the decoder with their times. The decoder hands pictures
/// back in presentation order, so each one out takes the earliest time fed and not yet out.
/// A picture that fails to decode leaves its time behind for good, which would label every
/// later picture one frame early; so the first `LEARN` pictures out measure how many times
/// the decoder holds at most, and later any excess is dropped, earliest first.
#[derive(Default)]
struct Stamps {
    pending: BinaryHeap<Reverse<(Duration, Duration)>>,
    /// The most times pending as a picture comes out, learnt from the first `LEARN`.
    depth: usize,
    /// Pictures out since the decoder opened.
    outs: usize,
}

impl Movie {
    /// Reads the init segment and the fragment index from `source`, then starts the thread,
    /// unwatched. Blocks until those first bytes arrive, so call it off the ui thread. Fails
    /// when `source` is no fragmented MP4 with one H.264 track and a `sidx`, or no decoder can
    /// be loaded.
    ///
    /// The thread reads `source` in order, one fragment ahead, and seeks it only on a jump of
    /// the clock. `source` should download ahead of those reads so they rarely block, and it
    /// must return an error, never block forever, once its download is cancelled: a read that
    /// never returns keeps the thread alive after the `Movie` is dropped.
    pub fn start(source: impl Read + Seek + Send + 'static, clock: Arc<dyn Clock>) -> Result<Self> {
        let decoding = Decoding::new(Stream::open(source)?)?;
        let shared = Arc::new(Shared {
            shown: Mutex::new(None),
            width: AtomicU32::new(FIRST_WIDTH),
            stop: AtomicBool::new(false),
            seen: AtomicU64::new(NEVER),
            born: Instant::now(),
        });
        std::thread::Builder::new()
            .name("video".into())
            .spawn({
                let shared = shared.clone();
                move || run(decoding, &shared, clock.as_ref())
            })
            .context("cannot start the video thread")?;
        Ok(Self { shared })
    }

    /// The frame on screen, if one has been shown yet.
    pub fn frame(&self) -> Option<Arc<Frame>> {
        let shown = self.shared.shown.lock().ok()?;
        shown.as_ref().map(|shown| shown.frame.clone())
    }

    /// Says a surface is drawing this movie at `width` pixels. Call it every frame the movie
    /// is on screen; the widest ask of the last moment wins, and an unwatched movie decodes
    /// nothing.
    pub fn watch(&self, width: u32) {
        let now = self.shared.born.elapsed().as_millis() as u64;
        let before = self.shared.seen.swap(now, Ordering::Relaxed);
        let stale = before == NEVER || now.saturating_sub(before) > UNWATCHED.as_millis() as u64;
        match stale {
            true => self.shared.width.store(width.max(1), Ordering::Relaxed),
            false => {
                self.shared.width.fetch_max(width.max(1), Ordering::Relaxed);
            }
        }
    }

    /// The presentation time of the frame on screen.
    pub fn pts(&self) -> Option<Duration> {
        let shown = self.shared.shown.lock().ok()?;
        shown.as_ref().map(|shown| shown.pts)
    }
}

impl Drop for Movie {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl<R: Read + Seek> Decoding<R> {
    fn new(stream: Stream<R>) -> Result<Self> {
        Ok(Self {
            convert: stream.converter(),
            decoder: decoder()?,
            stream,
            packets: VecDeque::new(),
            stamps: Stamps::default(),
            drained: VecDeque::new(),
            current: 0,
            next: 0,
            failed: false,
            flushed: false,
            fed: false,
            annexb: Vec::new(),
        })
    }

    /// Starts decoding over at the keyframe that opens the fragment holding `target`. A
    /// decoder that has been fed is replaced, so no picture from before the jump comes out
    /// after it: OpenH264 fails every packet after a `flush_remaining` mid-stream.
    fn seek(&mut self, target: Duration) -> Result<()> {
        if self.fed {
            self.decoder = decoder()?;
            self.fed = false;
        }
        self.convert = self.stream.converter();
        self.packets.clear();
        self.stamps = Stamps::default();
        self.drained.clear();
        self.flushed = false;
        let mut index = self.stream.segment_at(target);
        let mut samples = self.stream.fragment(index)?;
        // a fragment's first picture plays a little after its sidx start when B-frames delay
        // it, so a target in that gap belongs to the fragment before
        let first = samples.iter().map(|sample| sample.pts).min();
        if index > 0 && first.is_none_or(|first| first > target) {
            index -= 1;
            samples = self.stream.fragment(index)?;
        }
        self.packets = samples.into_iter().map(|sample| (index, sample)).collect();
        self.current = index;
        self.next = index + 1;
        self.failed = false;
        Ok(())
    }

    /// Reads fragment `next` onto the end of `packets`.
    fn load(&mut self) -> Result<()> {
        let fragment = self.next;
        let samples = self.stream.fragment(fragment)?;
        self.packets
            .extend(samples.into_iter().map(|sample| (fragment, sample)));
        self.next += 1;
        Ok(())
    }

    /// How long the pictures not yet fed play for.
    fn queued(&self) -> Duration {
        self.packets.iter().map(|(_, sample)| sample.duration).sum()
    }

    /// The next picture in presentation order, converted at `width` only when `keep` says it
    /// may be shown. Reads the next fragment once less than `PREFETCH` of pictures is left to
    /// feed, so the read never waits at a fragment boundary; `None` at the end.
    fn next(
        &mut self,
        width: u32,
        keep: impl Fn(Duration, Duration) -> bool,
    ) -> Result<Option<Decoded>> {
        loop {
            if let Some(decoded) = self.drained.pop_front() {
                return Ok(Some(decoded));
            }
            if !self.failed && self.next < self.stream.segments.len() && self.queued() < PREFETCH {
                match self.load() {
                    Ok(()) => {}
                    Err(error) if !self.packets.is_empty() => {
                        log::debug!("video: cannot read ahead, trying again later: {error:#}");
                        self.failed = true;
                    }
                    Err(error) => return Err(error),
                }
            }
            if let Some((fragment, sample)) = self.packets.pop_front() {
                self.current = fragment;
                self.convert.convert_packet(&sample.data, &mut self.annexb);
                self.stamps.fed(sample.pts, sample.duration);
                self.fed = true;
                match self.decoder.decode(&self.annexb) {
                    Ok(Some(image)) => {
                        let Some((pts, duration)) = self.stamps.out() else {
                            continue;
                        };
                        let frame = keep(pts, duration).then(|| picture(&image, width));
                        return Ok(Some(Decoded { pts, frame }));
                    }
                    Ok(None) => {}
                    Err(error) => log::debug!("video: a frame did not decode: {error}"),
                }
                continue;
            }
            if self.next < self.stream.segments.len() {
                self.failed = false;
                self.load()?;
                continue;
            }
            if self.flushed {
                return Ok(None);
            }
            self.flushed = true;
            let images = self
                .decoder
                .flush_remaining()
                .context("cannot flush the decoder")?;
            for image in images {
                let Some((pts, duration)) = self.stamps.out() else {
                    break;
                };
                let frame = keep(pts, duration).then(|| picture(&image, width));
                self.drained.push_back(Decoded { pts, frame });
            }
        }
    }
}

impl Stamps {
    /// Notes a picture fed to the decoder.
    fn fed(&mut self, pts: Duration, duration: Duration) {
        self.pending.push(Reverse((pts, duration)));
    }

    /// The time and duration of the picture the decoder just gave back.
    fn out(&mut self) -> Option<(Duration, Duration)> {
        match self.outs < LEARN {
            true => self.depth = self.depth.max(self.pending.len()).min(REORDER + 1),
            false => {
                while self.pending.len() > self.depth {
                    let Reverse((lost, _)) = self.pending.pop()?;
                    log::debug!("video: the picture at {lost:?} never came out");
                }
            }
        }
        self.outs += 1;
        let Reverse(next) = self.pending.pop()?;
        Some(next)
    }
}

/// The thread. Each turn reads the clock, seeks when it jumped, decodes until `AHEAD` past it
/// or `BUFFER` frames are waiting, shows the latest frame that is due and sleeps until the
/// next one is.
fn run<R: Read + Seek>(mut decoding: Decoding<R>, shared: &Shared, clock: &dyn Clock) {
    let mut queue: VecDeque<(Duration, Picture)> = VecDeque::new();
    // where the screen is: the time of the frame shown, or the target of the last seek
    let mut head: Option<Duration> = None;
    // the time of the last picture out of the decoder
    let mut decoded: Option<Duration> = None;
    let mut index = 0u64;
    let mut broken = false;
    while !shared.stop.load(Ordering::Relaxed) {
        let Some(now) = clock.now().filter(|_| watched(shared)) else {
            std::thread::sleep(IDLE);
            continue;
        };
        let seek = match head {
            None => true,
            Some(head) => {
                now + BEHIND < head
                    || (now > head + AHEAD && decoding.stream.segment_at(now) > decoding.current)
            }
        };
        if seek {
            queue.clear();
            decoded = None;
            head = Some(now);
            broken = match decoding.seek(now) {
                Ok(()) => false,
                Err(error) => {
                    log::warn!("video: cannot seek to {now:?}: {error:#}");
                    true
                }
            };
        }
        let width = shared.width.load(Ordering::Relaxed);
        while !broken
            && queue.len() < BUFFER
            && decoded.is_none_or(|pts| pts < now + AHEAD)
            && !shared.stop.load(Ordering::Relaxed)
        {
            // a picture whose successor is already due is never shown, so skip converting it
            match decoding.next(width, |pts, duration| pts + duration > now) {
                Ok(Some(picture)) => {
                    decoded = Some(picture.pts);
                    if let Some(frame) = picture.frame {
                        queue.push_back((picture.pts, frame));
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    log::warn!("video: cannot read the next fragment: {error:#}");
                    broken = true;
                }
            }
        }
        let mut due = None;
        while queue.front().is_some_and(|(pts, _)| *pts <= now) {
            due = queue.pop_front();
        }
        if let Some((pts, picture)) = due {
            index += 1;
            head = Some(pts);
            let frame = Arc::new(Frame {
                pixels: picture.pixels,
                width: picture.width,
                height: picture.height,
                index,
            });
            if let Ok(mut shown) = shared.shown.lock() {
                *shown = Some(Shown { frame, pts });
            }
        }
        let wait = queue
            .front()
            .map_or(IDLE, |(pts, _)| pts.saturating_sub(now))
            .clamp(NAP, IDLE);
        std::thread::sleep(wait);
    }
}

fn watched(shared: &Shared) -> bool {
    let now = shared.born.elapsed().as_millis() as u64;
    let seen = shared.seen.load(Ordering::Relaxed);
    seen != NEVER && now.saturating_sub(seen) <= UNWATCHED.as_millis() as u64
}

fn decoder() -> Result<Decoder> {
    let api = crate::codec::api()?;
    let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
    Decoder::with_api_config(api, config).context("cannot open the decoder")
}

fn picture(image: &impl openh264::formats::YUVSource, width: u32) -> Picture {
    let (pixels, width, height) = convert::bgra(image, width);
    Picture {
        pixels,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Mutex;
    use std::time::Instant;

    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/clip-dash.mp4");
    /// The width the tests draw at; wider than the fixture, so frames keep its 256.
    const WIDTH: u32 = 360;
    const WITHIN: Duration = Duration::from_millis(500);

    struct FakeClock(Mutex<Option<Duration>>);

    impl Clock for FakeClock {
        fn now(&self) -> Option<Duration> {
            *self.0.lock().unwrap()
        }
    }

    impl FakeClock {
        fn set(&self, at: Option<Duration>) {
            *self.0.lock().unwrap() = at;
        }
    }

    /// A movie of the fixture on a clock at `at`, or `None` when no decoder is installed.
    fn movie(at: Option<Duration>) -> Option<(Movie, Arc<FakeClock>)> {
        if crate::codec::find().is_none() {
            eprintln!("skipped: no openh264 installed");
            return None;
        }
        let clock = Arc::new(FakeClock(Mutex::new(at)));
        let movie = Movie::start(Cursor::new(FIXTURE.to_vec()), clock.clone()).unwrap();
        Some((movie, clock))
    }

    /// Draws `movie` at `width` until `done` holds or `limit` passes; says whether it held.
    fn draw_until(
        movie: &Movie,
        width: u32,
        limit: Duration,
        done: impl Fn(&Movie) -> bool,
    ) -> bool {
        let began = Instant::now();
        while began.elapsed() < limit {
            movie.watch(width);
            if done(movie) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn between(movie: &Movie, low: u64, high: u64) -> bool {
        movie.pts().is_some_and(|pts| {
            pts >= Duration::from_millis(low) && pts <= Duration::from_millis(high)
        })
    }

    #[test]
    fn a_lost_picture_does_not_shift_the_times_after_it() {
        // decode order of the fixture's pattern: I, then P, B, B, B repeating, 40 ms apart
        let frame = Duration::from_millis(40);
        let order = |n: u64| {
            let shown = match n {
                0 => 0,
                n => {
                    let base = (n - 1) / 4 * 4;
                    base + [4, 2, 1, 3][((n - 1) % 4) as usize]
                }
            };
            frame * shown as u32
        };
        let mut stamps = Stamps::default();
        // a decoder that holds three pictures back and gives them out in presentation order
        let mut held = BinaryHeap::new();
        let mut misses = Vec::new();
        for n in 0..200u64 {
            let pts = order(n);
            stamps.fed(pts, frame);
            held.push(Reverse(pts));
            if held.len() <= 3 {
                continue;
            }
            let Reverse(truth) = held.pop().unwrap();
            // the 80th picture fails to decode: the decoder gives nothing back for it
            if n == 80 {
                continue;
            }
            let (label, _) = stamps.out().unwrap();
            if label != truth {
                misses.push(n);
            }
        }
        // re-aligned within one GOP of 25 pictures after the loss
        assert!(
            misses.iter().all(|&n| (80..80 + 25).contains(&n)),
            "missed at {misses:?}"
        );
    }

    #[test]
    fn the_next_fragment_is_read_before_the_last_runs_out() {
        if crate::codec::find().is_none() {
            eprintln!("skipped: no openh264 installed");
            return;
        }
        let stream = Stream::open(Cursor::new(FIXTURE.to_vec())).unwrap();
        let mut decoding = Decoding::new(stream).unwrap();
        decoding.seek(Duration::ZERO).unwrap();
        for _ in 0..2 {
            decoding.next(WIDTH, |_, _| false).unwrap().unwrap();
        }
        // fragment 0 lasts 1 s, so less than that is left of it after two pictures
        assert_eq!(decoding.next, 2);
        assert!(decoding.packets.iter().any(|(fragment, _)| *fragment == 0));
    }

    #[test]
    fn frames_follow_the_clock() {
        let Some((movie, _clock)) = movie(Some(Duration::from_millis(2000))) else {
            return;
        };
        let held = draw_until(&movie, WIDTH, WITHIN, |movie| between(movie, 1960, 2000));
        assert!(held, "pts is {:?}", movie.pts());
    }

    #[test]
    fn a_seek_lands_on_the_keyframe_before_the_target() {
        let Some((movie, clock)) = movie(Some(Duration::from_millis(200))) else {
            return;
        };
        let held = draw_until(&movie, WIDTH, WITHIN, |movie| between(movie, 160, 200));
        assert!(held, "pts is {:?}", movie.pts());
        clock.set(Some(Duration::from_millis(3300)));
        let held = draw_until(&movie, WIDTH, WITHIN, |movie| between(movie, 3260, 3300));
        assert!(held, "pts is {:?}", movie.pts());
    }

    #[test]
    fn a_paused_clock_holds_the_frame() {
        let Some((movie, clock)) = movie(Some(Duration::from_millis(1000))) else {
            return;
        };
        let held = draw_until(&movie, WIDTH, WITHIN, |movie| between(movie, 960, 1000));
        assert!(held, "pts is {:?}", movie.pts());
        clock.set(None);
        let index = movie.frame().unwrap().index;
        draw_until(&movie, WIDTH, Duration::from_millis(300), |_| false);
        assert_eq!(movie.frame().unwrap().index, index);
    }

    #[test]
    fn watch_scales_the_frame_width() {
        let Some((movie, _clock)) = movie(Some(Duration::from_millis(500))) else {
            return;
        };
        let held = draw_until(&movie, 128, WITHIN, |movie| {
            movie.frame().is_some_and(|frame| frame.width == 128)
        });
        assert!(held);
    }
}
