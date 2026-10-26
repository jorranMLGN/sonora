use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;

#[cfg(all(test, feature = "spotify", feature = "youtube"))]
use super::auth::ClientId;
use super::http::Http;
use super::stream;
use super::wire::{self, Transcoding};
use rodio::Source as _;

use crate::audio::Trimmed;
use crate::cast::CastSink;
use crate::engine::{self, Fetch};
use crate::trim;
use crate::{PlaybackConfig, PlaybackEvents, PlaybackFactory, Player};

/// Ranks a transcoding preset; higher is better. Unknown presets rank above
/// nothing but below anything named, so a preset SoundCloud adds later is
/// still playable rather than silently skipped.
fn rank(preset: &str) -> u8 {
    match preset {
        "aac_160k" => 4,
        "aac_96k" => 2,
        "mp3_0_0" => 1,
        _ => 1,
    }
}

/// Chooses which transcoding to play.
///
/// The only `progressive` transcoding soundcloud offers is a legacy 128kbps
/// mp3; every better encoding (`aac_160k`, `aac_96k`) is hls-only. So this
/// prefers the best-ranked hls entry and falls back to progressive only when
/// no hls entry is offered at all. Filtering happens on `format.protocol`,
/// never on `preset` alone: `mp3_0_0` exists as both an hls and a progressive
/// entry.
///
/// An `abr_` preset is never chosen. It names an adaptive master playlist of
/// variants, which `stream::assemble` cannot read, and soundcloud refuses to
/// resolve it with a 404 anyway — so ranking it would only fail a track that
/// has a playable `aac_96k` beside it.
pub fn pick(transcodings: &[Transcoding]) -> Option<&Transcoding> {
    transcodings
        .iter()
        .filter(|t| t.format.protocol == "hls" && !t.preset.starts_with("abr_"))
        .max_by_key(|t| rank(&t.preset))
        .or_else(|| {
            transcodings
                .iter()
                .find(|t| t.format.protocol == "progressive")
        })
}

/// Starts the shared engine over SoundCloud's tracks.
pub struct Factory {
    http: Arc<Http>,
}

impl Factory {
    pub fn new(http: Arc<Http>) -> Self {
        Self { http }
    }
}

impl PlaybackFactory for Factory {
    fn start(
        &self,
        config: PlaybackConfig,
        cast: Option<CastSink>,
    ) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        let fetch = SoundCloud {
            http: self.http.clone(),
        };
        engine::start(fetch, config, "soundcloud", cast)
    }
}

/// The engine's fetch for SoundCloud. A track arrives whole, an HLS one assembled from its
/// segments, and decodes from memory, so a seek is the decoder's own.
struct SoundCloud {
    http: Arc<Http>,
}

/// A downloaded track and the length SoundCloud gave for it.
#[derive(Clone)]
pub struct Loaded {
    data: Arc<Vec<u8>>,
    duration: Option<Duration>,
}

#[async_trait]
impl Fetch for SoundCloud {
    type Loaded = Loaded;
    type Source = Trimmed<rodio::Decoder<Cursor<Bytes>>>;

    fn name(&self) -> &'static str {
        "soundcloud"
    }

    async fn load(&self, id: &str) -> Result<Loaded> {
        fetch(&self.http, id).await
    }

    fn length(&self, loaded: &Loaded) -> Option<Duration> {
        loaded.duration
    }

    /// The v2 api carries no per-track loudness, so a SoundCloud track has no gain of its own.
    fn open(&self, id: &str, loaded: &Loaded, at: Duration) -> Option<Self::Source> {
        let source = decode(loaded.data.clone())
            .inspect_err(|error| log::warn!("playback: cannot decode {id}: {error:#}"))
            .ok()?;
        let edit = trim::from_mp4(&loaded.data);
        let mut source = Trimmed::new(
            source,
            edit.map(|edit| edit.skip).unwrap_or_default(),
            edit.and_then(|edit| edit.take),
        );
        if !at.is_zero()
            && let Err(error) = source.try_seek(at)
        {
            log::warn!("playback: cannot start {id} at {}s: {error}", at.as_secs());
        }
        Some(source)
    }

    /// SoundCloud draws its own waveform for every track, so nothing is measured here.
    fn measurable(&self) -> bool {
        false
    }
}

/// Resolves a track id to its bytes: fetch the track for its transcodings,
/// `pick` the best one, resolve that transcoding's own url to the cdn url,
/// then download it.
///
/// `progressive` is a single GET. `hls` resolves to a media playlist url
/// instead of a direct one, so it goes through `stream::assemble`, which
/// fetches that playlist fresh, then the init segment and every media
/// segment in order, concatenating them into one fragmented mp4 buffer.
/// Fetches a track's transcodings, `pick`s the best one, and resolves that
/// transcoding's own url to the short-lived cdn (or, for hls, media
/// playlist) url named in `media.transcodings[].url`.
///
/// This is the whole selection step `fetch` needs before it downloads
/// anything, split out so `soundcloud::resolve_playable_url` — the entry
/// point `live_tests::soundcloud` calls to exercise this same path live —
/// can share it instead of reimplementing it.
async fn resolve_transcoding(
    http: &Http,
    id: &str,
) -> Result<(Transcoding, String, Option<Duration>)> {
    let track: wire::Track = http
        .get_json(&format!("/tracks/{id}"), &[])
        .await
        .context("cannot fetch the soundcloud track")?;
    let duration = Some(Duration::from_millis(track.duration));
    let chosen = pick(&track.media.transcodings)
        .context("soundcloud offers no playable transcoding for this track")?
        .clone();
    let resolved = http
        .resolve_stream(&chosen.url)
        .await
        .context("cannot resolve the soundcloud stream")?;
    Ok((chosen, resolved, duration))
}

/// Resolves a track id to the playable url `load` would stream from, without
/// downloading it — anonymous when `token` is `None`.
///
/// Exposed to the crate for `live_tests::soundcloud`, which needs this exact
/// selection-and-resolve path exercised against the live api rather than
/// reimplemented by hand, but cannot see `Http` or `Transcoding`: both stay
/// private to `soundcloud`, so this takes and returns plain strings.
///
/// `cfg(test)`: its only caller is a `#[tokio::test]`, which the compiler
/// already strips outside test builds, so this would otherwise warn as dead
/// code in a plain `cargo build`/`clippy`.
#[cfg(all(test, feature = "spotify", feature = "youtube"))]
pub(crate) async fn resolve_playable_url(
    client_id: String,
    token: Option<String>,
    track_id: &str,
) -> Result<String> {
    let client_id = ClientId::new(
        client_id,
        std::env::temp_dir().join("sonora-test-client-id"),
    );
    let http = match token {
        Some(token) => Http::with_token(client_id, token),
        None => Http::anonymous(client_id),
    };
    let (_chosen, url, _duration) = resolve_transcoding(&http, track_id).await?;
    Ok(url)
}

async fn fetch(http: &Http, id: &str) -> Result<Loaded> {
    let (chosen, resolved, duration) = resolve_transcoding(http, id).await?;
    let data = match chosen.format.protocol.as_str() {
        "progressive" => http
            .fetch_bytes(&resolved)
            .await
            .context("cannot download the soundcloud stream")?,
        "hls" => stream::assemble(http.agent(), &resolved)
            .await
            .context("cannot assemble the soundcloud hls stream")?,
        other => anyhow::bail!("soundcloud offered an unrecognised stream protocol {other}"),
    };
    Ok(Loaded {
        data: Arc::new(data),
        duration,
    })
}

fn decode(data: Arc<Vec<u8>>) -> Result<rodio::Decoder<Cursor<Bytes>>> {
    let length = data.len() as u64;
    rodio::Decoder::builder()
        .with_data(Cursor::new(Bytes(data)))
        .with_byte_len(length)
        .with_seekable(true)
        .build()
        .context("cannot decode audio")
}

/// A downloaded track as the decoder reads it.
pub struct Bytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::pick;
    use crate::soundcloud::wire::{Format, Track, Transcoding};

    fn transcoding(protocol: &str, preset: &str) -> Transcoding {
        Transcoding {
            url: format!("https://example.test/{preset}"),
            preset: preset.to_string(),
            format: Format {
                protocol: protocol.to_string(),
            },
            legacy: false,
        }
    }

    #[test]
    fn prefers_hls_over_progressive() {
        let all = vec![
            transcoding("progressive", "mp3_0_0"),
            transcoding("hls", "aac_160k"),
        ];
        assert_eq!(pick(&all).unwrap().format.protocol, "hls");
    }

    #[test]
    fn falls_back_to_progressive_when_no_hls_is_offered() {
        let all = vec![transcoding("progressive", "mp3_0_0")];
        assert_eq!(pick(&all).unwrap().format.protocol, "progressive");
    }

    #[test]
    fn has_nothing_to_pick_from_an_empty_list() {
        assert!(pick(&[]).is_none());
    }

    #[test]
    fn ranks_hls_presets_by_a_table_not_the_quality_label() {
        // aac_160k and abr_sq both carry the "sq" quality label, so a
        // heuristic on that label alone could not tell them apart.
        let all = vec![
            transcoding("hls", "abr_sq"),
            transcoding("hls", "aac_160k"),
            transcoding("hls", "aac_96k"),
        ];
        assert_eq!(pick(&all).unwrap().preset, "aac_160k");
    }

    #[test]
    fn filters_on_protocol_since_mp3_0_0_exists_as_both_hls_and_progressive() {
        let all = vec![
            transcoding("hls", "mp3_0_0"),
            transcoding("progressive", "mp3_0_0"),
        ];
        assert_eq!(pick(&all).unwrap().format.protocol, "hls");
    }

    #[test]
    fn an_unknown_preset_stays_playable_rather_than_being_skipped() {
        let all = vec![transcoding("hls", "some_future_preset")];
        assert!(pick(&all).is_some());
    }

    #[test]
    fn picks_the_best_transcoding_from_the_captured_fixture() {
        let track: Track =
            serde_json::from_str(include_str!("fixtures/track.json")).expect("fixture parses");
        let chosen = pick(&track.media.transcodings).expect("the fixture offers transcodings");
        assert_eq!(chosen.preset, "aac_160k");
        assert_eq!(chosen.format.protocol, "hls");
    }
}
