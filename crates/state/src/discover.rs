//! What the home page offers that the user has not heard yet: new releases from the artists
//! they follow on every signed-in provider, and radio mixes seeded from what they played last.
//! Both lists are kept in `storage::Cache`, so the page draws them before any provider answers.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, Entity, Task};
use music::{Album, MusicApi, Shape, Track};
use serde::{Deserialize, Serialize};
use storage::Cache;

use crate::releases::day_of;
use crate::{History, HistoryState, Io, Library, LibraryPart, Session, SessionEvent, Shelf, join};

/// How many followed artists are asked for their albums, most recently followed first.
const ARTISTS: usize = 24;
/// How many artist lookups are in flight at once, so a launch never fires two dozen at a time.
const IN_FLIGHT: usize = 4;
/// How many days old a release may be and still count as new.
const FRESH: i64 = 56;
/// How many history tracks seed a mix, each by a different artist.
const SEEDS: usize = 3;
/// A mix with fewer tracks than this left after filtering is dropped.
const MIX_MIN: usize = 8;
/// A mix is cut off at this many tracks.
const MIX_MAX: usize = 25;
/// How old, in seconds, a cached list may be before it is fetched again.
const STALE: i64 = 12 * 60 * 60;
/// What every Discover key in the cache starts with, so a full wipe drops them all at once.
const PREFIX: &str = "discover/";
/// The cache key of the new releases.
const RELEASES: &str = "discover/releases";
/// The cache key of the mixes.
const MIXES: &str = "discover/mixes";
/// How long to wait before asking for the featured album's tracks again, per try, so a
/// provider that fails for a moment does not leave the hero's play button off for good.
const RETRIES: [Duration; 3] = [
    Duration::from_secs(3),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// A radio mix: the history track it grew from and the tracks the provider answered with,
/// minus anything the user already played or favorited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mix {
    pub seed: Track,
    pub tracks: Vec<Track>,
}

/// What the hero at the top of the home page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Featured {
    Release(Box<Album>),
    /// An index into [`Discover::mixes`], so the hero and the shelf read the same mix.
    Mix(usize),
}

/// One cached list, with when it was fetched.
#[derive(Serialize, Deserialize)]
struct Stored<T> {
    saved_at: i64,
    rows: Vec<T>,
}

/// The home page's Discover lists: new releases and radio mixes across every signed-in
/// provider, read from the cache first and refetched once they are older than `STALE`.
pub struct Discover {
    session: Entity<Session>,
    library: Entity<Library>,
    history: Entity<History>,
    io: Io,
    cache: Cache,
    releases: Rc<Vec<Album>>,
    mixes: Rc<Vec<Mix>>,
    /// The featured release's tracks, for the hero's play button.
    featured_tracks: Rc<Vec<Track>>,
    /// When each list was last fetched, seconds since the epoch; 0 for never.
    releases_at: i64,
    mixes_at: i64,
    /// Whether the home page is on screen. While it is, a refetch is held in `held_*` and
    /// lands when the page is left, so nothing moves under the user's eyes.
    visible: bool,
    held_releases: Option<Vec<Album>>,
    held_mixes: Option<Vec<Mix>>,
    priming: Option<Task<()>>,
    releasing: Option<Task<()>>,
    mixing: Option<Task<()>>,
    featuring: Option<Task<()>>,
}

impl Discover {
    /// Reads both lists from the cache and follows the session, the library and the history,
    /// fetching again whenever one of them changes and a list is due.
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        history: Entity<History>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::WentOffline => {}
            SessionEvent::SignedIn(_) => this.signed_in(cx),
            SessionEvent::SignedOut(slug) => this.signed_out(slug, cx),
            SessionEvent::Reconnected(_) | SessionEvent::LocalChanged => {}
        })
        .detach();
        cx.observe(&library, |this, _, cx| this.refresh(cx))
            .detach();
        cx.observe(&history, |this, _, cx| this.refresh(cx))
            .detach();

        let mut discover = Self {
            session,
            library,
            history,
            io,
            cache: Cache::standard(),
            releases: Rc::default(),
            mixes: Rc::default(),
            featured_tracks: Rc::default(),
            releases_at: 0,
            mixes_at: 0,
            visible: false,
            held_releases: None,
            held_mixes: None,
            priming: None,
            releasing: None,
            mixing: None,
            featuring: None,
        };
        discover.prime(cx);
        discover
    }

    /// The new releases, newest first.
    pub fn releases(&self) -> Rc<Vec<Album>> {
        self.releases.clone()
    }

    /// The radio mixes, one per seed.
    pub fn mixes(&self) -> Rc<Vec<Mix>> {
        self.mixes.clone()
    }

    /// The featured release's tracks, empty until they have loaded or when a mix is featured.
    pub fn featured_tracks(&self) -> Rc<Vec<Track>> {
        self.featured_tracks.clone()
    }

    /// The first new release, else the first mix, else nothing.
    pub fn featured(&self) -> Option<Featured> {
        if let Some(album) = self.releases.first() {
            return Some(Featured::Release(Box::new(album.clone())));
        }
        (!self.mixes.is_empty()).then_some(Featured::Mix(0))
    }

    /// Tells Discover whether the home page is on screen. Leaving it lands what was held.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            return;
        }
        let mut changed = false;
        if let Some(releases) = self.held_releases.take() {
            self.releases = Rc::new(releases);
            self.feature(cx);
            changed = true;
        }
        if let Some(mixes) = self.held_mixes.take() {
            self.mixes = Rc::new(mixes);
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Reads both lists from the cache, then asks for whatever is missing or stale.
    fn prime(&mut self, cx: &mut Context<Self>) {
        let cache = self.cache.clone();
        let io = self.io.clone();
        self.priming = Some(cx.spawn(async move |this, cx| {
            let read = io
                .spawn_blocking(move || {
                    (
                        stored::<Album>(&cache, RELEASES),
                        stored::<Mix>(&cache, MIXES),
                    )
                })
                .await;
            let (releases, mixes) = read.unwrap_or_else(|error| {
                log::warn!("discover: cannot read the cache: {error:#}");
                (None, None)
            });
            this.update(cx, |this, cx| {
                if let Some(releases) = releases {
                    this.releases_at = releases.saved_at;
                    this.releases = Rc::new(releases.rows);
                    this.feature(cx);
                }
                if let Some(mixes) = mixes {
                    this.mixes_at = mixes.saved_at;
                    this.mixes = Rc::new(mixes.rows);
                }
                this.priming = None;
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    /// A provider joined, at startup or later: fetch both lists again with it included, keeping
    /// what is shown and cached until the answer lands.
    fn signed_in(&mut self, cx: &mut Context<Self>) {
        self.releasing = None;
        self.mixing = None;
        self.releases_at = 0;
        self.mixes_at = 0;
        // Cached releases land before any provider has restored, when there is no client to
        // ask for the featured album's tracks; this is the first moment there is.
        self.feature(cx);
        self.refresh(cx);
    }

    /// A provider left: drop what it contributed, from the screen and the cache, and fetch
    /// again from the providers still signed in. With none left, everything goes.
    fn signed_out(&mut self, slug: &str, cx: &mut Context<Self>) {
        if self.session.read(cx).active_slugs().is_empty() {
            return self.reset(cx);
        }
        self.releasing = None;
        self.mixing = None;
        let owned = |id: &str| music::tag::slug_of(id) == Some(slug);
        let releases: Vec<Album> = self
            .releases
            .iter()
            .filter(|album| !owned(&album.id))
            .cloned()
            .collect();
        let kept = |mix: &Mix| !mix.seed.id.as_deref().is_some_and(owned);
        let mixes: Vec<Mix> = self.mixes.iter().filter(|mix| kept(mix)).cloned().collect();
        if let Some(held) = self.held_releases.as_mut() {
            held.retain(|album| !owned(&album.id));
        }
        if let Some(held) = self.held_mixes.as_mut() {
            held.retain(|mix| kept(mix));
        }
        let cache = self.cache.clone();
        let (stored_releases, stored_mixes) = (releases.clone(), mixes.clone());
        self.io.spawn_blocking(move || {
            if stored_releases.is_empty() && stored_mixes.is_empty() {
                if let Err(error) = cache.forget(PREFIX) {
                    log::warn!("discover: cannot drop the cache: {error:#}");
                }
                return;
            }
            store(&cache, RELEASES, &stored_releases);
            store(&cache, MIXES, &stored_mixes);
        });
        self.releases = Rc::new(releases);
        self.mixes = Rc::new(mixes);
        self.releases_at = 0;
        self.mixes_at = 0;
        self.feature(cx);
        self.refresh(cx);
        cx.notify();
    }

    /// Drops everything a signed-in account produced and starts over.
    fn reset(&mut self, cx: &mut Context<Self>) {
        self.priming = None;
        self.releasing = None;
        self.mixing = None;
        self.featuring = None;
        self.releases = Rc::default();
        self.mixes = Rc::default();
        self.featured_tracks = Rc::default();
        self.held_releases = None;
        self.held_mixes = None;
        self.releases_at = 0;
        self.mixes_at = 0;
        let cache = self.cache.clone();
        self.io.spawn_blocking(move || {
            if let Err(error) = cache.forget(PREFIX) {
                log::warn!("discover: cannot drop the cache: {error:#}");
            }
        });
        self.refresh(cx);
        cx.notify();
    }

    /// Starts whatever fetch is due: never while the cache is still being read, never twice.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.priming.is_some() {
            return;
        }
        let now = now();
        if self.releasing.is_none() && now - self.releases_at > STALE {
            self.fetch_releases(cx);
        }
        if self.mixing.is_none() && now - self.mixes_at > STALE {
            self.fetch_mixes(cx);
        }
    }

    /// Every followed artist of every signed-in provider, once all their artist lists have
    /// arrived, most recently followed first, with the client that owns each.
    fn followed(&self, cx: &Context<Self>) -> Option<Vec<(String, Arc<dyn MusicApi>)>> {
        let session = self.session.read(cx);
        let library = self.library.read(cx);
        let slugs = session.active_slugs();
        if slugs.is_empty()
            || slugs
                .iter()
                .any(|slug| library.loading(Shelf::Of(slug), LibraryPart::Artists))
        {
            return None;
        }
        let mut artists: Vec<_> = slugs
            .iter()
            .filter(|slug| !matches!(library.shape(Shelf::Of(slug)), Shape::Catalog(_)))
            .filter_map(|slug| Some((session.client_for_slug(slug)?, *slug)))
            .flat_map(|(client, slug)| {
                library
                    .state(Shelf::Of(slug))
                    .artists()
                    .iter()
                    .map(move |artist| (artist.added_at, artist.id.clone(), client.clone()))
            })
            .collect();
        artists.sort_by_key(|(added_at, ..)| std::cmp::Reverse(added_at.unwrap_or(i64::MIN)));
        Some(
            artists
                .into_iter()
                .take(ARTISTS)
                .map(|(_, id, client)| (id, client))
                .collect(),
        )
    }

    /// Asks every followed artist, a few at a time, for their albums and keeps the new ones.
    /// Only once every provider's artists have arrived.
    fn fetch_releases(&mut self, cx: &mut Context<Self>) {
        let Some(artists) = self.followed(cx) else {
            return;
        };
        let io = self.io.clone();
        self.releasing = Some(cx.spawn(async move |this, cx| {
            let today = now() / 86_400;
            let found = join(io.spawn(async move {
                let mut albums: Vec<Album> = Vec::new();
                let mut answered = artists.is_empty();
                for chunk in artists.chunks(IN_FLIGHT) {
                    let lookups = chunk
                        .iter()
                        .map(|(id, client)| albums_of(id.clone(), client.clone()));
                    for found in futures::future::join_all(lookups)
                        .await
                        .into_iter()
                        .flatten()
                    {
                        answered = true;
                        albums.extend(found);
                    }
                }
                anyhow::Ok(answered.then(|| fresh(albums, today)))
            }))
            .await;
            this.update(cx, |this, cx| {
                this.releasing = None;
                match found {
                    Ok(Some(releases)) => {
                        this.releases_at = now();
                        this.land_releases(releases, cx);
                    }
                    Ok(None) => log::warn!("discover: no artist answered for new releases"),
                    Err(error) => log::warn!("discover: cannot load new releases: {error:#}"),
                }
            })
            .ok();
        }));
    }

    /// Asks each seed's provider for its radio. Only once the history has arrived and holds a
    /// seed whose provider has a radio; when every radio fails, the mixes shown stay.
    fn fetch_mixes(&mut self, cx: &mut Context<Self>) {
        let history = self.history.read(cx);
        if *history.state() != HistoryState::Ready {
            return;
        }
        let session = self.session.read(cx);
        let seeds = seeds(history.tracks(), |id| {
            session.capabilities_of(Shelf::of(id)).radio
        });
        let asked: Vec<(Track, Arc<dyn MusicApi>)> = seeds
            .into_iter()
            .filter_map(|seed| {
                let client = session.client_of(Shelf::of(seed.id.as_deref()?))?;
                Some((seed, client))
            })
            .collect();
        if asked.is_empty() {
            return;
        }
        let io = self.io.clone();
        self.mixing = Some(cx.spawn(async move |this, cx| {
            let answered = join(io.spawn(async move {
                let mut answered = Vec::new();
                let mut any = false;
                for (seed, client) in asked {
                    let id = seed.id.clone().unwrap_or_default();
                    match client.track_radio(&id, None).await {
                        Ok((radio, _)) => {
                            any = true;
                            answered.push((seed, radio));
                        }
                        Err(error) => log::warn!("discover: cannot load radio {id}: {error:#}"),
                    }
                }
                anyhow::Ok(any.then_some(answered))
            }))
            .await;
            this.update(cx, |this, cx| {
                this.mixing = None;
                match answered {
                    Ok(Some(answered)) => {
                        this.mixes_at = now();
                        this.land_mixes(answered, cx);
                    }
                    Ok(None) => log::warn!("discover: no radio answered for mixes"),
                    Err(error) => log::warn!("discover: cannot load mixes: {error:#}"),
                }
            })
            .ok();
        }));
    }

    /// Filters each radio against the history and the favorites, then stores and shows the
    /// mixes that still have enough left.
    fn land_mixes(&mut self, answered: Vec<(Track, Vec<Track>)>, cx: &mut Context<Self>) {
        let played: HashSet<String> = self
            .history
            .read(cx)
            .tracks()
            .iter()
            .filter_map(|track| track.id.clone())
            .collect();
        let library = self.library.read(cx);
        let mixes: Vec<Mix> = answered
            .into_iter()
            .filter_map(|(seed, radio)| {
                trimmed(&seed, radio, |id| played.contains(id) || library.saved(id))
            })
            .collect();
        let cache = self.cache.clone();
        let stored = mixes.clone();
        self.io
            .spawn_blocking(move || store(&cache, MIXES, &stored));
        match self.visible && !self.mixes.is_empty() {
            true => self.held_mixes = Some(mixes),
            false => self.mixes = Rc::new(mixes),
        }
        cx.notify();
    }

    /// Files the fetched releases in the cache, then shows them, or holds them while the page is
    /// on screen. The write happens here, inside the task's own update, so a fetch dropped by a
    /// sign-out never writes an old account's rows.
    fn land_releases(&mut self, releases: Vec<Album>, cx: &mut Context<Self>) {
        let cache = self.cache.clone();
        let rows = releases.clone();
        self.io
            .spawn_blocking(move || store(&cache, RELEASES, &rows));
        match self.visible && !self.releases.is_empty() {
            true => self.held_releases = Some(releases),
            false => {
                self.releases = Rc::new(releases);
                self.feature(cx);
            }
        }
        cx.notify();
    }

    /// Loads the featured release's tracks once, so the hero can play it, trying again after
    /// each `RETRIES` pause when the provider fails.
    fn feature(&mut self, cx: &mut Context<Self>) {
        let Some(album) = self.releases.first().cloned() else {
            self.featured_tracks = Rc::default();
            self.featuring = None;
            return;
        };
        let loaded = self
            .featured_tracks
            .first()
            .and_then(|track| track.album_id.as_deref());
        if loaded == Some(album.id.as_str()) {
            return;
        }
        self.featured_tracks = Rc::default();
        let Some(client) = self.session.read(cx).client_of(Shelf::of(&album.id)) else {
            return;
        };
        let io = self.io.clone();
        self.featuring = Some(cx.spawn(async move |this, cx| {
            let mut pauses = RETRIES.iter();
            loop {
                let id = album.id.clone();
                let asked = client.clone();
                match join(io.spawn(async move { asked.album_tracks(&id).await })).await {
                    Ok(tracks) => {
                        this.update(cx, |this, cx| {
                            this.featured_tracks = Rc::new(tracks);
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                    Err(error) => {
                        log::warn!("discover: cannot load the featured album: {error:#}");
                        let Some(&pause) = pauses.next() else {
                            return;
                        };
                        cx.background_executor().timer(pause).await;
                    }
                }
            }
        }));
    }
}

/// One artist's albums, or none when the provider cannot answer for them.
async fn albums_of(id: String, client: Arc<dyn MusicApi>) -> Option<Vec<Album>> {
    client
        .artist(&id)
        .await
        .inspect_err(|error| log::warn!("discover: cannot load artist {id}: {error:#}"))
        .ok()
        .map(|artist| artist.albums)
}

/// The newest playable tracks of the history whose id `usable` accepts, at most `SEEDS`, no
/// two by the same artist.
fn seeds(history: &[Track], usable: impl Fn(&str) -> bool) -> Vec<Track> {
    let mut artists = HashSet::new();
    history
        .iter()
        .filter(|track| track.playable && track.id.as_deref().is_some_and(&usable))
        .filter(|track| artists.insert(artist_of(track).to_lowercase()))
        .take(SEEDS)
        .cloned()
        .collect()
}

/// The first credited artist of a track, else its artist line.
fn artist_of(track: &Track) -> &str {
    track
        .artist_refs
        .first()
        .map_or(track.artists.as_str(), |artist| artist.name.as_str())
}

/// A seed's radio without the seed, without anything `known`, without repeats, capped at
/// `MIX_MAX`; none when fewer than `MIX_MIN` are left.
fn trimmed(seed: &Track, radio: Vec<Track>, known: impl Fn(&str) -> bool) -> Option<Mix> {
    let mut seen = HashSet::new();
    let tracks: Vec<Track> = radio
        .into_iter()
        .filter(|track| track.playable)
        .filter(|track| {
            track.id.as_deref().is_some_and(|id| {
                Some(id) != seed.id.as_deref() && !known(id) && seen.insert(id.to_owned())
            })
        })
        .take(MIX_MAX)
        .collect();
    (tracks.len() >= MIX_MIN).then(|| Mix {
        seed: seed.clone(),
        tracks,
    })
}

/// Seconds since the epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The albums released within `FRESH` days of `today` (days since the epoch), newest first,
/// one copy of a release that several providers list, the first provider's.
fn fresh(albums: impl IntoIterator<Item = Album>, today: i64) -> Vec<Album> {
    let mut seen = HashSet::new();
    let mut dated: Vec<(i64, Album)> = albums
        .into_iter()
        .filter_map(|album| {
            let day = day_of(&album.release_date)?;
            (day <= today && today - day <= FRESH).then_some((day, album))
        })
        .filter(|(_, album)| seen.insert(release_key(album)))
        .collect();
    dated.sort_by_key(|(day, _)| std::cmp::Reverse(*day));
    dated.into_iter().map(|(_, album)| album).collect()
}

/// What makes two providers' copies of one release the same: its name and its first artist.
fn release_key(album: &Album) -> (String, String) {
    let artist = album
        .artist_refs
        .first()
        .map_or(album.artists.as_str(), |artist| artist.name.as_str());
    (album.name.to_lowercase(), artist.to_lowercase())
}

/// A cached list, or none when it is missing or unreadable.
fn stored<T: for<'de> Deserialize<'de>>(cache: &Cache, key: &str) -> Option<Stored<T>> {
    let text = cache
        .read(key, str::to_owned)
        .inspect_err(|error| log::warn!("discover: cannot read {key}: {error:#}"))
        .ok()??;
    serde_json::from_str(&text)
        .inspect_err(|error| log::warn!("discover: cannot parse {key}: {error:#}"))
        .ok()
}

/// Writes a list to the cache, stamped now. A failure only costs the next launch a fetch.
fn store<T: Serialize>(cache: &Cache, key: &str, rows: &[T]) {
    let saved_at = now();
    let value = serde_json::json!({ "saved_at": saved_at, "rows": rows }).to_string();
    if let Err(error) = cache.write(key, &value, saved_at) {
        log::warn!("discover: cannot write {key}: {error:#}");
    }
}
