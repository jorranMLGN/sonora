//! Playback for YouTube Music: what [`crate::engine`] needs that is YouTube's own.
//!
//! The track streams through [`crate::stream`] like every other provider's, fed by ytmusic's
//! ranged download, which asks again from the byte a stalled connection stopped at. The file is
//! DASH fMP4, so the decoder opens it unseekable and a seek splices the header onto the segment
//! the `sidx` names, the way Apple Music's does. A splice far past what has arrived moves the
//! download there, so a seek into a long set plays within seconds. The threads, the queue, the
//! preload, the gapless join and the loudness gain are the engine's.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use ytmusic::{AudioStream, YtMusic};

use crate::aac::Aac;
use crate::audio::Trimmed;
use crate::cast::CastSink;
use crate::engine::{self, Fetch, Loudness};
use crate::stream::{PREROLL, Plain, Ranged, Reader, Source, Stream};
use crate::trim::{self, Trim};
use crate::vault::{Recording, Shelf};
use crate::youtube::segments::Segments;
use crate::{Encoding, Offer, PlaybackConfig, PlaybackEvents, PlaybackFactory, Player, Quality};

/// How long the stream metadata, and then the preroll, may each take before the attempt is
/// given up. The stream host has no timeout of its own, so a connection it dropped halfway
/// would otherwise hold the engine on a silent track forever.
const PATIENCE: Duration = Duration::from_secs(15);
/// The attempts a fetch gets before the track is reported unavailable.
const ATTEMPTS: u32 = 2;
/// The level the stream host measures `loudnessDb` against, in LUFS.
const REFERENCE_LUFS: f32 = -14.0;
/// The levels YouTube Music serves, best first. High is the best AAC stream there is, 256 kbps
/// for a Premium account and 128 kbps otherwise, normal is the 128 kbps one and low the 48 kbps
/// HE-AAC one.
pub(crate) const QUALITIES: &[Offer] = &[
    Offer {
        quality: Quality::High,
        encoding: Encoding::Aac { kbps: 256 },
        up_to: true,
    },
    Offer {
        quality: Quality::Normal,
        encoding: Encoding::Aac { kbps: 128 },
        up_to: false,
    },
    Offer {
        quality: Quality::Low,
        encoding: Encoding::Aac { kbps: 48 },
        up_to: false,
    },
];

/// A track arriving, with what the stream host said about its length and loudness and what its
/// header says about where the music is.
#[derive(Clone)]
pub struct Loaded {
    stream: Stream,
    /// Where each segment starts. `None` for a file with no index, which a seek then has to
    /// decode its way through.
    segments: Option<Arc<Segments>>,
    /// The edit list, which says how much priming to drop and how long the music runs.
    edit: Option<Trim>,
    loudness_db: Option<f32>,
    duration: Option<Duration>,
    /// The format the stream host picked, `None` when it gave no bit rate.
    encoding: Option<Encoding>,
}

/// Starts the engine for one YouTube site. Both sites share this playback, so each passes its
/// own slug, which names the engine's jam feed and its threads.
pub struct Factory {
    api: Arc<YtMusic>,
    slug: &'static str,
}

impl Factory {
    pub fn new(api: Arc<YtMusic>, slug: &'static str) -> Self {
        Self { api, slug }
    }

    #[cfg(test)]
    pub(crate) fn slug(&self) -> &'static str {
        self.slug
    }
}

impl PlaybackFactory for Factory {
    fn start(
        &self,
        config: PlaybackConfig,
        cast: Option<CastSink>,
    ) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        engine::start(
            YouTube {
                api: self.api.clone(),
                slug: self.slug,
                ceiling: ceiling(config.quality),
                shelf: config.vault.as_ref().map(|vault| vault.shelf(self.slug)),
            },
            config,
            self.slug,
            cast,
        )
    }
}

/// The engine's fetch for YouTube. `pub(crate)` so a live test can drive `load` and `open`
/// without a running engine.
pub(crate) struct YouTube {
    api: Arc<YtMusic>,
    /// The site's slug, which names the engine's threads and its shelf in the vault.
    slug: &'static str,
    /// The highest bitrate a stream is chosen at, in bits per second, or `None` for the best.
    ceiling: Option<u32>,
    /// Where played tracks are kept, so playing one again asks YouTube for nothing.
    shelf: Option<Shelf>,
}

/// What the stream host said about a track, kept beside it in the vault so a kept track plays
/// with the same length, loudness and signal path as a fresh one.
#[derive(Serialize, Deserialize)]
struct Facts {
    loudness_db: Option<f32>,
    duration_ms: Option<u64>,
    codec: String,
    kbps: u32,
}

impl YouTube {
    #[cfg(test)]
    pub(crate) fn new(api: Arc<YtMusic>) -> Self {
        Self {
            api,
            slug: "youtube",
            ceiling: None,
            shelf: None,
        }
    }
}

impl Source for AudioStream {
    async fn chunk(&mut self) -> Result<Option<Bytes>> {
        AudioStream::chunk(self).await
    }
}

impl Ranged for AudioStream {
    fn resumed(&self, at: u64) -> Self {
        AudioStream::resumed(self, at)
    }
}

#[async_trait]
impl Fetch for YouTube {
    type Loaded = Loaded;
    type Source = Trimmed<rodio::Decoder<Reader>>;

    fn name(&self) -> &'static str {
        self.slug
    }

    fn shelf(&self) -> Option<&Shelf> {
        self.shelf.as_ref()
    }

    /// Opens the stream and waits for the preroll, giving a stalled attempt one more go before
    /// failing. Only a timeout is retried, since a refusal from the stream host is as final the
    /// second time.
    async fn load(&self, id: &str) -> Result<Loaded> {
        if let Some(loaded) = self.recall(id).await {
            return Ok(loaded);
        }
        let mut attempt = 1;
        loop {
            match attempt_open(&self.api, id, self.ceiling, self.shelf.as_ref()).await {
                Err(error) if attempt < ATTEMPTS && error.is::<tokio::time::error::Elapsed>() => {
                    log::warn!("playback: {id} stalled, trying again: {error:#}");
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    fn length(&self, loaded: &Loaded) -> Option<Duration> {
        loaded.duration
    }

    fn loudness(&self, loaded: &Loaded) -> Option<Loudness> {
        loaded.loudness_db.map(|db| Loudness {
            lufs: REFERENCE_LUFS + db,
            peak: None,
        })
    }

    fn encoding(&self, loaded: &Loaded) -> Option<Encoding> {
        loaded.encoding
    }

    fn gated(&self, error: &anyhow::Error) -> bool {
        error.downcast_ref::<ytmusic::SignInRequired>().is_some()
    }

    async fn downloaded(&self, loaded: &Loaded) {
        loaded.stream.finished().await;
    }

    fn progress(&self, loaded: &Loaded) -> Option<f32> {
        let total = loaded.stream.total().filter(|total| *total > 0)?;
        Some(loaded.stream.arrived() as f32 / total as f32)
    }

    /// A fragmented stream opened unseekable cannot move, so a seek opens a second decoder
    /// spliced to the target.
    fn reopen_to_seek(&self) -> bool {
        true
    }

    /// Builds a decoder over the stream, trimmed to what the edit list says is heard, and
    /// places it at `at`.
    ///
    /// The decoder is told neither the length nor that it may seek. Told either, symphonia walks
    /// every top-level box of the file before the first packet, which waits for the whole
    /// download. Starting anywhere but the beginning splices the header onto the segment that
    /// covers the position, and the part of that segment before it is trimmed off the front.
    fn open(&self, id: &str, loaded: &Loaded, at: Duration) -> Option<Self::Source> {
        let priming = loaded.edit.map(|edit| edit.skip).unwrap_or_default();
        let entry = match at.is_zero() {
            true => None,
            false => loaded
                .segments
                .as_ref()
                .and_then(|segments| segments.entry(at + priming)),
        };
        let reader = match entry {
            Some(entry) => loaded.stream.spliced(entry.head, entry.graft),
            None => loaded.stream.reader(),
        };
        let decoder = match rodio::Decoder::builder()
            .with_symphonia_decoder::<Aac>()
            .with_data(reader)
            .with_seekable(false)
            .build()
        {
            Ok(decoder) => decoder,
            Err(error) => {
                log::warn!("playback: cannot decode the youtube track {id}: {error}");
                return None;
            }
        };
        let take = loaded.edit.and_then(|edit| edit.take);
        // Without an index the only way to a position is through every sample before it.
        let (skip, take) = match entry {
            Some(entry) => (entry.into, None),
            None => (priming + at, take.map(|take| take.saturating_sub(at))),
        };
        Some(Trimmed::new(decoder, skip, take))
    }
}

/// Opens the video of `id` no taller than `max_height` as a feed that downloads ahead of its
/// reader.
pub(crate) async fn open_video_feed(
    api: &Arc<YtMusic>,
    id: &str,
    max_height: u32,
) -> Result<crate::VideoFeed> {
    let (format, source) = tokio::time::timeout(PATIENCE, api.open_video(id, max_height))
        .await
        .context("video metadata timed out")??;
    let total = source.total();
    let stream = Stream::pulling_ranged_windowed(source, total, window(format.bitrate));
    let reader = stream.reader();
    log::debug!(
        "video: {id} at {}p, itag {}, {:.1} MiB, {} kbps",
        format.height,
        format.itag,
        total.unwrap_or_default() as f64 / (1024.0 * 1024.0),
        format.bitrate / 1000
    );
    Ok(crate::VideoFeed {
        width: format.width,
        height: format.height,
        reader,
        stream: crate::VideoDownload::new(stream),
    })
}

/// How far a video's download runs ahead of what is being watched: `AHEAD` of it at the
/// format's bitrate, so a stall in the connection has minutes to recover in, within bounds
/// that keep a wrong bitrate from fetching too little or most of the file.
fn window(bitrate: u32) -> usize {
    const AHEAD: u64 = 180;
    const LEAST: usize = 8 * 1024 * 1024;
    const MOST: usize = 128 * 1024 * 1024;
    const UNKNOWN: usize = 64 * 1024 * 1024;
    match bitrate {
        0 => UNKNOWN,
        bits => usize::try_from(u64::from(bits) / 8 * AHEAD)
            .unwrap_or(MOST)
            .clamp(LEAST, MOST),
    }
}

impl YouTube {
    /// The track from the vault, when it was kept at the quality asked for. Nothing here
    /// reaches YouTube, so a kept track needs no player request and no token.
    async fn recall(&self, id: &str) -> Option<Loaded> {
        let kept = self
            .shelf
            .as_ref()?
            .find(id, &variant(self.ceiling))
            .await?;
        let facts = serde_json::from_slice::<Facts>(&kept.facts)
            .inspect_err(|error| {
                log::warn!("playback: the kept facts of {id} do not read: {error}")
            })
            .ok()?;
        let stream = Stream::pulling(kept.source, Some(kept.length), Plain)
            .primed()
            .await
            .inspect_err(|error| log::warn!("playback: the kept copy of {id} broke: {error:#}"))
            .ok()?;
        let (segments, edit) = front(&stream);
        log::debug!("playback: {id} started from the vault");
        Some(Loaded {
            stream,
            segments: segments.map(Arc::new),
            edit,
            loudness_db: facts.loudness_db,
            duration: facts.duration_ms.map(Duration::from_millis),
            encoding: served(&facts.codec, facts.kbps),
        })
    }
}

/// Opens the stream and waits for the preroll, each under its own deadline. With a shelf, the
/// track is kept in the vault once all of it has arrived.
async fn attempt_open(
    api: &Arc<YtMusic>,
    id: &str,
    ceiling: Option<u32>,
    shelf: Option<&Shelf>,
) -> Result<Loaded> {
    let started = Instant::now();
    let (format, audio) = tokio::time::timeout(PATIENCE, api.open_audio_within(id, ceiling))
        .await
        .context("stream metadata timed out")??;
    let total = audio.total();
    let audio = match shelf {
        Some(shelf) => {
            let facts = Facts {
                loudness_db: format.loudness_db,
                duration_ms: format.duration.map(|duration| duration.as_millis() as u64),
                codec: format.codec.clone(),
                kbps: format.bitrate / 1000,
            };
            shelf.keep(
                id,
                &variant(ceiling),
                move || serde_json::to_vec(&facts).ok(),
                audio,
            )
        }
        None => Recording::through(audio),
    };
    // A known length lets a seek far ahead move the download there instead of waiting for it.
    // Such a jump drops the copy being kept, since the vault keeps a track only whole.
    let stream = match total {
        Some(_) => Stream::pulling_ranged(audio, total),
        None => Stream::pulling(audio, total, Plain),
    };
    let stream = tokio::time::timeout(PATIENCE, stream.primed())
        .await
        .context("stream preroll timed out")??;
    let (segments, edit) = front(&stream);
    log::debug!(
        "playback: {id} started, itag {} {} {} kbps, {:.1} MiB, {} segments, in {:?}",
        format.itag,
        format.codec,
        format.bitrate / 1000,
        total.unwrap_or_default() as f64 / (1024.0 * 1024.0),
        segments.as_ref().map_or(0, Segments::count),
        started.elapsed()
    );
    match edit {
        Some(edit) => log::debug!(
            "playback: {id} trims {:?} of priming, plays {:?}",
            edit.skip,
            edit.take
        ),
        None => log::debug!("playback: {id} carries no edit list"),
    }
    Ok(Loaded {
        stream,
        segments: segments.map(Arc::new),
        edit,
        loudness_db: format.loudness_db,
        duration: format.duration,
        encoding: served(&format.codec, format.bitrate / 1000),
    })
}

/// The segment index and the edit list, which sit at the front of the file inside what the
/// preroll waited for.
fn front(stream: &Stream) -> (Option<Segments>, Option<Trim>) {
    stream
        .with(|_, spool| {
            let front = spool.bytes(0..spool.len().min(PREROLL)).unwrap_or_default();
            (Segments::read(&front), trim::from_mp4(&front))
        })
        .unwrap_or_default()
}

/// What tells two kept copies of one track apart: the bitrate ceiling it was fetched under.
fn variant(ceiling: Option<u32>) -> String {
    ceiling.map_or_else(|| "best".to_owned(), |ceiling| ceiling.to_string())
}

/// The bitrate ceiling a level stands for, in bits per second. Each sits between two streams,
/// since the bitrate YouTube reports runs a little above the nominal one.
fn ceiling(quality: Quality) -> Option<u32> {
    match quality.within(QUALITIES) {
        Quality::HiRes | Quality::Lossless | Quality::High => None,
        Quality::Normal => Some(160_000),
        Quality::Low => Some(64_000),
    }
}

/// The encoding of the stream the host serves, by its codec name and bit rate in kilobits a
/// second, `None` when it gave no bit rate.
fn served(codec: &str, kbps: u32) -> Option<Encoding> {
    (kbps > 0).then(|| match codec {
        "opus" => Encoding::Opus { kbps },
        codec if codec.starts_with("mp4a") => Encoding::Aac { kbps },
        _ => Encoding::Other,
    })
}
