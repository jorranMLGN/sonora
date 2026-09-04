use anyhow::{Context as _, Result};

use crate::soundcloud::http::Http;
use crate::soundcloud::wire::{self, Page};

const LIMIT: &str = "50";

pub async fn tracks(http: &Http, query: &str) -> Result<Vec<crate::Track>> {
    let page: Page<wire::Track> = http
        .get_json("/search/tracks", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud tracks")?;
    Ok(page.collection.into_iter().map(wire::track).collect())
}

pub async fn playlists(http: &Http, query: &str) -> Result<Vec<crate::Playlist>> {
    let page = raw_playlists(http, query).await?;
    Ok(page
        .collection
        .into_iter()
        .filter(|raw| !wire::is_album(raw))
        .map(wire::playlist)
        .collect())
}

pub async fn albums(http: &Http, query: &str) -> Result<Vec<crate::Album>> {
    let page = raw_playlists(http, query).await?;
    Ok(page
        .collection
        .into_iter()
        .filter(wire::is_album)
        .map(wire::album)
        .collect())
}

async fn raw_playlists(http: &Http, query: &str) -> Result<Page<wire::Playlist>> {
    http.get_json("/search/playlists", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud playlists")
}
