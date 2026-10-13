# Home with Discover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the home page into layout B: a featured hero, a Jump back in deck, New releases
from followed artists, Quick picks, mixes seeded from history, then the provider shelves, for
every signed-in provider at once.

**Architecture:** A new GPUI entity `state::Discover` owns new releases, mixes, the featured
item and their `storage::Cache` snapshot. `state::Home` gains one accessor, `resume`. `HomeView`
reads both and lays out the page from existing pieces (`PageHero`, `Picks::mixed`, `Shelves`).

**Tech Stack:** Rust 2024, GPUI, tokio via `state::Io`, `storage::Cache` (SQLite), Fluent.

**Spec:** `docs/superpowers/specs/2026-10-09-home-discover-design.md`

## Global Constraints

- No Spotify Web API; data comes only through `MusicApi` (`artist`, `track_radio`, `album_tracks`).
- Network work only inside `io.spawn`; tasks stored in fields, never `.detach()`ed.
- No hardcoded colour, radius or size; read `cx.theme()`. Module-top `const` for local details.
- Every user-facing string through `t!`; never `t!` in a constructor.
- No new tests (CLAUDE.md "Do not add tests unless asked").
- Doc comments on every new type and non-obvious function.
- Commits: Conventional Commits, no body, **no** `Co-Authored-By`. **Every `git commit` must be
  shown to and approved by the user first** (global rule); subagents get the same restriction.
- Constants: `ARTISTS = 24`, `IN_FLIGHT = 4`, `FRESH = 56` days, `SEEDS = 3`, `MIX_MIN = 8`,
  `MIX_MAX = 25`, `STALE = 12h`, `RESUME = 8`.

## Review Focus

1. A followed artist with no `release_date` precision (year only, empty string) — must never
   count as new and never panic while parsing.
2. Signing out of one provider while releases are in flight — the task is dropped, the lists
   and `discover/` cache keys are cleared, nothing from the old account lands afterwards.
3. Home page open while a refetch lands — hero, releases and mixes must not change in view;
   they land when the page is left (`set_visible(false)`).
4. A provider whose `track_radio` errors (Local, Subsonic without radio) — that seed is skipped,
   other mixes still appear.
5. No artists followed and an empty history — no hero, no empty headings, the page starts at
   Quick picks exactly as today.

(No test code is added for these per the house rule; each is checked by hand in Task 6.)

---

## File Structure

| File | Responsibility |
|---|---|
| Create `crates/state/src/discover.rs` | `Discover` entity, `Mix`, `Featured`, cache, pure helpers |
| Modify `crates/state/src/lib.rs` | `mod discover;` + `pub use discover::{Discover, Featured, Mix};` |
| Modify `crates/state/src/home.rs` | take `Entity<History>`, add `resume` |
| Create `crates/views/src/screens/home/featured.rs` | the hero element |
| Modify `crates/views/src/screens/home/mod.rs` | layout B, tint sampling, second deck and shelves |
| Modify `crates/views/src/root.rs:142-143` | construct `Discover`, pass it and `History` on |
| Modify `assets/i18n/{en-US,ru,uk,pl}/main.ftl` | new `home-*` keys |
| Modify `README.md` (via script), `CHANGELOG.md` | coverage table, `Added` entry |

---

### Task 1: `state::Discover` with new releases

**Files:**
- Create: `crates/state/src/discover.rs`
- Modify: `crates/state/src/lib.rs` (module list near line 11, re-exports near line 45)

**Interfaces:**
- Consumes: `Library::state(Shelf) -> &LibraryState`, `LibraryState::artists() -> &[SavedArtist]`,
  `Library::loading(Shelf, LibraryPart) -> bool`, `Session::active_slugs() -> Vec<&'static str>`,
  `Session::client_for_slug(&str)`, `Session::client_of(Shelf)`, `SessionEvent`,
  `History::{state, tracks}`, `storage::Cache::{standard, read, write, forget}`, `join`, `Io`.
- Produces:
  - `pub struct Discover` with `new(session: Entity<Session>, library: Entity<Library>, history: Entity<History>, io: Io, cx: &mut Context<Self>) -> Self`
  - `pub fn releases(&self) -> Rc<Vec<Album>>`
  - `pub fn mixes(&self) -> Rc<Vec<Mix>>` (empty until Task 2)
  - `pub fn featured(&self) -> Option<Featured>`
  - `pub fn featured_tracks(&self) -> Rc<Vec<Track>>`
  - `pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>)`
  - `pub struct Mix { pub seed: Track, pub tracks: Vec<Track> }`
  - `pub enum Featured { Release(Album), Mix(usize) }`

- [ ] **Step 1: Create `crates/state/src/discover.rs`**

```rust
//! What the home page offers that the user has not heard yet: new releases from the artists
//! they follow on every signed-in provider, and radio mixes seeded from what they played last.
//! Both lists are kept in `storage::Cache`, so the page draws them before any provider answers.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use gpui::{Context, Entity, Task};
use music::{Album, MusicApi, Track};
use serde::{Deserialize, Serialize};
use storage::Cache;

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
const MIX_MAX: usize = 25;
/// How old, in seconds, a cached list may be before it is fetched again.
const STALE: i64 = 12 * 60 * 60;
const PREFIX: &str = "discover/";
const RELEASES: &str = "discover/releases";
const MIXES: &str = "discover/mixes";

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
    Release(Album),
    /// An index into [`Discover::mixes`], so the hero and the shelf read the same mix.
    Mix(usize),
}

/// One cached list, with when it was fetched.
#[derive(Serialize, Deserialize)]
struct Stored<T> {
    saved_at: i64,
    rows: Vec<T>,
}

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
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        history: Entity<History>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedIn(_) | SessionEvent::SignedOut(_) => this.reset(cx),
            SessionEvent::Reconnected(_) | SessionEvent::LocalChanged => {}
        })
        .detach();
        cx.observe(&library, |this, _, cx| this.refresh(cx)).detach();
        cx.observe(&history, |this, _, cx| this.refresh(cx)).detach();

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

    pub fn releases(&self) -> Rc<Vec<Album>> {
        self.releases.clone()
    }

    pub fn mixes(&self) -> Rc<Vec<Mix>> {
        self.mixes.clone()
    }

    pub fn featured_tracks(&self) -> Rc<Vec<Track>> {
        self.featured_tracks.clone()
    }

    /// The first new release, else the first mix, else nothing.
    pub fn featured(&self) -> Option<Featured> {
        if let Some(album) = self.releases.first() {
            return Some(Featured::Release(album.clone()));
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
                .spawn_blocking(move || (stored::<Album>(&cache, RELEASES), stored::<Mix>(&cache, MIXES)))
                .await;
            let Ok((releases, mixes)) = read else {
                return;
            };
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

    /// Drops everything a signed-in account produced and starts over.
    fn reset(&mut self, cx: &mut Context<Self>) {
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

    fn fetch_releases(&mut self, cx: &mut Context<Self>) {
        let Some(artists) = self.followed(cx) else {
            return;
        };
        let io = self.io.clone();
        let cache = self.cache.clone();
        self.releasing = Some(cx.spawn(async move |this, cx| {
            let today = now() / 86_400;
            let found = join(io.spawn(async move {
                let albums: Vec<Vec<Album>> = futures::stream::iter(artists)
                    .map(|(id, client)| async move {
                        match client.artist(&id).await {
                            Ok(artist) => artist.albums,
                            Err(error) => {
                                log::warn!("discover: cannot load artist {id}: {error:#}");
                                Vec::new()
                            }
                        }
                    })
                    .buffered(IN_FLIGHT)
                    .collect()
                    .await;
                let releases = fresh(albums.into_iter().flatten(), today);
                store(&cache, RELEASES, &releases);
                anyhow::Ok(releases)
            }))
            .await;
            this.update(cx, |this, cx| {
                this.releases_at = now();
                match found {
                    Ok(releases) => this.land_releases(releases, cx),
                    Err(error) => log::warn!("discover: cannot load new releases: {error:#}"),
                }
            })
            .ok();
        }));
    }

    /// Mixes arrive in Task 2; until then nothing is fetched.
    fn fetch_mixes(&mut self, _cx: &mut Context<Self>) {}

    fn land_releases(&mut self, releases: Vec<Album>, cx: &mut Context<Self>) {
        match self.visible && !self.releases.is_empty() {
            true => self.held_releases = Some(releases),
            false => {
                self.releases = Rc::new(releases);
                self.feature(cx);
            }
        }
        cx.notify();
    }

    /// Loads the featured release's tracks once, so the hero can play it.
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
            let id = album.id.clone();
            match join(io.spawn(async move { client.album_tracks(&id).await })).await {
                Ok(tracks) => {
                    this.update(cx, |this, cx| {
                        this.featured_tracks = Rc::new(tracks);
                        cx.notify();
                    })
                    .ok();
                }
                Err(error) => log::warn!("discover: cannot load the featured album: {error:#}"),
            }
        }));
    }
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

/// Days since 1970-01-01 for a `YYYY-MM-DD` date. A year or a year and month alone is not a
/// day, so it answers none.
fn day_of(date: &str) -> Option<i64> {
    let mut parts = date.splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts
        .next()?
        .split(|letter: char| !letter.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let year = match month <= 2 {
        true => year - 1,
        false => year,
    };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    Some(era * 146_097 + of_cycle - 719_468)
}

/// A cached list, or none when it is missing or unreadable.
fn stored<T: for<'de> Deserialize<'de>>(cache: &Cache, key: &str) -> Option<Stored<T>> {
    let text = cache
        .read(key)
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
```

- [ ] **Step 2: Register the module**

In `crates/state/src/lib.rs`, add `mod discover;` between `mod detail;` and `mod discord;`, and
`pub use discover::{Discover, Featured, Mix};` between `pub use detail::…;` and `pub use drm::…;`.

- [ ] **Step 3: Build**

Run: `cargo check -p state`
Expected: compiles. If `HistoryState` is reported unused, leave it: Task 2 uses it. If clippy
later flags it, Task 2 resolves it.

- [ ] **Step 4: Commit (ask the user first)**

```bash
git add crates/state/src/discover.rs crates/state/src/lib.rs
git commit -m "feat(state): find new releases from followed artists"
```

---

### Task 2: Mixes from history

**Files:**
- Modify: `crates/state/src/discover.rs`

**Interfaces:**
- Consumes: `History::state() -> &HistoryState`, `History::tracks() -> &[Track]`,
  `Library::saved(&str) -> bool`, `MusicApi::track_radio(&str) -> Result<Vec<Track>>`.
- Produces: `Discover::mixes()` now filled; `Featured::Mix(0)` when there is no release.

- [ ] **Step 1: Replace the placeholder `fetch_mixes` with the real one and add `land_mixes`**

```rust
    /// Asks each seed's provider for its radio. Only once the history has arrived and is not
    /// empty; a seed whose provider has no radio is skipped.
    fn fetch_mixes(&mut self, cx: &mut Context<Self>) {
        let history = self.history.read(cx);
        if *history.state() != HistoryState::Ready {
            return;
        }
        let seeds = seeds(history.tracks());
        if seeds.is_empty() {
            return;
        }
        let session = self.session.read(cx);
        let asked: Vec<(Track, Arc<dyn MusicApi>)> = seeds
            .into_iter()
            .filter_map(|seed| {
                let client = session.client_of(Shelf::of(seed.id.as_deref()?))?;
                Some((seed, client))
            })
            .collect();
        let io = self.io.clone();
        self.mixing = Some(cx.spawn(async move |this, cx| {
            let answered = join(io.spawn(async move {
                let mut answered = Vec::new();
                for (seed, client) in asked {
                    let id = seed.id.clone().unwrap_or_default();
                    match client.track_radio(&id).await {
                        Ok(radio) => answered.push((seed, radio)),
                        Err(error) => log::warn!("discover: cannot load radio {id}: {error:#}"),
                    }
                }
                anyhow::Ok(answered)
            }))
            .await;
            this.update(cx, |this, cx| {
                this.mixes_at = now();
                match answered {
                    Ok(answered) => this.land_mixes(answered, cx),
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
```

- [ ] **Step 2: Add the two pure helpers at the bottom, above `now`**

```rust
/// The newest playable tracks of the history, at most `SEEDS`, no two by the same artist.
fn seeds(history: &[Track]) -> Vec<Track> {
    let mut artists = HashSet::new();
    history
        .iter()
        .filter(|track| track.playable && track.id.is_some())
        .filter(|track| artists.insert(artist_of(track).to_lowercase()))
        .take(SEEDS)
        .cloned()
        .collect()
}

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
```

- [ ] **Step 3: Build and lint**

Run: `cargo clippy -p state --all-targets`
Expected: no warnings.

- [ ] **Step 4: Commit (ask the user first)**

```bash
git add crates/state/src/discover.rs
git commit -m "feat(state): mix radio from the listening history"
```

---

### Task 3: `Home::resume`

**Files:**
- Modify: `crates/state/src/home.rs` (struct, `new`, a new accessor, a private helper)
- Modify: `crates/views/src/root.rs:142` (pass `history`)

**Interfaces:**
- Consumes: `History::tracks()`.
- Produces: `Home::new(library, session, history: Entity<History>, io, cx)` and
  `pub fn resume(&self, cx: &App) -> Rc<Vec<GenreItem>>`.

- [ ] **Step 1: Add the field, constant and constructor argument**

In `home.rs`: `use crate::{History, Io, Library, …};`, a constant under `PICKS_LIMIT`:

```rust
/// How many items Jump back in shows.
const RESUME: usize = 8;
```

a field `history: Entity<History>,` after `session`, the constructor parameter
`history: Entity<History>,` after `session`, an observer next to the library one:

```rust
        cx.observe(&history, |_, _, cx| cx.notify()).detach();
```

and `history,` in the `Self { … }` literal.

- [ ] **Step 2: Add the accessor after `quick_picks()`**

```rust
    /// What Jump back in shows: every provider's recently played, woven, then the listening
    /// history, one copy of each, `RESUME` at most.
    pub fn resume(&self, cx: &App) -> Rc<Vec<GenreItem>> {
        let mut seen = HashSet::new();
        let history = self.history.read(cx).tracks().iter().cloned();
        Rc::new(
            self.recent
                .iter()
                .cloned()
                .chain(history.map(GenreItem::Track))
                .filter(|item| seen.insert(item_key(item)))
                .take(RESUME)
                .collect(),
        )
    }
```

and the helper beside `blank`:

```rust
/// What tells two shelf items apart, whatever their kind.
fn item_key(item: &GenreItem) -> String {
    match item {
        GenreItem::Playlist(playlist) => format!("playlist:{}", playlist.id),
        GenreItem::Album(album) => format!("album:{}", album.id),
        GenreItem::Genre(genre) => format!("genre:{}", genre.id),
        GenreItem::Artist(artist) => format!("artist:{}", artist.id),
        GenreItem::Track(track) => format!("track:{}", track.id.as_deref().unwrap_or(&track.name)),
    }
}
```

- [ ] **Step 3: Pass the history from `Root`**

In `crates/views/src/root.rs`, move `let history = Sonora::global(cx).history.clone();` above the
`home_state` line, rename it `history_state`, and use it in both places:

```rust
        let history_state = Sonora::global(cx).history.clone();
        let home_state = cx.new(|cx| {
            Home::new(library.clone(), session.clone(), history_state.clone(), io.clone(), cx)
        });
        let home = cx.new(|cx| HomeView::new(home_state, playback.clone(), cx));
        let history = cx.new(|cx| HistoryView::new(history_state, playback.clone(), window, cx));
```

- [ ] **Step 4: Build**

Run: `cargo check --workspace`
Expected: compiles.

- [ ] **Step 5: Commit (ask the user first)**

```bash
git add crates/state/src/home.rs crates/views/src/root.rs
git commit -m "feat(state): list what to jump back into on home"
```

---

### Task 4: Strings

**Files:**
- Modify: `assets/i18n/en-US/main.ftl`, `assets/i18n/ru/main.ftl`, `assets/i18n/uk/main.ftl`,
  `assets/i18n/pl/main.ftl` (each right after its `home-quick-picks-empty` line)

- [ ] **Step 1: en-US**

```ftl
home-featured-release = New release · { $provider }
home-featured-mix = Mix
home-featured-play = Play
home-featured-open = Open album
home-resume = Jump back in
home-new-releases = New from your artists
home-because = Because you listened to { $artist }
```

- [ ] **Step 2: ru**

```ftl
home-featured-release = Новый релиз · { $provider }
home-featured-mix = Микс
home-featured-play = Слушать
home-featured-open = Открыть альбом
home-resume = Продолжить
home-new-releases = Новое от ваших исполнителей
home-because = Потому что вы слушали { $artist }
```

- [ ] **Step 3: uk**

```ftl
home-featured-release = Новий реліз · { $provider }
home-featured-mix = Мікс
home-featured-play = Слухати
home-featured-open = Відкрити альбом
home-resume = Продовжити
home-new-releases = Нове від ваших виконавців
home-because = Бо ви слухали { $artist }
```

- [ ] **Step 4: pl**

```ftl
home-featured-release = Nowe wydanie · { $provider }
home-featured-mix = Miks
home-featured-play = Odtwórz
home-featured-open = Otwórz album
home-resume = Wróć do słuchania
home-new-releases = Nowości od twoich wykonawców
home-because = Bo słuchałeś { $artist }
```

- [ ] **Step 5: Coverage and tests**

Run: `python3 scripts/i18n-coverage.py && cargo test -p i18n`
Expected: README table updated, i18n tests pass.

- [ ] **Step 6: Commit (ask the user first)**

```bash
git add assets/i18n README.md
git commit -m "feat(views): add strings for the discover home"
```

---

### Task 5: Layout B in `HomeView`

**Files:**
- Create: `crates/views/src/screens/home/featured.rs`
- Modify: `crates/views/src/screens/home/mod.rs`
- Modify: `crates/views/src/root.rs` (construct `Discover`)

**Interfaces:**
- Consumes: everything `Discover` produces (Task 1-2), `Home::resume` (Task 3), the keys of
  Task 4, `ui::tint`, `PageHero`, `HeroMetaStrip`, `HeroPlayButton`, `release_date_label`,
  `Picks::mixed`, `Shelves::render`, `Origin::{album, radio}`, `router::navigate`.
- Produces: `featured::hero(featured, discover, tint, playback, cx) -> AnyElement`;
  `HomeView::new(home, discover: Entity<Discover>, playback, cx)`.

- [ ] **Step 1: Create `featured.rs`**

```rust
//! The hero at the top of the home page: a new release from an artist the user follows, or
//! else a mix, over a wash of its cover's hue.

use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, Hsla, div, linear_color_stop, linear_gradient};
use i18n::t;
use router::{Destination, navigate};
use state::{Discover, Featured, Origin, Playback, Sonora};
use ui::{ActiveTheme as _, Button};

use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero, release_date_label};

/// How strongly the cover's hue shows at the top of the wash.
const WASH: f32 = 0.35;

/// The cover the hero shows, so the view can sample its hue before drawing it.
pub(crate) fn cover(featured: &Featured, discover: &Discover) -> Option<String> {
    match featured {
        Featured::Release(album) => album.cover_large.clone().or_else(|| album.cover.clone()),
        Featured::Mix(index) => discover.mixes().get(*index)?.seed.cover.clone(),
    }
}

pub(crate) fn hero(
    featured: &Featured,
    discover: &Discover,
    tint: Option<Hsla>,
    playback: &Entity<Playback>,
    cx: &App,
) -> Option<AnyElement> {
    let theme = *cx.theme();
    let top = tint.map_or(theme.secondary, |tint| tint.opacity(WASH));
    let page = match featured {
        Featured::Release(album) => {
            let session = Sonora::global(cx).session.read(cx);
            let provider = music::tag::slug_of(&album.id)
                .and_then(|slug| session.provider_name_for(slug))
                .unwrap_or_default();
            let opened = album.id.clone();
            PageHero::new("home-featured", album.name.clone())
                .cover(cover(featured, discover))
                .eyebrow(t!("home-featured-release", provider = provider))
                .meta(
                    HeroMetaStrip::new()
                        .text(album.artists.clone())
                        .text(release_date_label(&album.release_date)),
                )
                .actions(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            HeroPlayButton::new(
                                "home-featured-play",
                                t!("home-featured-play"),
                                discover.featured_tracks().as_ref().clone(),
                                playback.clone(),
                            )
                            .from(Some(Origin::album(album.id.clone()))),
                        )
                        .child(
                            Button::new("home-featured-open")
                                .outline()
                                .label(t!("home-featured-open"))
                                .on_click(move |_, _, cx| {
                                    navigate(Destination::Album(opened.clone()), cx)
                                }),
                        ),
                )
        }
        Featured::Mix(index) => {
            let mixes = discover.mixes();
            let mix = mixes.get(*index)?;
            let seed = mix.seed.id.clone().unwrap_or_default();
            PageHero::new(
                "home-featured",
                t!("home-because", artist = mix.seed.artists.as_str()),
            )
            .cover(cover(featured, discover))
            .eyebrow(t!("home-featured-mix"))
            .actions(
                HeroPlayButton::new(
                    "home-featured-play",
                    t!("home-featured-play"),
                    mix.tracks.clone(),
                    playback.clone(),
                )
                .from(Some(Origin::radio(seed))),
            )
        }
    };

    Some(
        div()
            .w_full()
            .p(theme.metrics.inset)
            .rounded(theme.radius * 2.)
            .bg(linear_gradient(
                180.,
                linear_color_stop(top, 0.),
                linear_color_stop(theme.background.opacity(0.), 1.),
            ))
            .child(page)
            .into_any_element(),
    )
}
```

Check while implementing: if `Button::on_click`'s closure signature differs, copy it from
`crates/views/src/shared/hero.rs` `shuffle_button`. If `music` is not a dependency name usable
from `views` (it is: `views` already imports `music::GenreItem`), keep `music::tag::slug_of`.

- [ ] **Step 2: Wire `Discover` into `HomeView`**

In `mod.rs`: `mod featured;`, imports `use gpui::{Hsla, SharedString, Task};`,
`use music::GenreSection;`, `use state::{Discover, …};`. New fields:

```rust
    discover: Entity<Discover>,
    /// Shelves for New releases, drawn above Quick picks; `shelves` draws the rest.
    fresh: Entity<Shelves>,
    resume: Pager,
    /// The hero's cover and its sampled hue, kept so a cover is sampled once.
    tint: Option<(String, Option<Hsla>)>,
    sampling: Option<Task<()>>,
```

`new` takes `discover: Entity<Discover>` after `home`, observes it, and builds the second shelves:

```rust
        cx.observe(&discover, |_, _, cx| cx.notify()).detach();
        let fresh = cx.new(|cx| Shelves::new("home-fresh", me, playback.clone(), cx));
        cx.observe(&fresh, |_, _, cx| cx.notify()).detach();
```

`set_visible` forwards to both:

```rust
        self.discover
            .update(cx, |discover, cx| discover.set_visible(visible, cx));
```

- [ ] **Step 3: Sample the hero's hue**

Add to `impl HomeView`:

```rust
    /// Samples the featured cover's hue once per cover; the hero redraws when it lands.
    fn sample(&mut self, cover: Option<String>, cx: &mut Context<Self>) {
        let Some(cover) = cover else {
            self.tint = None;
            return;
        };
        if self.tint.as_ref().is_some_and(|(sampled, _)| *sampled == cover) {
            return;
        }
        self.tint = Some((cover.clone(), None));
        let task = ui::tint(SharedString::from(cover.clone()), cx);
        self.sampling = Some(cx.spawn(async move |this, cx| {
            let tint = task.await;
            this.update(cx, |this, cx| {
                this.tint = Some((cover, tint));
                cx.notify();
            })
            .ok();
        }));
    }
```

- [ ] **Step 4: Add the Jump back in deck**

```rust
    /// Jump back in: one page of what the user played last, across every provider.
    fn resume(&mut self, width: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items = self.home.read(cx).resume(cx);
        if items.is_empty() {
            return None;
        }
        let shape = Shape::new(width, items.len());
        let pages = shape.pages;
        let page = self.resume.fit(shape.columns, shape.pages);
        let opened = items.clone();
        let home = cx.entity().downgrade();

        Some(
            Picks::mixed("home-resume", items, self.playback.clone(), width, page)
                .title("home-resume")
                .on_previous(cx.listener(|this, _, _, cx| {
                    this.resume.page = this.resume.page.saturating_sub(1);
                    cx.notify();
                }))
                .on_next(cx.listener(move |this, _, _, cx| {
                    this.resume.page = (this.resume.page + 1).min(pages.saturating_sub(1));
                    cx.notify();
                }))
                .on_context_menu(move |place, event, _, cx| {
                    open_item_menu(&home, &opened, place, event, cx);
                })
                .into_any_element(),
        )
    }
```

- [ ] **Step 5: Lay the page out in `render`**

Replace the body of `render` from `let picks = …` to the end with:

```rust
        let discover = self.discover.read(cx);
        let featured = discover.featured();
        let cover = featured
            .as_ref()
            .and_then(|featured| featured::cover(featured, discover));
        let releases = discover.releases();
        let mixes = discover.mixes();
        self.sample(cover, cx);
        let tint = self.tint.as_ref().and_then(|(_, tint)| *tint);
        let hero = featured.as_ref().and_then(|featured| {
            featured::hero(featured, self.discover.read(cx), tint, &self.playback, cx)
        });

        let resume = self.resume(available, cx);
        let picks = self.quick_picks(available, cx);
        let width = self.width;

        let fresh: Vec<GenreSection> = (!releases.is_empty())
            .then(|| GenreSection {
                title: t!("home-new-releases").to_string(),
                items: releases.iter().cloned().map(GenreItem::Album).collect(),
                provider: None,
            })
            .into_iter()
            .collect();
        let fresh = (!fresh.is_empty()).then(|| {
            self.fresh
                .update(cx, |shelves, cx| {
                    shelves.render(Rc::new(fresh), Mode::Grid, width, window, cx)
                })
        });

        let feed = self.home.read(cx).sections();
        let feeding = self.home.read(cx).is_feeding();
        let mut lower: Vec<GenreSection> = mixes
            .iter()
            .map(|mix| GenreSection {
                title: t!("home-because", artist = mix.seed.artists.as_str()).to_string(),
                items: mix.tracks.iter().cloned().map(GenreItem::Track).collect(),
                provider: None,
            })
            .collect();
        lower.extend(feed.iter().cloned());
        let shelves = self
            .shelves
            .update(cx, |shelves, cx| match lower.is_empty() && feeding {
                true => shelves.pending(width, cx),
                false => vec![shelves.render(Rc::new(lower), Mode::Grid, width, window, cx)],
            });

        Scroller::new("home-page", &self.scrollbar)
            .p(theme.metrics.inset)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .children(hero)
                    .children(resume)
                    .children(fresh)
                    .child(picks)
                    .children(shelves),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
            .into_any_element()
```

Note: `Rising` — `PageHero` sits inside the page, which already enters through the workspace's
transition; wrap `hero` with `.rising("home-featured")` only if `ui::Rising` is a trait on
`Div` (check `crates/ui/src/lib.rs`); otherwise leave it, the page transition covers it.

- [ ] **Step 6: Construct `Discover` in `Root`**

In `crates/views/src/root.rs`, after the `home_state` line from Task 3:

```rust
        let discover = cx.new(|cx| {
            Discover::new(session.clone(), library.clone(), history_state.clone(), io.clone(), cx)
        });
        let home = cx.new(|cx| HomeView::new(home_state, discover, playback.clone(), cx));
```

and add `Discover` to the `state::{…}` import.

- [ ] **Step 7: Format, lint, test**

Run: `cargo fmt && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: no warnings, all tests pass.

- [ ] **Step 8: Commit (ask the user first)**

```bash
git add crates/views/src/screens/home crates/views/src/root.rs
git commit -m "feat(views): lay home out around a featured release"
```

---

### Task 6: Changelog and a real run

**Files:**
- Modify: `CHANGELOG.md` (`## [Unreleased]` → `### Added`, first bullet)

- [ ] **Step 1: Changelog entry**

```markdown
- The home page now opens on a featured new release, a Jump back in row, new releases from the
  artists you follow and mixes based on what you played.
```

- [ ] **Step 2: Run the app and check the Review Focus by hand**

Run: `cargo run --locked --package sonora` (signed in to at least two providers).
Check, in order:
1. Hero shows a release with a tinted wash; Play starts the album; Open album navigates.
2. Jump back in mixes items from more than one provider.
3. New from your artists lists only albums dated within 56 days; nothing year-only.
4. A Because you listened to … shelf has no track from the history.
5. Sign out of one provider: its releases and mixes disappear, nothing reappears from it.
6. Quit and relaunch with the network off: hero and releases draw from the cache.
7. Fresh profile (no follows, no history): page starts at Quick picks, no empty headings.

Take a screenshot of the finished page for the user.

- [ ] **Step 3: Commit (ask the user first)**

```bash
git add CHANGELOG.md
git commit -m "docs: note the discover home in the changelog"
```
