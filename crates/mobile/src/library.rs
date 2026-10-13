//! A provider's saved tracks, albums, artists and playlists, a page at a time.
//!
//! For now every page reads the whole list from the provider and slices it; milestone 4 moves
//! this onto the providers' own paging and a snapshot in `mobile.sqlite`.

use anyhow::{Context as _, Result};

use crate::Item;
use crate::core::{self, Core};

/// How many items one page holds.
const PAGE: usize = 50;

/// Which part of a library to list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Part {
    Songs,
    Albums,
    Artists,
    Playlists,
}

/// One page of a library part. `total` is the length of the whole part.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LibraryPage {
    pub items: Vec<Item>,
    pub total: u32,
}

/// Page `page` of `part` of the library of the provider `slug`.
pub async fn page(core: &Core, slug: &str, part: Part, page: u32) -> Result<LibraryPage> {
    let session = core
        .session(slug)
        .with_context(|| format!("cannot list the library: {slug} is not signed in"))?;
    let api = &session.api;
    let items: Vec<Item> = match part {
        Part::Songs => core.remember(&api.saved_tracks().await?),
        Part::Albums => api.saved_albums().await?.iter().map(core::album).collect(),
        Part::Playlists => api.playlists().await?.iter().map(core::playlist).collect(),
        Part::Artists => api
            .saved_artists()
            .await?
            .into_iter()
            .map(|artist| Item {
                provider: slug.to_owned(),
                id: artist.id,
                title: artist.name,
                subtitle: String::new(),
                artwork: artist.cover,
                duration_ms: 0,
                explicit: false,
                playable: true,
            })
            .collect(),
    };
    let total = items.len() as u32;
    let items = items
        .into_iter()
        .skip(page as usize * PAGE)
        .take(PAGE)
        .collect();
    Ok(LibraryPage { items, total })
}
