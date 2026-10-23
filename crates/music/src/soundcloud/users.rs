//! Artist pages, user profiles, and related-track radio.
//!
//! `/users/{id}/related_artists` returns 404 on the live API, so there is no
//! related-artist substitute here — `wire::artist_profile` simply carries no
//! such list. `images` fans out over many ids at once, the way
//! `spotify::profiles` does for display names, but bounds its concurrency:
//! a library view can ask for a hundred artists, and a hundred simultaneous
//! requests to one host is how a client gets rate-limited.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::soundcloud::http::{self, Http};
use crate::soundcloud::wire::{self, Page, User};

/// How many `GET /users/{id}` requests `images` keeps in flight at once.
const IMAGE_CONCURRENCY: usize = 8;
/// The most pages `every_page` reads, so a listing that never ends cannot loop forever.
const MAX_PAGES: usize = 20;

/// How many tracks one page of an artist's uploads asks for.
const UPLOADS_PAGE: usize = 200;

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

/// Every track `id` uploaded, newest first, behind the `known` ones the page already ranks.
/// Pages through `/users/{id}/tracks` until SoundCloud has no next page or
/// `ArtistCatalogue::TRACKS` are in hand.
pub async fn catalogue(
    http: &Http,
    id: &str,
    known: &[crate::Track],
) -> Result<crate::ArtistCatalogue> {
    let first = format!("/users/{id}/tracks?limit={UPLOADS_PAGE}&linked_partitioning=1");
    let raws = every_page(first, crate::ArtistCatalogue::TRACKS, |path| async move {
        http.get_json::<Page<wire::Track>>(&path, &[])
            .await
            .context("cannot fetch the soundcloud uploads")
    })
    .await?;
    let tracks = raws
        .into_iter()
        .map(|raw| remembered_track(http, raw))
        .collect();
    Ok(crate::ArtistCatalogue::every_track(known, tracks))
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

/// Where the track's waveform file lives, when it has one.
pub async fn waveform_url(http: &Http, id: &str) -> Result<Option<String>> {
    Ok(fetch_track(http, id)
        .await?
        .waveform_url
        .filter(|url| !url.is_empty()))
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

/// The items of a listing from `first` on, following each page's `next_href` through `fetch`
/// until there is none, a page comes back empty or `cap` items are in hand. A failed first page
/// is an error; a later one ends the walk with what came before it.
async fn every_page<T, F, Fut>(first: String, cap: usize, mut fetch: F) -> Result<Vec<T>>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Page<T>>>,
{
    let mut items = Vec::new();
    let mut next = Some(first);
    for _ in 0..MAX_PAGES {
        let Some(path) = next.take() else {
            break;
        };
        let page = match fetch(path).await {
            Ok(page) => page,
            Err(error) if !items.is_empty() => {
                log::warn!("soundcloud: cannot fetch the next page, keeping what came: {error:#}");
                break;
            }
            Err(error) => return Err(error),
        };
        if page.collection.is_empty() {
            break;
        }
        items.extend(page.collection);
        if items.len() >= cap {
            items.truncate(cap);
            break;
        }
        next = page.next_href.as_deref().and_then(http::next_path);
    }
    Ok(items)
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
    use std::cell::RefCell;

    use super::*;

    fn page(items: &[u32], next: Option<&str>) -> Page<u32> {
        Page {
            collection: items.to_vec(),
            next_href: next.map(str::to_owned),
        }
    }

    /// Answers each path from `pages`, writing down every path asked for.
    async fn walk(
        pages: &[(&str, Page<u32>)],
        cap: usize,
        asked: &RefCell<Vec<String>>,
    ) -> Result<Vec<u32>> {
        every_page("/users/7/tracks?limit=2".to_owned(), cap, |path| {
            asked.borrow_mut().push(path.clone());
            let found = pages
                .iter()
                .find(|(at, _)| *at == path)
                .map(|(_, page)| Page {
                    collection: page.collection.clone(),
                    next_href: page.next_href.clone(),
                });
            async move { found.with_context(|| format!("no page at {path}")) }
        })
        .await
    }

    #[tokio::test]
    async fn every_page_follows_next_href_until_there_is_none() {
        let pages = [
            (
                "/users/7/tracks?limit=2",
                page(
                    &[1, 2],
                    Some("https://api-v2.soundcloud.com/users/7/tracks?offset=b&limit=2"),
                ),
            ),
            ("/users/7/tracks?offset=b&limit=2", page(&[3], None)),
        ];
        let asked = RefCell::new(Vec::new());

        let items = walk(&pages, 500, &asked).await.unwrap();

        assert_eq!(items, [1, 2, 3]);
        assert_eq!(asked.borrow().len(), 2);
    }

    #[tokio::test]
    async fn every_page_stops_at_the_cap_without_asking_for_more() {
        let next = Some("https://api-v2.soundcloud.com/users/7/tracks?limit=2");
        let pages = [("/users/7/tracks?limit=2", page(&[1, 2, 3], next))];
        let asked = RefCell::new(Vec::new());

        let items = walk(&pages, 5, &asked).await.unwrap();

        assert_eq!(items, [1, 2, 3, 1, 2]);
        assert_eq!(asked.borrow().len(), 2);
    }

    #[tokio::test]
    async fn every_page_stops_after_max_pages_of_a_listing_that_never_ends() {
        let next = Some("https://api-v2.soundcloud.com/users/7/tracks?limit=2");
        let pages = [("/users/7/tracks?limit=2", page(&[1], next))];
        let asked = RefCell::new(Vec::new());

        let items = walk(&pages, usize::MAX, &asked).await.unwrap();

        assert_eq!(asked.borrow().len(), MAX_PAGES);
        assert_eq!(items.len(), MAX_PAGES);
    }

    #[tokio::test]
    async fn every_page_stops_at_an_empty_page() {
        let next = Some("https://api-v2.soundcloud.com/users/7/tracks?limit=2");
        let pages = [("/users/7/tracks?limit=2", page(&[], next))];
        let asked = RefCell::new(Vec::new());

        let items = walk(&pages, 500, &asked).await.unwrap();

        assert!(items.is_empty());
        assert_eq!(asked.borrow().len(), 1);
    }

    #[tokio::test]
    async fn every_page_keeps_what_it_has_when_a_later_page_fails() {
        let next = Some("https://api-v2.soundcloud.com/users/7/tracks?offset=gone");
        let pages = [("/users/7/tracks?limit=2", page(&[1, 2], next))];
        let asked = RefCell::new(Vec::new());

        assert_eq!(walk(&pages, 500, &asked).await.unwrap(), [1, 2]);
        assert!(walk(&[], 500, &asked).await.is_err());
    }

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
