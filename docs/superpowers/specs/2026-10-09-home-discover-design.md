# Home with Discover — Design

The home page becomes one page for every signed-in provider at once, that helps the user get
back to what they were playing and find music they have not heard yet. It keeps everything it
draws today (Quick picks, the woven provider shelves) and adds a featured hero, a Resume grid,
New releases from followed artists, and radio mixes seeded from the listening history.

## Goals

1. **Discover**: new releases from followed artists and radio mixes seeded from recent plays,
   on top of what each provider already recommends.
2. **Combined**: the new sections mix every signed-in provider; nothing new is per provider.
3. **Richer**: one featured item at the top, tinted from its cover.
4. **Back quickly**: what the user played last is one click away, near the top.

## What exists (verified 2026-10-09)

- `state::Home` (`crates/state/src/home.rs`) already asks every active provider for
  `home_paged`, weaves the lanes a row at a time (`woven`) and holds back changes while the
  page is on screen (`visible`, `pending`, `land`).
- Provider feeds: Spotify `MADE_FOR_YOU` via pathfinder, Apple `/me/recommendations`, YouTube
  `genres::home`, Subsonic random/newest/frequent. Deezer returns an empty feed.
- `Library::state(shelf).artists()` holds every shelf's followed artists, primed from
  `state::snapshot` at startup.
- `MusicApi::artist(id)` returns `Artist { albums: Vec<Album>, .. }`; `Album` carries
  `release_date` (`YYYY-MM-DD`) and `year`.
- `MusicApi::track_radio(id)` exists on every provider that has radio.
- `state::History::tracks()` is the listening history, most recent first, ids tagged.
- `ui::palette::tint(url, cx)` samples a cover's dominant hue, `None` for greyscale art.
- `shared::hero::{PageHero, HeroPlayButton, release_date_label}`, `shared::shelves::Shelves`,
  `shared::cards::{album_card, track_card, item_card}`, `ui::Card` (row and tile modes),
  `storage::Cache` (`cache.sqlite`, key → text).

## Decisions

1. **A new entity, `state::Discover`**, beside `Home`. `Home` keeps weaving provider feeds;
   `Discover` owns new releases, mixes and their cache. `HomeView` reads both. `home.rs` does
   not grow beyond the one `resume` accessor.
2. **No new `MusicApi` method.** Releases come from `artist()` per followed artist, mixes from
   `track_radio`. A provider-native `new_releases()` with this as its default is a possible
   later step and is out of scope here.
3. **Ids route themselves.** Every id is tagged (`music::tagged`), so `Shelf::of(id)` picks the
   client for an artist or a seed; `Discover` has no provider routing of its own.
4. **Cache first.** Both lists are stored in `storage::Cache` and drawn from it at startup, then
   refetched when older than 12 hours. A snapshot is a copy only; deleting `cache.sqlite` costs one
   reload.
5. **Nothing jumps in view.** `Discover` follows `Home`'s rule: while the home page is visible a
   refetch is held and lands when the page is left. `HomeView::set_visible` tells both.

## State: `state::Discover` (`crates/state/src/discover.rs`)

```rust
pub struct Mix {
    pub seed: Track,
    pub tracks: Vec<Track>,
}

pub enum Featured {
    Release(Album),
    Mix(usize), // index into mixes()
}

impl Discover {
    pub fn releases(&self) -> Rc<Vec<Album>>;
    pub fn mixes(&self) -> Rc<Vec<Mix>>;
    pub fn featured(&self) -> Option<Featured>;
    pub fn is_loading(&self) -> bool;
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>);
}
```

Constants at module top: `ARTISTS = 24`, `IN_FLIGHT = 4`, `FRESH = 56` days, `SEEDS = 3`,
`MIX_MIN = 8`, `MIX_MAX = 25`, `STALE = 12h`.

### New releases

1. Wait until every active shelf's artists part is ready (`cx.observe(&library, …)`).
2. Gather `artists()` from every `Shelf::Of(slug)` in `Session::active_slugs()` order, sorted by
   `added_at` descending (missing last), first `ARTISTS`.
3. `client_of(Shelf::of(id)).artist(id)` for each, at most `IN_FLIGHT` at a time, all inside
   `io.spawn`.
4. Keep albums whose `release_date` parses and is within `FRESH` days of today. An album with only
   a year never counts.
5. Deduplicate on lowercased `(name, first artist name)`, keeping the first provider's copy.
6. Sort newest first.

A failing artist is logged (`discover: cannot load artist …`) and skipped.

### Mixes

1. Wait for `History` to be ready (`cx.observe(&history, …)`).
2. Seeds: walk `History::tracks()` and take the first `SEEDS` playable tracks whose first artist
   differs from every seed taken so far.
3. `client_of(Shelf::of(seed_id)).track_radio(seed_id)` for each, inside `io.spawn`.
4. Drop tracks that are in the history, in any shelf's favorites, or are the seed itself; cap at
   `MIX_MAX`. A mix with fewer than `MIX_MIN` left is dropped.

A failing seed is logged (`discover: cannot load radio …`) and skipped.

### Featured

The first release if there is one, otherwise the first mix, otherwise nothing. For a featured
release `Discover` also loads its tracks once (`album_tracks`), as `featured_tracks()`, so the
hero's play button has something to start; the button is disabled until they arrive.

### Cache

Keys `discover/releases` and `discover/mixes`, value `{ saved_at, rows }` as JSON (`Album` and
`Track` already derive `Serialize`; `Mix` derives it too). Read at construction; a fetch starts
when the value is missing or older than `STALE`.

### Session

On `SignedIn` / `SignedOut`: drop both tasks, clear both lists, `Cache::forget("discover/")`,
start over. `Reconnected` and `LocalChanged` do nothing.

### Wiring

Constructed in `views::Root::new` beside `Home` (which is built there too), from the `Library`,
`Session` and `History` entities `Root` already holds. Not on the `Sonora` global: only the home
page reads it.

## State: `Home::resume`

`pub fn resume(&self, cx: &App) -> Rc<Vec<GenreItem>>`: the woven `listen_again` of every
provider, then `History::tracks()` as `GenreItem::Track`, deduplicated by id, first 8.
`Home` receives the `History` entity in its constructor and observes it.

## View: layout B (`crates/views/src/screens/home/`)

Top to bottom, inside the existing `Scroller`:

1. **Featured hero** — `home/featured.rs`. Built on `PageHero` + `HeroPlayButton`.
   - Background: a vertical gradient from the cover's `ui::palette::tint` (sampled once per url,
     kept on the view) to `theme.background`; no tint → `theme.secondary`.
   - Release: eyebrow `home-featured-release` with the provider name, the album title, artist
     links (`InlineLinks`), `release_date_label`. Click navigates to `Destination::Album`; play
     starts the album.
   - Mix: eyebrow `home-featured-mix`, title `home-because` with the seed's artist, play starts
     the mix tracks.
   - Below `Room::Snug` the cover stacks above the text. Appears through `Rising`.
   - Absent when `Discover::featured()` is `None`.
2. **Resume** — title `home-resume`, a second `Picks::mixed` deck (the row-mode deck Quick picks
   already uses) over `Home::resume`, up to 8 items, with its own pager. Clicking an item plays
   it or opens it, as Quick picks do. Hidden when empty.
3. **New releases** — title `home-new-releases`, one `GenreSection` of `GenreItem::Album`s built
   at render time and drawn by a second `Shelves` entity (it sits above Quick picks). Hidden when
   empty.
4. **Quick picks** — unchanged.
5. **Mixes** — one shelf per `Mix`, title `home-because { $artist }`, built as a `GenreSection`
   of `GenreItem::Track`s at render time and drawn by the same `Shelves` as the provider
   sections, so a card behaves like every other track card. The hero's play button is what
   starts a whole mix.
6. **Provider shelves** — the existing woven sections, unchanged.

The failure page stays as it is and shows only when every section above is empty.
`HomeView` observes `Discover` and forwards `set_visible` to it.

## Localization

New keys in `assets/i18n/en-US/main.ftl`, translated in `ru`, `uk`, `pl` where possible:

```ftl
home-featured-release = New release · { $provider }
home-featured-mix = Mix
home-resume = Jump back in
home-new-releases = New from your artists
home-because = Because you listened to { $artist }
```

Then re-run `scripts/i18n-coverage.py`.

## Out of scope

- A provider-native `new_releases()` trait method.
- Genre or mood based recommendations.
- Any setting to hide a home section.

## Verification

No new tests (house rule). `cargo fmt`, `cargo clippy --workspace --all-targets`,
`cargo test --workspace`, then run the app signed in to at least two providers and check: hero
tinted, Resume mixes providers, New releases shows only dated albums from the last 56 days, mixes
contain no history tracks, a second launch draws from the cache before the network answers.

## Changelog

Under `## [Unreleased]` → `Added`: "The home page now opens on a featured new release, a Jump
back in grid, new releases from the artists you follow and mixes based on what you played."
