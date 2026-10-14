//! SoundCloud's own waveform for a track: the `waveform_url` on the track, which names a json
//! file on `wave.sndcdn.com` holding `{ width, height, samples }`, every sample a bar height
//! from 0 to `height`.

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::Waveform;
use crate::soundcloud::http::Http;
use crate::soundcloud::users;

/// The file `waveform_url` names, as `wave.sndcdn.com` serves it.
#[derive(Deserialize)]
struct Wave {
    #[serde(default)]
    samples: Vec<f32>,
}

/// Fetches and reads the waveform SoundCloud drew for `id`. `None` when the track has none.
pub(super) async fn waveform(http: &Http, id: &str) -> Result<Option<Waveform>> {
    let Some(url) = users::waveform_url(http, id).await? else {
        return Ok(None);
    };
    // the CDN file is public: no client id, no token
    let response = http
        .agent()
        .get(json_url(&url))
        .send()
        .await
        .context("cannot reach the soundcloud waveform")?
        .error_for_status()
        .context("cannot fetch the soundcloud waveform")?;
    let body = response
        .bytes()
        .await
        .context("cannot read the soundcloud waveform")?;
    parse(&body)
}

/// Reads the waveform file. Its `height` is left aside: the samples are scaled so the loudest
/// is 1, as an engine-measured waveform is, so both draw alike.
fn parse(body: &[u8]) -> Result<Option<Waveform>> {
    let wave: Wave =
        serde_json::from_slice(body).context("cannot parse the soundcloud waveform")?;
    Ok(Waveform::new(&wave.samples))
}

/// The json form of a waveform url. Older tracks still name the `.png` rendering of it.
fn json_url(url: &str) -> String {
    match url.strip_suffix(".png") {
        Some(stem) => format!("{stem}.json"),
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{json_url, parse};

    #[test]
    fn the_samples_are_read_and_scaled_to_the_loudest() {
        let body = br#"{"width":4,"height":140,"samples":[0,35,70,140]}"#;

        let waveform = parse(body).unwrap().unwrap();

        assert_eq!(waveform.levels(), &[0., 0.25, 0.5, 1.]);
    }

    #[test]
    fn a_full_width_file_is_pooled_to_the_bucket_count() {
        let samples: Vec<String> = (0..1800).map(|index| (index % 140).to_string()).collect();
        let body = format!(
            r#"{{"width":1800,"height":140,"samples":[{}]}}"#,
            samples.join(",")
        );

        let waveform = parse(body.as_bytes()).unwrap().unwrap();

        assert_eq!(waveform.levels().len(), crate::WAVEFORM_BUCKETS);
    }

    #[test]
    fn a_file_without_samples_is_no_waveform() {
        assert!(
            parse(br#"{"width":1800,"height":140,"samples":[]}"#)
                .unwrap()
                .is_none()
        );
        assert!(parse(br#"{"width":1800,"height":140}"#).unwrap().is_none());
    }

    #[test]
    fn a_broken_file_is_an_error() {
        assert!(parse(b"<html>").is_err());
    }

    #[test]
    fn a_png_url_is_asked_for_as_json() {
        assert_eq!(
            json_url("https://wave.sndcdn.com/abc_m.png"),
            "https://wave.sndcdn.com/abc_m.json"
        );
        assert_eq!(
            json_url("https://wave.sndcdn.com/abc_m.json"),
            "https://wave.sndcdn.com/abc_m.json"
        );
    }
}
