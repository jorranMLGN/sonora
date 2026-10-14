# Search filters, sorting and history — Design

Desktop search learns to narrow what it shows. A type tab picks songs, artists, albums or
playlists, a filter menu picks which providers take part and can limit hits to the library,
a sort menu orders hits by title, artist, duration or year, and an empty field shows recent
searches. This is phase 1. Pagination ("load more") and native artist search change the
`MusicApi` trait and get their own spec (phase 2).

## Goals

1. **Narrow**: one type at a time, with more hits than the mixed view's 20 per kind.
2. **Fewer calls**: a hidden provider or an excluded type is never asked.
3. **Order**: sort by title, artist, duration or year, ascending or descending.
4. **Come back**: recent searches one click away when the field is empty.

## Non-goals

- Mobile (`apps/ios`, `crates/mobile`). It follows later on the same state.
- Pagination, `MusicApi::search_artists`: phase 2.
- The query or lens in the route. Back navigation does not restore filters.
- Persisting the lens across restarts.

## What exists (verified 2026-10-14)

- `MusicApi::{search, search_albums, search_playlists}` (`crates/music/src/lib.rs:316-324`),
  query only, provider-fixed limits (Spotify 20, Apple 25, Deezer 50/30/30, SoundCloud 50,
  Subsonic 50/30).
- `state::Search` (`crates/state/src/search.rs`): `ask()` debounces 250 ms and fans out three
  `io.spawn` calls per active provider; `catalogs` keeps the last rows per provider;
  `salvaged` keeps old rows when a part fails; `served` is the query that every part answered;
  `rank()` scores library and catalog hits, `capped` truncates to `LIMIT = 20` per kind.
- `SearchView` (`crates/views/src/screens/search.rs`): best-match card, one column per lane
  grouped by `GROUPS`, a mixed list when stacked, genre grid when empty, keyboard navigation.
- `ui::TabBar`, `ui::{Filter, FlagAxis, SortAxis, Sort}`, `chrome::tools::{sorts, filters}`,
  `Popovers`. `detail.rs` already cycles a sort axis.
- `app_state` (`key`, `value`) in `state.sqlite`; `settings.rs:797` stores one JSON row there.

## Decisions

1. **The lens lives in `state::Search`**, not the view, because it decides which network
   calls go out.
2. **Hidden sources are a deny list** (`hidden`), so a provider that signs in later takes part
   without touching the lens.
3. **Changing the lens refetches only when it widens**: a source turned on, or the type going
   from one kind to a broader one. Otherwise `rank()` reruns on the cached `catalogs`.
4. **`served` records what was asked**, the query plus the kinds and sources fetched, so
   `ask()` does not treat a narrower earlier answer as complete.
5. **Cap**: 20 per kind under All, 50 when one kind is selected.
6. **Sorting happens after ranking and within a kind.** Items without the sort value (an
   album has no duration) keep their relevance order after the ones that have it. Best match
   is the top hit before sorting; the view hides the card while a sort is active.
7. **History is its own entity, `state::SearchHistory`**, stored as one JSON row
   (`app_state.key = 'search-history'`), at most 20 queries, newest first, deduplicated
   case-insensitively. A query is recorded on Enter or when a hit is opened or played, never
   per keystroke.

## State

`crates/state/src/search.rs`:

```rust
/// What the search screen narrows its hits to. The default shows everything, ranked.
#[derive(Clone, Default, PartialEq)]
pub struct Lens {
    pub kind: Option<Kind>,
    pub hidden: Vec<&'static str>,
    pub mine_only: bool,
    pub order: Option<(Axis, Sort)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Axis { Title, Artist, Duration, Year }
```

- `Search::lens()`, `Search::set_lens(lens, cx)`.
- `ask()` skips `search` / `search_albums` / `search_playlists` for kinds the lens excludes
  (Artist needs tracks and albums, since artists are derived from them) and skips hidden
  providers.
- `rank()` caps per the lens, drops hits from hidden lanes, drops catalog-only hits when
  `mine_only`, then applies `order`.
- On `SessionEvent::SignedOut(slug)` the slug leaves `hidden`.

`crates/state/src/search_history.rs`: `SearchHistory { entries: Vec<String>, task }` with
`entries()`, `record(query, cx)`, `remove(query, cx)`, `clear(cx)`. Load and save run on `Io`.
A failed read or write logs `search: history: …` and leaves the list empty or unchanged.

## Views

`crates/views/src/screens/search.rs`:

```
[ search field ....................... ]  [⇅] [⏷]
[ All | Songs | Artists | Albums | Playlists ]
[ Best match ]                  (relevance + All only)
| Spotify | YouTube | Library | (columns as today)
```

- `TabBar` sets `lens.kind`. With one kind a column is one flat list; `seats()` takes the
  lens.
- Sort picker through `tools::sorts()`. Axes per kind: All → Title, Artist; Songs → + Duration;
  Albums → + Year; Artists and Playlists → Title. Clicking cycles ascending, descending, off.
- Filter picker through `tools::filters()`: one `Filter::Flag` per signed-in source, plus
  "Only in library"; Reset restores `Lens::default()`.
- Empty field: a "Recent searches" section above the genre grid, each row fills the query,
  ✕ removes it, "Clear" empties the list. Hidden when there is no history.
- Filters that leave nothing show `search-filtered-empty` with a reset button.
- Stacked layout gets the same tab bar and pickers; the mixed list follows the lens.
- `alt-1` … `alt-5` pick a tab: new actions in `crates/input`, bound in `SEARCH_CONTEXT`.
- New Fluent keys in `assets/i18n/en-US/main.ftl`: `search-all`, `search-recent`,
  `search-recent-clear`, `search-mine-only`, `search-filtered-empty`, sort axis labels
  (reuse existing `sort-*` keys where they exist).

## Errors

- A failing provider keeps today's `salvaged` behaviour. A hidden provider is never asked, so
  it never reports.
- History I/O failure never blocks search.

## Testing (TDD)

Tests are written first, in `#[cfg(test)] mod tests` at the bottom of the file they cover:

- `search.rs`: one kind returns only that kind, capped at 50; `mine_only` drops catalog-only
  hits; hidden lanes drop out; duration sort ascending and descending, valueless items last;
  widening the lens marks the query unserved, narrowing does not.
- `search_history.rs`: record deduplicates case-insensitively and moves to front; at most 20;
  remove and clear.

## Changelog

Under `## [Unreleased]` → `Added`:

- Search can now be narrowed to songs, artists, albums or playlists, limited to chosen
  providers or your library, and sorted by title, artist, duration or year.
- Recent searches show up when the search field is empty.
