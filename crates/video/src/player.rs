//! Plays one clip in a loop on a thread of its own. The clip is parsed once; the thread decodes
//! a frame at the clip's own pace, and sleeps whenever it is paused or nothing has drawn a frame
//! for `UNWATCHED`, so a canvas nobody sees costs nothing.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use openh264::decoder::{Decoder, DecoderConfig, Flush};

use crate::annexb::Mp4BitstreamConverter;
use crate::convert;

/// How long after the last `watch` the thread keeps decoding.
const UNWATCHED: Duration = Duration::from_millis(500);
/// How often an idle thread looks again.
const IDLE: Duration = Duration::from_millis(50);
/// The width a player starts at before anything has asked for one.
const FIRST_WIDTH: u32 = 360;
/// `seen` before the first `watch`, so a fresh player counts as unwatched.
const NEVER: u64 = u64::MAX;
/// How many bytes of decoded frames one clip may keep to replay its laps: a few seconds of
/// Canvas at the width of the queue fits, a fullscreen one usually does not and decodes.
const KEPT: usize = 192 << 20;

/// One decoded picture, BGRA, the way gpui's `RenderImage` wants it. `index` only grows, so a
/// view can tell a new frame from the one it already holds.
pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub index: u64,
}

struct Shared {
    latest: Mutex<Option<Arc<Frame>>>,
    width: AtomicU32,
    running: AtomicBool,
    stop: AtomicBool,
    /// Milliseconds since `born` at the last `watch`.
    seen: AtomicU64,
    born: Instant,
}

/// A looping clip. Dropping it stops the thread without waiting for it: the owner drops it on
/// the ui thread on every skip, and the thread sees `stop` within one `IDLE` or one frame.
pub struct Player {
    shared: Arc<Shared>,
}

struct Clip {
    packets: Vec<Vec<u8>>,
    frame: Duration,
}

/// The frames of one whole lap, kept so the next laps replay them instead of decoding the clip
/// again, which is most of what a Canvas costs. A lap is kept only when it was decoded at one
/// width from its first packet and fits `budget`; otherwise every lap decodes as before.
struct Reel {
    budget: usize,
    state: Spool,
}

/// Where a `Reel` stands with the lap it keeps.
enum Spool {
    /// Nothing kept, and nothing being kept.
    Idle,
    /// Keeping the lap being decoded at `width`.
    Recording {
        width: u32,
        frames: Vec<Arc<Frame>>,
        bytes: usize,
    },
    /// A lap at `width` did not fit, so laps at that width are not kept again.
    Oversized { width: u32 },
    /// Replaying a kept lap at `width`; `at` is the frame shown next.
    Playing {
        width: u32,
        frames: Vec<Arc<Frame>>,
        at: usize,
    },
}

impl Player {
    /// Parses `bytes` as an MP4 with one H.264 track and starts the thread paused and unwatched.
    /// Fails when the file is no such MP4 or no decoder can be loaded.
    pub fn start(bytes: Vec<u8>) -> Result<Self> {
        let clip = parse(&bytes)?;
        let api = crate::codec::api()?;
        let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
        let decoder = Decoder::with_api_config(api, config).context("cannot open the decoder")?;
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            width: AtomicU32::new(FIRST_WIDTH),
            running: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            seen: AtomicU64::new(NEVER),
            born: Instant::now(),
        });
        std::thread::Builder::new()
            .name("canvas".into())
            .spawn({
                let shared = shared.clone();
                move || run(clip, decoder, &shared)
            })
            .context("cannot start the canvas thread")?;
        Ok(Self { shared })
    }

    /// The newest frame, if one has been decoded yet.
    pub fn frame(&self) -> Option<Arc<Frame>> {
        self.shared.latest.lock().ok()?.clone()
    }

    /// Says a surface is drawing this clip at `width` pixels. Call it every frame the clip is on
    /// screen; the widest ask of the last moment wins.
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

    /// Pauses or resumes the loop, following playback.
    pub fn set_running(&self, running: bool) {
        self.shared.running.store(running, Ordering::Relaxed);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl Reel {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            state: Spool::Idle,
        }
    }

    /// A lap starts decoding from its first packet at `width`.
    fn begin(&mut self, width: u32) {
        self.state = match std::mem::replace(&mut self.state, Spool::Idle) {
            Spool::Oversized { width: failed } if failed == width => Spool::Oversized { width },
            _ => Spool::Recording {
                width,
                frames: Vec::new(),
                bytes: 0,
            },
        };
    }

    /// A frame of the lap being decoded, asked for at `width`.
    fn keep(&mut self, frame: &Arc<Frame>, width: u32) {
        let Spool::Recording {
            width: recording,
            frames,
            bytes,
        } = &mut self.state
        else {
            return;
        };
        if *recording != width {
            self.state = Spool::Idle;
            return;
        }
        *bytes += frame.pixels.len();
        match *bytes <= self.budget {
            true => frames.push(frame.clone()),
            false => self.state = Spool::Oversized { width },
        }
    }

    /// The lap being decoded reached its last packet.
    fn end(&mut self) {
        self.state = match std::mem::replace(&mut self.state, Spool::Idle) {
            Spool::Recording { width, frames, .. } if !frames.is_empty() => Spool::Playing {
                width,
                frames,
                at: 0,
            },
            Spool::Recording { .. } => Spool::Idle,
            other => other,
        };
    }

    /// The next kept frame to show at `width`, or nothing when the clip has to be decoded:
    /// no lap is kept, or the one kept was for another width, which is then let go.
    fn replay(&mut self, width: u32) -> Option<Arc<Frame>> {
        let Spool::Playing {
            width: kept,
            frames,
            at,
        } = &mut self.state
        else {
            return None;
        };
        if *kept != width {
            self.state = Spool::Idle;
            return None;
        }
        let frame = frames[*at].clone();
        *at = (*at + 1) % frames.len();
        Some(frame)
    }
}

fn parse(bytes: &[u8]) -> Result<Clip> {
    let mut mp4 = mp4::Mp4Reader::read_header(Cursor::new(bytes), bytes.len() as u64)
        .context("cannot read the canvas")?;
    let track = mp4
        .tracks()
        .values()
        .find(|track| track.media_type().ok() == Some(mp4::MediaType::H264))
        .context("the canvas has no h264 track")?;
    let (id, count) = (track.track_id(), track.sample_count());
    let fps = track.frame_rate();
    let frame = Duration::from_secs_f64(1.0 / if fps > 1.0 { fps } else { 25.0 });
    let mut convert = Mp4BitstreamConverter::for_mp4_track(track)?;
    let mut packets = Vec::with_capacity(count as usize);
    for index in 1..=count {
        let Some(sample) = mp4.read_sample(id, index)? else {
            continue;
        };
        let mut packet = Vec::new();
        convert.convert_packet(&sample.bytes, &mut packet);
        packets.push(packet);
    }
    anyhow::ensure!(!packets.is_empty(), "the canvas has no frames");
    Ok(Clip { packets, frame })
}

/// The loop. The first packet carries SPS/PPS and an IDR, so feeding it again restarts the
/// decoder cleanly at every lap. `Flush::NoFlush` holds two frames back, so the last two of a
/// lap are skipped; at 25–30 fps that is invisible. Once a whole lap is kept in `Reel`, the
/// laps after it replay those frames and the decoder rests until the width changes.
fn run(clip: Clip, mut decoder: Decoder, shared: &Shared) {
    let mut reel = Reel::new(KEPT);
    let mut next = 0usize;
    let mut index = 0u64;
    let mut due = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let now = shared.born.elapsed().as_millis() as u64;
        let seen = shared.seen.load(Ordering::Relaxed);
        let watched = seen != NEVER && now.saturating_sub(seen) <= UNWATCHED.as_millis() as u64;
        if !shared.running.load(Ordering::Relaxed) || !watched {
            std::thread::sleep(IDLE);
            due = Instant::now();
            continue;
        }
        let width = shared.width.load(Ordering::Relaxed);
        match reel.replay(width) {
            Some(frame) => show(shared, frame),
            None => {
                if next == 0 {
                    reel.begin(width);
                }
                match decoder.decode(&clip.packets[next]) {
                    Ok(Some(image)) => {
                        let (pixels, width_drawn, height) = convert::bgra(&image, width);
                        index += 1;
                        let frame = Arc::new(Frame {
                            pixels,
                            width: width_drawn,
                            height,
                            index,
                        });
                        reel.keep(&frame, width);
                        show(shared, frame);
                    }
                    Ok(None) => {}
                    Err(error) => log::debug!("canvas: frame {next} did not decode: {error}"),
                }
                next = (next + 1) % clip.packets.len();
                if next == 0 {
                    reel.end();
                }
            }
        }
        due += clip.frame;
        let wait = due.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            due = Instant::now();
        }
        std::thread::sleep(wait);
    }
}

/// Makes `frame` the newest one a view picks up.
fn show(shared: &Shared, frame: Arc<Frame>) {
    if let Ok(mut latest) = shared.latest.lock() {
        *latest = Some(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(index: u64, width: u32) -> Arc<Frame> {
        Arc::new(Frame {
            pixels: vec![0; (width * 2 * 4) as usize],
            width,
            height: 2,
            index,
        })
    }

    fn lap(reel: &mut Reel, width: u32, frames: &[u64]) {
        reel.begin(width);
        for index in frames {
            reel.keep(&frame(*index, width), width);
        }
        reel.end();
    }

    fn indices(reel: &mut Reel, width: u32, count: usize) -> Vec<Option<u64>> {
        (0..count)
            .map(|_| reel.replay(width).map(|frame| frame.index))
            .collect()
    }

    #[test]
    fn a_whole_lap_that_fits_is_replayed_round_and_round() {
        let mut reel = Reel::new(1 << 20);
        assert!(reel.replay(4).is_none());
        lap(&mut reel, 4, &[1, 2, 3]);
        assert_eq!(
            indices(&mut reel, 4, 5),
            [Some(1), Some(2), Some(3), Some(1), Some(2)]
        );
    }

    #[test]
    fn a_lap_over_the_budget_is_not_kept_nor_tried_again_at_that_width() {
        let mut reel = Reel::new(100);
        lap(&mut reel, 4, &[1, 2, 3, 4]);
        assert!(reel.replay(4).is_none());
        reel.begin(4);
        assert!(matches!(reel.state, Spool::Oversized { width: 4 }));
        lap(&mut reel, 2, &[5, 6]);
        assert_eq!(indices(&mut reel, 2, 2), [Some(5), Some(6)]);
    }

    #[test]
    fn another_width_lets_the_kept_lap_go() {
        let mut reel = Reel::new(1 << 20);
        lap(&mut reel, 4, &[1, 2]);
        assert_eq!(reel.replay(4).map(|frame| frame.index), Some(1));
        assert!(reel.replay(8).is_none());
        assert!(reel.replay(4).is_none());
        lap(&mut reel, 8, &[3]);
        assert_eq!(indices(&mut reel, 8, 2), [Some(3), Some(3)]);
    }

    #[test]
    fn a_lap_whose_width_changed_halfway_is_not_kept() {
        let mut reel = Reel::new(1 << 20);
        reel.begin(4);
        reel.keep(&frame(1, 4), 4);
        reel.keep(&frame(2, 8), 8);
        reel.end();
        assert!(reel.replay(8).is_none());
        assert!(reel.replay(4).is_none());
    }

    #[test]
    fn a_lap_joined_halfway_is_not_kept() {
        let mut reel = Reel::new(1 << 20);
        reel.keep(&frame(1, 4), 4);
        reel.end();
        assert!(reel.replay(4).is_none());
    }
}
