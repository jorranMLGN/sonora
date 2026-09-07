//! Artist pages, user profiles, and related-track radio.
//!
//! `/users/{id}/related_artists` returns 404 on the live API, so there is no
//! related-artist substitute here — `wire::artist_profile` simply carries no
//! such list. `images` fans out over many ids at once, the way
//! `spotify::profiles` does for display names, but bounds its concurrency:
//! a library view can ask for a hundred artists, and a hundred simultaneous
//! requests to one host is how a client gets rate-limited.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::soundcloud::http::Http;
use crate::soundcloud::wire::{self, Page, User};

/// How many `GET /users/{id}` requests `images` keeps in flight at once.
const IMAGE_CONCURRENCY: usize = 8;

async fn fetch_user(http: &Http, id: &str) -> Result<User> {
    let raw: User = http
        .get_json(&format!("/users/{id}"), &[])
        .await
        .context("cannot fetch the soundcloud user")?;
    http.remember_permalink(raw.id, raw.permalink_url.clone());
    Ok(raw)
}

pub async fn artist(http: &Http, id: &str) -> Result<crate::Artist> {
    let raw = fetch_user(http, id).await?;
    let top = top_tracks(http, id)
        .await
        .inspect_err(|error| log::warn!("soundcloud: cannot load top tracks for {id}: {error:#}"))
        .unwrap_or_default();
    Ok(wire::artist(raw, top))
}

pub async fn profile(http: &Http, id: &str) -> Result<crate::ArtistProfile> {
    let raw = fetch_user(http, id).await?;
    Ok(wire::artist_profile(raw))
}

pub async fn detail(http: &Http, id: &str) -> Result<crate::UserDetail> {
    let raw = fetch_user(http, id).await?;
    Ok(wire::user_detail(raw))
}

pub async fn top_tracks(http: &Http, id: &str) -> Result<Vec<crate::Track>> {
    let page: Page<wire::Track> = http
        .get_json(&format!("/users/{id}/toptracks"), &[])
        .await
        .context("cannot fetch the soundcloud top tracks")?;
    Ok(page
        .collection
        .into_iter()
        .map(|raw| remembered_track(http, raw))
        .collect())
}

async fn fetch_track(http: &Http, id: &str) -> Result<wire::Track> {
    let raw: wire::Track = http
        .get_json(&format!("/tracks/{id}"), &[])
        .await
        .context("cannot fetch the soundcloud track")?;
    http.remember_permalink(raw.id, raw.permalink_url.clone());
    Ok(raw)
}

pub async fn track(http: &Http, id: &str) -> Result<crate::Track> {
    Ok(wire::track(fetch_track(http, id).await?))
}

pub async fn track_playcount(http: &Http, id: &str) -> Result<Option<u64>> {
    Ok(fetch_track(http, id).await?.playback_count)
}

pub async fn related(http: &Http, track_id: &str) -> Result<Vec<crate::Track>> {
    let page: Page<wire::Track> = http
        .get_json(&format!("/tracks/{track_id}/related"), &[])
        .await
        .context("cannot fetch the soundcloud related tracks")?;
    Ok(page
        .collection
        .into_iter()
        .map(|raw| remembered_track(http, raw))
        .collect())
}

fn remembered_track(http: &Http, raw: wire::Track) -> crate::Track {
    http.remember_permalink(raw.id, raw.permalink_url.clone());
    wire::track(raw)
}

/// Fetches an avatar per id, bounded to `IMAGE_CONCURRENCY` requests in
/// flight at once. An id that fails to resolve is left out of the result
/// rather than failing the whole batch.
pub async fn images(http: &Http, ids: Vec<String>) -> Result<HashMap<String, String>> {
    let limit = Arc::new(Semaphore::new(IMAGE_CONCURRENCY));
    let mut pending = JoinSet::new();
    for id in ids {
        let http = http.clone();
        let limit = limit.clone();
        pending.spawn(async move {
            let _permit = limit.acquire_owned().await.ok()?;
            let raw = fetch_user(&http, &id)
                .await
                .inspect_err(|error| {
                    log::debug!("soundcloud: cannot load avatar for {id}: {error:#}")
                })
                .ok()?;
            let avatar = raw.avatar_url?;
            Some((id, avatar))
        });
    }

    let mut images = HashMap::new();
    while let Some(joined) = pending.join_next().await {
        if let Ok(Some((id, avatar))) = joined {
            images.insert(id, avatar);
        }
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_tracks_and_related_remember_every_track_permalink() {
        let http = Http::anonymous(crate::soundcloud::auth::ClientId::new(
            "client".to_string(),
            std::env::temp_dir().join("sonora-test-client-id"),
        ));
        let raw: wire::Track = serde_json::from_str(include_str!("fixtures/track.json")).unwrap();
        let id = raw.id.to_string();
        let permalink = raw.permalink_url.clone().unwrap();

        let converted = remembered_track(&http, raw);

        assert_eq!(converted.id.as_deref(), Some(id.as_str()));
        assert_eq!(http.permalink(&id).as_deref(), Some(permalink.as_str()));
    }
}
