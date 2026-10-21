//! Read-only live checks of the plain-YouTube provider as a guest. No account, cookie or
//! credential is read or written.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use ytmusic::YtMusic;

use crate::MusicApi as _;
use crate::engine::Fetch as _;
use crate::youtube::playback::YouTube;
use crate::youtube_video::YouTubeVideoClient;

/// A Boiler Room set of about two hours, long enough that a seek to ninety minutes lands far
/// past anything a plain download would have fetched yet.
const LONG_SET: &str = "c0-hvjV2A5Y";
const NINETY_MINUTES: Duration = Duration::from_secs(90 * 60);
/// 64 KiB of decoded samples, as `f32`.
const SAMPLES: usize = 64 * 1024 / 4;
const DEADLINE: Duration = Duration::from_secs(5);

#[tokio::test]
#[ignore = "talks to youtube.com as a guest"]
async fn live_search_finds_the_boiler_room_channel() -> Result<()> {
    let client = YouTubeVideoClient::new(Arc::new(YtMusic::anonymous()));
    let channels = client.search_artists("boiler room").await?;

    let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    if !names
        .iter()
        .any(|name| name.to_lowercase().contains("boiler room"))
    {
        bail!("no Boiler Room channel among {names:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to youtube.com as a guest"]
async fn live_a_long_set_seeks_to_ninety_minutes_quickly() -> Result<()> {
    let fetch = YouTube::new(crate::youtube_video::provider().guest_client());
    let loaded = fetch.load(LONG_SET).await.context("cannot load the set")?;

    // The set was longer than ninety minutes when this was written. Should it ever be cut
    // shorter, the seek still lands far into the file, at 85% of what is left of it.
    let length = fetch
        .length(&loaded)
        .context("the set reported no length")?;
    let at = NINETY_MINUTES.min(length.mul_f32(0.85));
    println!("set length {length:?}, seeking to {at:?}");
    let started = Instant::now();
    // The engine opens a source on its own thread, because the decoder blocks on the download
    // that the runtime has to keep driving.
    let heard = tokio::task::spawn_blocking(move || {
        let source = fetch
            .open(LONG_SET, &loaded, at)
            .context("cannot open the set at the seek position")?;
        Ok::<_, anyhow::Error>(source.take(SAMPLES).count())
    })
    .await
    .context("the decoder thread failed")??;
    let took = started.elapsed();

    println!("read {heard} samples after the seek in {took:?}");
    if heard < SAMPLES {
        bail!("only {heard} of {SAMPLES} samples arrived");
    }
    if took > DEADLINE {
        bail!("the seek took {took:?}, longer than {DEADLINE:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to youtube.com as a guest"]
async fn live_a_set_opens_a_480p_video_feed() -> Result<()> {
    let client = YouTubeVideoClient::new(crate::youtube_video::provider().guest_client());
    let feed = client
        .video(LONG_SET, 480)
        .await?
        .context("the set has no video")?;
    println!("feed {}x{}", feed.width, feed.height);
    if feed.height != 480 {
        bail!("expected 480p, got {}p", feed.height);
    }

    let started = Instant::now();
    // The reader blocks on the download, which the runtime has to keep driving.
    let mut reader = feed.reader;
    tokio::task::spawn_blocking(move || {
        let mut buffer = vec![0u8; 64 * 1024];
        std::io::Read::read_exact(&mut reader, &mut buffer).context("cannot read 64 KiB")
    })
    .await??;
    let took = started.elapsed();
    println!("read 64 KiB in {took:?}");
    if took > DEADLINE {
        bail!("64 KiB took {took:?}");
    }
    drop(feed.stream);
    Ok(())
}

/// A clock that runs with the wall from `from`, standing in for playback.
struct WallClock {
    from: Duration,
    began: Instant,
}

impl crate::video::Clock for WallClock {
    fn now(&self) -> Option<Duration> {
        Some(self.from + self.began.elapsed())
    }
}

/// A manual performance check, not a pass/fail test: plays a real video feed against a wall
/// clock, drawn the way fullscreen draws it, and prints how often a new frame comes out. Run
/// it by exact name, in a dev and a release build, after changing the `video` crate:
///
/// `cargo test --package music --lib live_video_frame_cadence -- --ignored --exact
/// live_tests::youtube_video::live_video_frame_cadence --nocapture`
///
/// `CADENCE_ID`, `CADENCE_HEIGHT` (1080), `CADENCE_WIDTH` (1920) and `CADENCE_SECS` (20)
/// change what is played. Smooth is about the feed's frame rate with no gap over 100 ms.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "talks to youtube.com as a guest; a manual performance check"]
async fn live_video_frame_cadence() -> Result<()> {
    let knob = |name: &str, default: u64| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let id = std::env::var("CADENCE_ID").unwrap_or_else(|_| "BY5QK7AdEfw".into());
    let height = knob("CADENCE_HEIGHT", 1080) as u32;
    let width = knob("CADENCE_WIDTH", 1920) as u32;
    let secs = Duration::from_secs(knob("CADENCE_SECS", 20));
    let start = Duration::from_secs(30 * 60);

    let client = YouTubeVideoClient::new(crate::youtube_video::provider().guest_client());
    let feed = client.video(&id, height).await?.context("no video")?;
    let (stream, reader) = (feed.stream, feed.reader);
    let clock = Arc::new(WallClock {
        from: start,
        began: Instant::now(),
    });
    let movie =
        tokio::task::spawn_blocking(move || crate::video::Movie::start(reader, clock)).await??;
    let report = tokio::task::spawn_blocking(move || {
        let began = Instant::now();
        let (mut index, mut since) = (0, Instant::now());
        let (mut gaps, mut steps) = (Vec::new(), Vec::new());
        let mut last_pts = None;
        while began.elapsed() < secs {
            movie.watch(width);
            let Some((frame, pts)) = movie.frame().zip(movie.pts()) else {
                std::thread::sleep(Duration::from_millis(4));
                continue;
            };
            if frame.index != index {
                if index != 0 {
                    gaps.push(since.elapsed());
                }
                if let Some(last) = last_pts {
                    steps.push(pts.saturating_sub(last));
                }
                (index, since, last_pts) = (frame.index, Instant::now(), Some(pts));
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        if gaps.is_empty() {
            return format!("{index} frames shown in {secs:?}");
        }
        gaps.sort();
        let shown: Duration = gaps.iter().sum();
        let fps = gaps.len() as f64 / shown.as_secs_f64().max(f64::EPSILON);
        let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q) as usize];
        let frame = steps.iter().min().copied().unwrap_or_default();
        format!(
            "{}x{} drawn {width} wide: {fps:.1} fps shown (feed frame {frame:?}); gap p50 {:?} \
             p90 {:?} max {:?}; {} gaps over 100 ms",
            feed.width,
            feed.height,
            at(0.5),
            at(0.9),
            at(1.0),
            gaps.iter()
                .filter(|gap| **gap > Duration::from_millis(100))
                .count(),
        )
    })
    .await?;
    println!("{report}");
    drop(stream);
    Ok(())
}
