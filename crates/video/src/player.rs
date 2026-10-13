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
/// lap are skipped; at 25–30 fps that is invisible.
fn run(clip: Clip, mut decoder: Decoder, shared: &Shared) {
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
        match decoder.decode(&clip.packets[next]) {
            Ok(Some(image)) => {
                let width = shared.width.load(Ordering::Relaxed);
                let (pixels, width, height) = convert::bgra(&image, width);
                index += 1;
                if let Ok(mut latest) = shared.latest.lock() {
                    *latest = Some(Arc::new(Frame {
                        pixels,
                        width,
                        height,
                        index,
                    }));
                }
            }
            Ok(None) => {}
            Err(error) => log::debug!("canvas: frame {next} did not decode: {error}"),
        }
        next = (next + 1) % clip.packets.len();
        due += clip.frame;
        let wait = due.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            due = Instant::now();
        }
        std::thread::sleep(wait);
    }
}
