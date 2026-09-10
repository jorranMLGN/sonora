use std::sync::Arc;

use anyhow::{Context as _, Result};
use reqwest::{Client, StatusCode};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// How many segment requests `assemble` keeps in flight at once.
///
/// The same bound `users::images` uses: enough to hide per-request latency,
/// few enough not to look like a flood to one host.
const SEGMENT_CONCURRENCY: usize = 8;

/// The initialisation segment URI, from the `#EXT-X-MAP` line.
///
/// HLS uses `#` for both comments and directives. This one names the fMP4
/// init segment carrying the moov box; without it the media segments are
/// headerless and decode to silence.
pub fn init_segment(playlist: &str) -> Option<String> {
    playlist.lines().find_map(|line| {
        let line = line.trim();
        let attrs = line.strip_prefix("#EXT-X-MAP:")?;
        attrs.split(',').find_map(|attr| {
            attr.trim()
                .strip_prefix("URI=\"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string)
        })
    })
}

/// Reads the media segment URLs out of a media playlist, in listed order.
///
/// This is not an adaptive HLS client. SoundCloud hands out a single-variant
/// media playlist per transcoding, so every non-comment line is a segment and
/// the quality was already chosen by `playback::pick`. The `#EXT-X-MAP` line
/// is excluded here even though it does not start a comment in the usual
/// sense: its URI is returned separately by `init_segment` and must be
/// fetched first.
pub fn segments(playlist: &str) -> Vec<String> {
    playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Whether this playlist's segments need an initialisation segment to decode.
///
/// Fragmented-mp4 segments carry no headers of their own: without the moov
/// box out of `#EXT-X-MAP` they decode to silence. Every other container
/// soundcloud serves carries its own headers and needs nothing.
///
/// The segment name says which one it is, and `#EXT-X-VERSION` does not —
/// that names the playlist features in use, not the container. Soundcloud
/// serves its mp3 transcodings as version 6 with self-contained `.mp3`
/// segments and no `#EXT-X-MAP`, which is correct and playable, so reading
/// the version as a container refused every track offering no aac
/// transcoding at all.
///
/// This therefore asks for positive evidence of fragmented mp4 rather than
/// assuming it: a segment name it does not recognise is played, not refused.
fn needs_init_segment(playlist: &str) -> bool {
    segments(playlist).iter().any(|url| fragmented(url))
}

/// Whether a segment url names a fragmented-mp4 file.
///
/// The name is the last path element with any query stripped: every
/// soundcloud segment url carries an `expires`/`Signature` query, and some
/// names carry dots of their own (`Npsj1UIY4m9e.128.mp3`), so only the last
/// extension of that element counts.
fn fragmented(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name.rsplit_once('.').map(|(_, extension)| extension),
        Some("m4s" | "mp4" | "fmp4")
    )
}

/// Fetches a media playlist and assembles it into one buffer: the init
/// segment first (if any), then every media segment in listed order.
///
/// The segments have to be *concatenated* in order, not *fetched* in order,
/// so they are fetched `SEGMENT_CONCURRENCY` at a time and placed by index.
/// Nothing plays until the whole buffer is here — the same shape
/// `youtube::playback` has — but that provider gets its track in one
/// response, where an hls track is a few hundred, and paying each round trip
/// end to end made the wait grow with the track's length: measured at 1.2s
/// for a 3½-minute track and 8.7s for a 26-minute one.
///
/// The playlist is fetched fresh on every call rather than cached: every
/// segment URL carries `expires=` and a CloudFront `Signature`, and a long
/// pause before a seek can outlive them.
pub async fn assemble(http: &Client, url: &str) -> Result<Vec<u8>> {
    let playlist = fetch_text(http, url)
        .await
        .context("cannot fetch the hls media playlist")?;

    let init = init_segment(&playlist);
    if init.is_none() && needs_init_segment(&playlist) {
        anyhow::bail!(
            "this hls playlist declares fragmented mp4 segments but names no \
             init segment, so the audio would decode to silence"
        );
    }

    let mut urls = Vec::new();
    urls.extend(init);
    urls.extend(segments(&playlist));

    let limit = Arc::new(Semaphore::new(SEGMENT_CONCURRENCY));
    let mut pending = JoinSet::new();
    for (place, url) in urls.iter().cloned().enumerate() {
        let http = http.clone();
        let limit = limit.clone();
        pending.spawn(async move {
            let _permit = limit.acquire_owned().await;
            (place, fetch_segment(&http, &url).await)
        });
    }

    let mut parts: Vec<Option<Vec<u8>>> = vec![None; urls.len()];
    while let Some(joined) = pending.join_next().await {
        let (place, fetched) = joined.context("an hls segment fetch did not finish")?;
        parts[place] = Some(fetched.context("cannot fetch an hls segment")?);
    }

    let mut buffer = Vec::with_capacity(parts.iter().flatten().map(Vec::len).sum());
    for part in parts.into_iter().flatten() {
        buffer.extend_from_slice(&part);
    }
    Ok(buffer)
}

async fn fetch_text(http: &Client, url: &str) -> Result<String> {
    let response = http
        .get(url)
        .send()
        .await
        .context("cannot reach soundcloud's cdn")?;
    let status = response.status();
    if status == StatusCode::FORBIDDEN {
        anyhow::bail!("soundcloud's cdn rejected the url, its signature likely expired");
    }
    if !status.is_success() {
        anyhow::bail!("soundcloud's cdn refused the request with {status}");
    }
    response
        .text()
        .await
        .context("cannot read the hls playlist")
}

async fn fetch_segment(http: &Client, url: &str) -> Result<Vec<u8>> {
    let response = http
        .get(url)
        .send()
        .await
        .context("cannot reach soundcloud's cdn")?;
    let status = response.status();
    if status == StatusCode::FORBIDDEN {
        anyhow::bail!("soundcloud's cdn rejected the segment url, its signature likely expired");
    }
    if !status.is_success() {
        anyhow::bail!("soundcloud's cdn refused the segment with {status}");
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .context("cannot read the hls segment")
}

#[cfg(test)]
mod tests {
    use super::{fragmented, init_segment, needs_init_segment, segments};

    const PLAYLIST: &str = "#EXTM3U\n\
        #EXT-X-VERSION:7\n\
        #EXT-X-TARGETDURATION:10\n\
        #EXT-X-PLAYLIST-TYPE:VOD\n\
        #EXT-X-MAP:URI=\"https://cf-hls.sndcdn.com/a/aac_160k/init.mp4?expires=1&Signature=abc\"\n\
        #EXTINF:10.007800,\n\
        https://cf-hls.sndcdn.com/a/aac_160k/data000.m4s?expires=1&Signature=abc\n\
        #EXTINF:10.007800,\n\
        https://cf-hls.sndcdn.com/a/aac_160k/data001.m4s?expires=1&Signature=abc\n\
        #EXT-X-ENDLIST\n";

    #[test]
    fn finds_the_init_segment_uri_on_the_ext_x_map_line() {
        assert_eq!(
            init_segment(PLAYLIST),
            Some(
                "https://cf-hls.sndcdn.com/a/aac_160k/init.mp4?expires=1&Signature=abc".to_string()
            )
        );
    }

    #[test]
    fn excludes_the_init_segment_from_the_media_segments() {
        assert_eq!(
            segments(PLAYLIST),
            vec![
                "https://cf-hls.sndcdn.com/a/aac_160k/data000.m4s?expires=1&Signature=abc"
                    .to_string(),
                "https://cf-hls.sndcdn.com/a/aac_160k/data001.m4s?expires=1&Signature=abc"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn has_no_init_segment_when_the_playlist_carries_no_ext_x_map_line() {
        let playlist = "#EXTM3U\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.aac\n\
            #EXT-X-ENDLIST\n";
        assert_eq!(init_segment(playlist), None);
    }

    #[test]
    fn lists_segments_in_order() {
        let playlist = "#EXTM3U\n\
            #EXT-X-VERSION:3\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.aac\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/1.aac\n\
            #EXT-X-ENDLIST\n";
        assert_eq!(
            segments(playlist),
            vec![
                "https://cf-hls.sndcdn.com/a/0.aac".to_string(),
                "https://cf-hls.sndcdn.com/a/1.aac".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        assert!(segments("#EXTM3U\n\n#EXT-X-ENDLIST\n").is_empty());
    }

    /// Soundcloud serves its mp3 transcodings exactly like this: version 6,
    /// self-contained `.mp3` segments, and no `#EXT-X-MAP` — correct, and
    /// playable. Tracks that offer no aac transcoding at all have only this.
    const MP3_PLAYLIST: &str = "#EXTM3U\n\
        #EXT-X-VERSION:6\n\
        #EXT-X-PLAYLIST-TYPE:VOD\n\
        #EXT-X-TARGETDURATION:10\n\
        #EXT-X-MEDIA-SEQUENCE:0\n\
        #EXTINF:1.985272,\n\
        https://cf-hls-media.sndcdn.com/media/159660/0/31762/Npsj1UIY4m9e.128.mp3?Policy=abc\n\
        #EXTINF:2.977908,\n\
        https://cf-hls-media.sndcdn.com/media/159660/1/31762/Npsj1UIY4m9e.128.mp3?Policy=abc\n\
        #EXT-X-ENDLIST\n";

    #[test]
    fn mp3_segments_need_no_init_segment_whatever_the_version_says() {
        assert!(!needs_init_segment(MP3_PLAYLIST));
    }

    #[test]
    fn a_version_7_playlist_needs_an_init_segment() {
        assert!(needs_init_segment(PLAYLIST));
    }

    #[test]
    fn self_contained_segments_need_no_init_segment() {
        let playlist = "#EXTM3U\n\
            #EXT-X-VERSION:3\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.aac\n\
            #EXT-X-ENDLIST\n";
        assert!(!needs_init_segment(playlist));
    }

    #[test]
    fn a_segment_name_it_does_not_recognise_is_played_not_refused() {
        let playlist = "#EXTM3U\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.weird\n\
            #EXT-X-ENDLIST\n";
        assert!(!needs_init_segment(playlist));
    }

    // The segment name is what decides, so these pin the two ways soundcloud
    // makes it hard to read: a signed query after the extension, and dots
    // inside the name itself.

    #[test]
    fn the_query_string_does_not_hide_the_extension() {
        assert!(fragmented(
            "https://x.sndcdn.com/a/data000.m4s?expires=1&Signature=abc"
        ));
        assert!(!fragmented(
            "https://x.sndcdn.com/a/0.mp3?expires=1&Signature=abc"
        ));
    }

    #[test]
    fn only_the_last_extension_of_the_name_counts() {
        assert!(!fragmented(
            "https://x.sndcdn.com/media/0/Npsj1UIY4m9e.128.mp3?Policy=abc"
        ));
        assert!(fragmented("https://x.sndcdn.com/a/seg.128.mp4"));
    }
}
