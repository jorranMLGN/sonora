//! Home, a provider's saved tracks, albums, artists and playlists, the album, playlist and
//! artist pages, and the saved toggles.
//!
//! A library part is read through the provider's own paging and handed out in pages of
//! `PAGE`, so the first rows draw while the rest still arrive. The first page of every part is
//! kept in `mobile.sqlite`, so Library draws it again before the network answers.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow};
use music::{GenreItem, GenreSection, HomeFeed, MusicApi, Pages, SavedArtist, tag};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::core::{self, Core, Session};
use crate::store::Store;
use crate::{Item, SessionState};

/// How many items one page holds.
const PAGE: usize = 50;
/// How many pages of a playlist `playlist` follows before it leaves the rest to
/// `playlist_more`.
const PLAYLIST_PAGES: usize = 4;
/// Every part, in the order the app lists them.
const PARTS: [Part; 4] = [Part::Songs, Part::Albums, Part::Artists, Part::Playlists];

/// Which part of a library to list, and which kind of item a saved toggle is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Part {
    Songs,
    Albums,
    Artists,
    Playlists,
}

/// One page of a library part. `total` is the length of the whole part when the provider
/// says it, or the count so far; `has_more` says whether another page follows.
#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct LibraryPage {
    pub items: Vec<Item>,
    pub total: u32,
    pub has_more: bool,
}

/// What a shelf on Home is, so the app can name the two the provider does not title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SectionKind {
    /// What the account played lately, in every shape the provider lists it.
    ListenAgain,
    /// The provider's quick picks, all tracks.
    QuickPicks,
    /// A shelf the provider titled itself.
    Shelf,
}

/// One shelf on Home, from one provider, its mixed cards split by kind.
#[derive(Clone, Debug, uniffi::Record)]
pub struct HomeSection {
    /// Unique on the page, as `spotify:3`.
    pub id: String,
    /// The provider's own title; empty for `ListenAgain` and `QuickPicks`.
    pub title: String,
    /// The provider's display name.
    pub provider: String,
    pub kind: SectionKind,
    pub tracks: Vec<Item>,
    pub albums: Vec<Item>,
    pub artists: Vec<Item>,
    pub playlists: Vec<Item>,
}

/// An album page.
#[derive(Clone, Debug, uniffi::Record)]
pub struct AlbumDetail {
    pub album: Item,
    pub tracks: Vec<Item>,
}

/// A playlist page. `continuation` fetches the tracks after these through `playlist_more`.
#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaylistDetail {
    pub playlist: Item,
    pub tracks: Vec<Item>,
    pub continuation: Option<String>,
}

/// The next stretch of a playlist, and the continuation after it.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct PlaylistMore {
    pub tracks: Vec<Item>,
    pub continuation: Option<String>,
}

/// An artist page.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ArtistDetail {
    pub artist: Item,
    pub top_tracks: Vec<Item>,
    pub albums: Vec<Item>,
}

/// Every id known to be in a library, across the signed-in providers, by kind.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SavedIds {
    pub tracks: Vec<String>,
    pub albums: Vec<String>,
    pub artists: Vec<String>,
    pub playlists: Vec<String>,
}

/// The core's library state: the open listings, what is known to be saved, and the snapshot
/// store.
pub struct Library {
    store: Mutex<Store>,
    listings: Mutex<HashMap<(String, Part), Shared>>,
    saved: Mutex<HashMap<(String, Part), HashSet<String>>>,
}

/// One library part as it is read: what arrived so far and the pages still to come.
#[derive(Default)]
struct Listing {
    /// The session the pages come from, so a new session starts the listing over.
    source: Option<Arc<dyn MusicApi>>,
    items: Vec<Item>,
    total: Option<usize>,
    pages: Option<mpsc::Receiver<Result<Lot>>>,
}

/// One provider page, mapped: the total the provider gave, if any, and the items.
type Lot = (Option<usize>, Vec<Item>);

/// A listing that one reader at a time continues, across awaits.
type Shared = Arc<tokio::sync::Mutex<Listing>>;

impl Library {
    /// Opens the library's own connection to `mobile.sqlite`.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            store: Mutex::new(Store::open(path)?),
            listings: Mutex::default(),
            saved: Mutex::default(),
        })
    }

    fn listing(&self, slug: &str, part: Part) -> Shared {
        let mut listings = self
            .listings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        listings.entry((slug.to_owned(), part)).or_default().clone()
    }

    fn mark(&self, slug: &str, part: Part, ids: impl IntoIterator<Item = String>) {
        if let Ok(mut saved) = self.saved.lock() {
            saved
                .entry((slug.to_owned(), part))
                .or_default()
                .extend(ids);
        }
    }

    fn replace(&self, slug: &str, part: Part, ids: HashSet<String>) {
        if let Ok(mut saved) = self.saved.lock() {
            saved.insert((slug.to_owned(), part), ids);
        }
    }

    fn unmark(&self, slug: &str, part: Part, id: &str) {
        if let Ok(mut saved) = self.saved.lock()
            && let Some(ids) = saved.get_mut(&(slug.to_owned(), part))
        {
            ids.remove(id);
        }
    }

    fn snapshot(&self, slug: &str, part: Part) -> Option<LibraryPage> {
        self.store.lock().ok()?.load(&snapshot_key(slug, part))
    }

    fn keep(&self, slug: &str, part: Part, page: &LibraryPage) {
        if let Ok(store) = self.store.lock() {
            store.save(&snapshot_key(slug, part), page);
        }
    }
}

impl Listing {
    /// Starts the listing over from the provider's first page.
    fn restart(&mut self, core: &Arc<Core>, api: Arc<dyn MusicApi>, part: Part) {
        self.pages = Some(stream(core.clone(), api.clone(), part));
        self.source = Some(api);
        self.items.clear();
        self.total = None;
    }

    /// Reads pages until `wanted` items have arrived or the listing ends. Answers true when
    /// this call read the listing to its end.
    async fn fill(&mut self, wanted: usize) -> Result<bool> {
        while self.items.len() < wanted {
            let Some(pages) = self.pages.as_mut() else {
                return Ok(false);
            };
            match pages.recv().await {
                Some(Ok((total, items))) => {
                    self.total = total.or(self.total);
                    self.items.extend(items);
                }
                Some(Err(error)) => {
                    self.pages = None;
                    return Err(error);
                }
                None => {
                    self.pages = None;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn page(&self, page: usize) -> LibraryPage {
        let start = page * PAGE;
        let items: Vec<Item> = self.items.iter().skip(start).take(PAGE).cloned().collect();
        let has_more = self.items.len() > start + PAGE || self.pages.is_some();
        let total = self.total.unwrap_or(self.items.len()).max(self.items.len());
        LibraryPage {
            items,
            total: total as u32,
            has_more,
        }
    }
}

/// The home feed of every provider with a session, guests included, woven a shelf at a time
/// so no provider owns the top of the page. One provider failing leaves the others' shelves;
/// only all of them failing is an error.
pub async fn home(core: Arc<Core>) -> Result<Vec<HomeSection>> {
    let asked: Vec<(String, String, Session)> = core
        .providers()
        .into_iter()
        .filter_map(|info| {
            let session = core.session(&info.slug)?;
            Some((info.slug, info.name, session))
        })
        .collect();
    if asked.is_empty() {
        return Ok(Vec::new());
    }
    let feeds = asked.iter().map(|(slug, name, session)| {
        let core = core.clone();
        async move {
            let feed = latest(&session.api).await;
            (slug, feed.map(|feed| lane(&core, slug, name, feed)))
        }
    });
    let mut lanes = Vec::new();
    let mut failed = Vec::new();
    for (slug, lane) in futures::future::join_all(feeds).await {
        match lane {
            Ok(lane) => lanes.push(lane),
            Err(error) => {
                log::warn!("home: {slug} failed: {error:#}");
                failed.push(error);
            }
        }
    }
    if lanes.is_empty()
        && let Some(error) = failed.pop()
    {
        return Err(error.context("cannot load home"));
    }
    Ok(woven(lanes))
}

/// Page `page` of `part` of the library of the provider `slug`. Page zero reads the part
/// afresh and replaces the snapshot; the pages after it continue that reading.
pub async fn page(core: Arc<Core>, slug: &str, part: Part, page: u32) -> Result<LibraryPage> {
    let session = core
        .session(slug)
        .with_context(|| format!("cannot list the library: {slug} is not signed in"))?;
    if !signed_in(&core, slug) {
        return Ok(LibraryPage::default());
    }
    let page = page as usize;
    let listing = core.library.listing(slug, part);
    let mut listing = listing.lock().await;
    let same = listing
        .source
        .as_ref()
        .is_some_and(|source| Arc::ptr_eq(source, &session.api));
    if page == 0 || !same {
        listing.restart(&core, session.api.clone(), part);
    }
    let ended = listing.fill((page + 1) * PAGE + 1).await?;
    core.library
        .mark(slug, part, listing.items.iter().map(|item| item.id.clone()));
    if ended {
        let ids = listing.items.iter().map(|item| item.id.clone()).collect();
        core.library.replace(slug, part, ids);
    }
    let answer = listing.page(page);
    if page == 0 {
        core.library.keep(slug, part, &answer);
    }
    Ok(answer)
}

/// The first page of `part` as it was last listed, from `mobile.sqlite`, or `None` before the
/// first listing.
pub fn snapshot(core: &Core, slug: &str, part: Part) -> Option<LibraryPage> {
    core.library.snapshot(slug, part)
}

/// Every id in the libraries of the signed-in providers. The first call reads each library
/// to its end; the next ones answer from what is known, kept current by the listings and by
/// `set_saved`.
pub async fn saved(core: Arc<Core>) -> Result<SavedIds> {
    let slugs: Vec<String> = core
        .providers()
        .into_iter()
        .map(|info| info.slug)
        .filter(|slug| signed_in(&core, slug))
        .collect();
    let reads = slugs.iter().flat_map(|slug| {
        let core = core.clone();
        PARTS.into_iter().map(move |part| {
            let core = core.clone();
            async move {
                if let Err(error) = read_all(&core, slug, part).await {
                    log::warn!("library: cannot read {slug} {part:?}: {error:#}");
                }
            }
        })
    });
    futures::future::join_all(reads).await;
    let saved = core
        .library
        .saved
        .lock()
        .map_err(|_| anyhow!("the saved ids are poisoned"))?;
    let mut ids = SavedIds::default();
    for ((slug, part), set) in saved.iter() {
        if !slugs.contains(slug) {
            continue;
        }
        let list = match part {
            Part::Songs => &mut ids.tracks,
            Part::Albums => &mut ids.albums,
            Part::Artists => &mut ids.artists,
            Part::Playlists => &mut ids.playlists,
        };
        list.extend(set.iter().cloned());
    }
    Ok(ids)
}

/// Puts the item `id` of kind `part` into its provider's library, or takes it out.
pub async fn set_saved(core: Arc<Core>, id: &str, part: Part, saved: bool) -> Result<()> {
    let (slug, api) = api(&core, id)?;
    match part {
        Part::Songs => api.set_track_saved(id, saved).await,
        Part::Albums => api.set_album_saved(id, saved).await,
        Part::Artists => api.set_artist_saved(id, saved).await,
        Part::Playlists if saved => api.add_playlist_to_library(id).await,
        Part::Playlists => api.remove_playlist_from_library(id).await,
    }
    .context(match saved {
        true => "cannot save",
        false => "cannot remove from the library",
    })?;
    match saved {
        true => core.library.mark(&slug, part, [id.to_owned()]),
        false => core.library.unmark(&slug, part, id),
    }
    Ok(())
}

/// The album `id` with its tracks.
pub async fn album(core: Arc<Core>, id: &str) -> Result<AlbumDetail> {
    let (_, api) = api(&core, id)?;
    let detail = api.album(id).await.context("cannot load the album")?;
    let mut album = core::album(&detail.album);
    album.artwork = detail
        .cover_max
        .or(detail.album.cover_large.clone())
        .or(album.artwork);
    Ok(AlbumDetail {
        album,
        tracks: core.remember(&detail.tracks),
    })
}

/// The playlist `id` with its first tracks, following the provider's continuations a few
/// times so most playlists arrive whole.
pub async fn playlist(core: Arc<Core>, id: &str) -> Result<PlaylistDetail> {
    let (_, api) = api(&core, id)?;
    let detail = api.playlist(id).await.context("cannot load the playlist")?;
    let mut tracks = core.remember(&detail.tracks);
    let mut continuation = detail.continuation;
    for _ in 1..PLAYLIST_PAGES {
        let Some(from) = continuation.take() else {
            break;
        };
        let more = more(&core, &api, &from).await;
        tracks.extend(more.tracks);
        continuation = more.continuation;
    }
    Ok(PlaylistDetail {
        playlist: core::playlist(&detail.playlist),
        tracks,
        continuation,
    })
}

/// The tracks of playlist `id` after `continuation`. A provider that cannot continue ends
/// the playlist there rather than failing the page.
pub async fn playlist_more(core: Arc<Core>, id: &str, continuation: &str) -> Result<PlaylistMore> {
    let (_, api) = api(&core, id)?;
    Ok(more(&core, &api, continuation).await)
}

/// The artist `id` with their top tracks and albums.
pub async fn artist(core: Arc<Core>, id: &str) -> Result<ArtistDetail> {
    let (_, api) = api(&core, id)?;
    let found = api.artist(id).await.context("cannot load the artist")?;
    let catalogue = api
        .artist_catalogue(id, &found.top_tracks)
        .await
        .inspect_err(|error| log::warn!("library: no catalogue for {id}: {error:#}"))
        .unwrap_or_default();
    let top_tracks = match catalogue.top_tracks.is_empty() {
        true => found.top_tracks,
        false => catalogue.top_tracks,
    };
    let albums = match catalogue.albums.is_empty() {
        true => found.albums,
        false => catalogue.albums,
    };
    Ok(ArtistDetail {
        artist: Item {
            provider: core::provider(id),
            id: id.to_owned(),
            title: found.name,
            subtitle: String::new(),
            album: String::new(),
            artwork: found.cover_large,
            duration_ms: 0,
            explicit: false,
            playable: true,
        },
        top_tracks: core.remember(&top_tracks),
        albums: albums.iter().map(core::album).collect(),
    })
}

/// A saved artist as the phone draws it.
pub fn artist_item(artist: &SavedArtist) -> Item {
    Item {
        provider: core::provider(&artist.id),
        id: artist.id.clone(),
        title: artist.name.clone(),
        subtitle: String::new(),
        album: String::new(),
        artwork: artist.cover.clone(),
        duration_ms: 0,
        explicit: false,
        playable: true,
    }
}

/// Whether `slug` has a session with an account, which is what a library needs.
fn signed_in(core: &Core, slug: &str) -> bool {
    core.providers()
        .into_iter()
        .any(|info| info.slug == slug && matches!(info.session, SessionState::SignedIn { .. }))
}

/// The slug and the api of the provider the tagged `id` belongs to.
fn api(core: &Core, id: &str) -> Result<(String, Arc<dyn MusicApi>)> {
    let slug = tag::slug_of(id).with_context(|| format!("{id} names no provider"))?;
    let session = core
        .session(slug)
        .with_context(|| format!("{slug} is not signed in"))?;
    Ok((slug.to_owned(), session.api))
}

/// Reads `part` of `slug` to its end, unless a listing already did.
async fn read_all(core: &Arc<Core>, slug: &str, part: Part) -> Result<()> {
    let session = core
        .session(slug)
        .with_context(|| format!("{slug} is not signed in"))?;
    let listing = core.library.listing(slug, part);
    let mut listing = listing.lock().await;
    let same = listing
        .source
        .as_ref()
        .is_some_and(|source| Arc::ptr_eq(source, &session.api));
    if !same {
        listing.restart(core, session.api.clone(), part);
    } else if listing.pages.is_none() {
        return Ok(());
    }
    let ended = listing.fill(usize::MAX).await?;
    let ids: HashSet<String> = listing.items.iter().map(|item| item.id.clone()).collect();
    match ended {
        true => core.library.replace(slug, part, ids),
        false => core.library.mark(slug, part, ids),
    }
    Ok(())
}

/// The last lot of a provider's home feed, which is the whole feed.
async fn latest(api: &Arc<dyn MusicApi>) -> Result<HomeFeed> {
    let mut lots = api.home_paged().await?;
    let mut feed = None;
    while let Some(lot) = lots.recv().await {
        match lot {
            Ok(lot) => feed = Some(lot),
            Err(error) if feed.is_none() => return Err(error),
            Err(error) => {
                log::warn!("home: a later lot failed: {error:#}");
                break;
            }
        }
    }
    Ok(feed.unwrap_or_default())
}

/// One provider's feed as shelves: what it played lately, its quick picks, then its own
/// shelves. Empty shelves are left out.
fn lane(core: &Core, slug: &str, name: &str, feed: HomeFeed) -> Vec<HomeSection> {
    let mut sections = Vec::new();
    let mut push = |section: HomeSection| {
        let empty = section.tracks.is_empty()
            && section.albums.is_empty()
            && section.artists.is_empty()
            && section.playlists.is_empty();
        if !empty {
            sections.push(section);
        }
    };
    push(section(
        core,
        format!("{slug}:listen-again"),
        String::new(),
        name,
        SectionKind::ListenAgain,
        &feed.listen_again,
    ));
    if let Some(picks) = feed.quick_picks {
        push(HomeSection {
            id: format!("{slug}:quick-picks"),
            title: String::new(),
            provider: name.to_owned(),
            kind: SectionKind::QuickPicks,
            tracks: core.remember(&picks),
            albums: Vec::new(),
            artists: Vec::new(),
            playlists: Vec::new(),
        });
    }
    for (index, GenreSection { title, items, .. }) in feed.sections.into_iter().enumerate() {
        push(section(
            core,
            format!("{slug}:{index}"),
            title,
            name,
            SectionKind::Shelf,
            &items,
        ));
    }
    sections
}

/// A shelf of mixed cards, split by kind. Genre cards have no page on the phone and are left
/// out.
fn section(
    core: &Core,
    id: String,
    title: String,
    provider: &str,
    kind: SectionKind,
    items: &[GenreItem],
) -> HomeSection {
    let mut tracks = Vec::new();
    let mut albums = Vec::new();
    let mut artists = Vec::new();
    let mut playlists = Vec::new();
    for item in items {
        match item {
            GenreItem::Track(track) => tracks.push(track.clone()),
            GenreItem::Album(album) => albums.push(core::album(album)),
            GenreItem::Artist(artist) => artists.push(artist_item(artist)),
            GenreItem::Playlist(playlist) => playlists.push(core::playlist(playlist)),
            GenreItem::Genre(_) => {}
        }
    }
    HomeSection {
        id,
        title,
        provider: provider.to_owned(),
        kind,
        tracks: core.remember(&tracks),
        albums,
        artists,
        playlists,
    }
}

/// The lanes interleaved one entry at a time, in lane order.
fn woven<T>(lanes: Vec<Vec<T>>) -> Vec<T> {
    let rounds = lanes.iter().map(Vec::len).max().unwrap_or(0);
    let mut lanes: Vec<_> = lanes.into_iter().map(Vec::into_iter).collect();
    let mut woven = Vec::new();
    for _ in 0..rounds {
        for lane in &mut lanes {
            woven.extend(lane.next());
        }
    }
    woven
}

/// One continuation of a playlist, or nothing more when the provider cannot continue.
async fn more(core: &Core, api: &Arc<dyn MusicApi>, from: &str) -> PlaylistMore {
    match api.playlist_continuation(from).await {
        Ok((tracks, continuation)) => PlaylistMore {
            tracks: core.remember(&tracks),
            continuation,
        },
        Err(error) => {
            log::warn!("library: cannot continue a playlist: {error:#}");
            PlaylistMore::default()
        }
    }
}

/// Reads `part` from `api` a provider page at a time, each mapped to items, on the core's
/// runtime. Dropping the receiver stops the reading.
fn stream(core: Arc<Core>, api: Arc<dyn MusicApi>, part: Part) -> mpsc::Receiver<Result<Lot>> {
    let (sender, receiver) = mpsc::channel(1);
    tokio::spawn(async move {
        match part {
            Part::Songs => {
                let pages = api.saved_tracks_paged().await;
                forward(pages, &sender, |tracks| core.remember(&tracks)).await;
            }
            Part::Albums => {
                let pages = api.saved_albums_paged().await;
                forward(pages, &sender, |albums| {
                    albums.iter().map(core::album).collect()
                })
                .await;
            }
            Part::Artists => {
                let pages = api.saved_artists_paged().await;
                forward(pages, &sender, |artists| {
                    artists.iter().map(artist_item).collect()
                })
                .await;
            }
            Part::Playlists => {
                let pages = api.playlists().await.map(music::whole);
                forward(pages, &sender, |playlists| {
                    playlists.iter().map(core::playlist).collect()
                })
                .await;
            }
        }
    });
    receiver
}

/// Hands each provider page on, mapped, until the pages end or nobody listens.
async fn forward<T>(
    pages: Result<Pages<T>>,
    sender: &mpsc::Sender<Result<Lot>>,
    map: impl Fn(Vec<T>) -> Vec<Item>,
) {
    let mut pages = match pages {
        Ok(pages) => pages,
        Err(error) => {
            let _ = sender.send(Err(error)).await;
            return;
        }
    };
    while let Some(page) = pages.recv().await {
        let lot = page.map(|page| (page.total, map(page.items)));
        if sender.send(lot).await.is_err() {
            return;
        }
    }
}

fn snapshot_key(slug: &str, part: Part) -> String {
    let part = match part {
        Part::Songs => "songs",
        Part::Albums => "albums",
        Part::Artists => "artists",
        Part::Playlists => "playlists",
    };
    format!("library/{slug}/{part}")
}
