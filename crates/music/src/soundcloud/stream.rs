use anyhow::{Context as _, Result};
use reqwest::{Client, StatusCode};

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
/// HLS gained fragmented-MP4 segments at version 6. Below that, segments carry
/// their own headers and an absent `#EXT-X-MAP` is normal; at or above it, an
/// absent one means the media segments have no moov box and decode to silence.
/// A playlist with no `#EXT-X-VERSION` line at all defaults to `true`: the
/// only playlists this provider has actually seen (soundcloud's) declare
/// version 7, so treating an unversioned one as fragmented is the safer
/// default — a false negative here is silent, a false positive just refuses
/// loudly instead of playing something that happened to be raw audio.
fn needs_init_segment(playlist: &str) -> bool {
    playlist
        .lines()
        .find_map(|line| line.trim().strip_prefix("#EXT-X-VERSION:"))
        .and_then(|version| version.trim().parse::<u32>().ok())
        .is_none_or(|version| version >= 6)
}

/// Fetches a media playlist and assembles it into one buffer: the init
/// segment first (if any), then every media segment in listed order.
///
/// Sequential, not parallel — the segments must land in order and a track is
/// only a couple of dozen of them. The playlist is fetched fresh on every
/// call rather than cached: every segment URL carries `expires=` and a
/// CloudFront `Signature`, and a long pause before a seek can outlive them.
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

    let mut buffer = Vec::new();
    if let Some(init) = init {
        let bytes = fetch_segment(http, &init)
            .await
            .context("cannot fetch the hls init segment")?;
        buffer.extend_from_slice(&bytes);
    }
    for segment in segments(&playlist) {
        let bytes = fetch_segment(http, &segment)
            .await
            .context("cannot fetch an hls media segment")?;
        buffer.extend_from_slice(&bytes);
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
    use super::{init_segment, needs_init_segment, segments};

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

    #[test]
    fn a_version_7_playlist_needs_an_init_segment() {
        assert!(needs_init_segment(PLAYLIST));
    }

    #[test]
    fn a_version_3_playlist_needs_no_init_segment() {
        let playlist = "#EXTM3U\n\
            #EXT-X-VERSION:3\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.aac\n\
            #EXT-X-ENDLIST\n";
        assert!(!needs_init_segment(playlist));
    }

    #[test]
    fn a_playlist_with_no_version_line_defaults_to_needing_one() {
        let playlist = "#EXTM3U\n\
            #EXTINF:10.0,\n\
            https://cf-hls.sndcdn.com/a/0.aac\n\
            #EXT-X-ENDLIST\n";
        assert!(needs_init_segment(playlist));
    }
}
