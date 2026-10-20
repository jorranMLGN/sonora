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
