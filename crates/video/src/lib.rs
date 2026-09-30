//! A short, silent H.264 clip played as frames: the decoder store, the download and the
//! looping player. Knows nothing about music or gpui; `music::video` is the one door into it.

pub mod codec;

use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context as _, Result};

/// How long a connection may take to open, and a whole download to finish, before it counts
/// as failed. Canvases are a few MB and Cisco's binary under one, so both are generous.
const CONNECT: Duration = Duration::from_secs(10);
const WHOLE: Duration = Duration::from_secs(60);

fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let built = reqwest::Client::builder()
        .connect_timeout(CONNECT)
        .timeout(WHOLE)
        .build()
        .context("cannot build the download client")?;
    Ok(CLIENT.get_or_init(|| built))
}

/// Fetches a file whole. Canvases are a few MB and Cisco's binary under one, so nothing streams.
pub async fn download(url: &str) -> Result<Vec<u8>> {
    let response = client()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("cannot reach {url}"))?
        .error_for_status()
        .with_context(|| format!("{url} refused the download"))?;
    Ok(response
        .bytes()
        .await
        .context("cannot read the download")?
        .to_vec())
}
