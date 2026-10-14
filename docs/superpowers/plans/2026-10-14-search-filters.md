# Search filters, sorting and history Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Desktop search gets a type tab bar, a source and "only in library" filter, a sort menu and recent searches.

**Architecture:** A `Lens` in `state::Search` decides which provider calls go out (`Parts`, `Served`) and which ranked hits survive (`narrowed`, `sorted`). A new `state::SearchHistory` entity keeps recent queries in `app_state`. `SearchView` drives both through `TabBar`, `chrome::tools::{sorts, filters}` and a recent-searches section.

**Tech Stack:** Rust 2024, GPUI, rusqlite, serde_json, Fluent (`i18n`).

**Spec:** `docs/superpowers/specs/2026-10-14-search-filters-design.md`

## Global Constraints

- Work in worktree `/home/jorran/Documents/github/sonora-search`, branch `feat/search-filters`.
- **Never `git commit` or `git push` without the user's explicit approval for that one commit.** Every "Commit" step below means: show the exact command and files, ask, wait for yes.
- Commit messages: Conventional Commits, lowercase, imperative, no body, no `Co-Authored-By`, no assistant attribution.
- No hardcoded colours, radii or sizes; read `cx.theme()` / `theme.metrics`.
- Every user-facing string comes from `assets/i18n/en-US/main.ftl`; never call `t!` in a constructor.
- Network work runs on `Io` and is awaited from `cx.spawn`; tasks live in fields, never detached.
- Tests are `#[cfg(test)] mod tests` at the bottom of the file they cover. TDD: failing test first.
- Cap 20 per kind under All, 50 with one kind. History keeps 20, newest first, case-insensitive dedupe.
- Commands run from the worktree root. If mold is missing, prefix with `RUSTFLAGS=""`.

## Review Focus

1. Widening the lens while a fetch is in flight: the old task is dropped and the wider fetch wins; no stale answer overwrites it (Task 3, `set_lens` drops `task` before `ask`).
2. Every source hidden: the screen shows the filtered-empty message with Reset, not a blank area (Task 5, step for empty lanes).
3. A hidden provider signs out and back in: it takes part again (Task 3 test `signing_out_forgets_a_hidden_source`).
4. A query recorded before history finished loading is kept and the stored list is not overwritten by the partial one (Task 4 test `a_record_during_load_merges`).
5. Switching from Songs sorted by duration to All drops the duration order instead of sorting by a missing value (Task 3 test `changing_kind_drops_an_axis_it_lacks`).

---

### Task 1: Lens, Parts and Served drive which calls go out

**Files:**
- Modify: `crates/state/src/search.rs` (types near `Kind`, `Search` struct, `ask`, `providers`, tests)
- Modify: `crates/state/src/lib.rs` (re-export `Lens`, `Axis`)

**Interfaces:**
- Produces: `pub struct Lens { pub kind: Option<Kind>, pub hidden: Vec<&'static str>, pub mine_only: bool, pub order: Option<(Axis, ui::Sort)> }` (`Clone, Debug, Default, PartialEq`), `pub enum Axis { Title, Artist, Duration, Year }`, private `Parts::of(Option<Kind>)`, `Parts::covers`, `Served { query, parts, slugs }`, `Served::covers(&Served) -> bool`, `Search::wanted(&self, &str, &Context<Self>) -> Served`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/state/src/search.rs`:

```rust
    fn served(query: &str, kind: Option<Kind>, slugs: &[&'static str]) -> Served {
        Served {
            query: query.to_owned(),
            parts: Parts::of(kind),
            slugs: slugs.to_vec(),
        }
    }

    #[test]
    fn artists_need_songs_and_albums() {
        assert_eq!(
            Parts::of(Some(Kind::Artist)),
            Parts {
                songs: true,
                albums: true,
                playlists: false,
            }
        );
    }

    #[test]
    fn narrowing_is_already_served() {
        let all = served("queen", None, &["spotify", "youtube"]);
        assert!(all.covers(&served("queen", Some(Kind::Album), &["spotify"])));
    }

    #[test]
    fn widening_the_kind_needs_a_fetch() {
        let albums = served("queen", Some(Kind::Album), &["spotify"]);
        assert!(!albums.covers(&served("queen", None, &["spotify"])));
    }

    #[test]
    fn a_new_source_needs_a_fetch() {
        let one = served("queen", None, &["spotify"]);
        assert!(!one.covers(&served("queen", None, &["spotify", "youtube"])));
    }

    #[test]
    fn another_query_needs_a_fetch() {
        let asked = served("queen", None, &["spotify"]);
        assert!(!asked.covers(&served("queens", None, &["spotify"])));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p state search::tests::`
Expected: compile error, `cannot find type Served` / `Parts`.

- [ ] **Step 3: Implement the types**

In `crates/state/src/search.rs`, add `Debug` to `Kind`'s derive (`#[derive(Clone, Copy, Debug, PartialEq, Eq)]`), add `use ui::Sort;` to the imports, and add after `impl Kind`:

```rust
/// What the search screen narrows its hits to. The default shows every kind from every
/// signed-in provider, ranked by relevance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lens {
    pub kind: Option<Kind>,
    /// Providers the user switched off. A deny list, so a provider that signs in later takes
    /// part without touching the lens.
    pub hidden: Vec<&'static str>,
    pub mine_only: bool,
    /// `None` is relevance.
    pub order: Option<(Axis, Sort)>,
}

/// Which value of a hit the search screen orders by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Title,
    Artist,
    Duration,
    Year,
}

/// Which of a provider's three search calls a lens needs. Artists come from tracks and
/// albums, since no provider is asked for artists directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Parts {
    songs: bool,
    albums: bool,
    playlists: bool,
}

impl Parts {
    fn of(kind: Option<Kind>) -> Self {
        match kind {
            None => Self {
                songs: true,
                albums: true,
                playlists: true,
            },
            Some(Kind::Song) => Self {
                songs: true,
                ..Self::default()
            },
            Some(Kind::Artist) => Self {
                songs: true,
                albums: true,
                ..Self::default()
            },
            Some(Kind::Album) => Self {
                albums: true,
                ..Self::default()
            },
            Some(Kind::Playlist) => Self {
                playlists: true,
                ..Self::default()
            },
        }
    }

    fn covers(self, other: Self) -> bool {
        (self.songs || !other.songs)
            && (self.albums || !other.albums)
            && (self.playlists || !other.playlists)
    }
}

/// What one search fetched: the query, the calls and the providers it asked.
#[derive(Clone, Debug, PartialEq)]
struct Served {
    query: String,
    parts: Parts,
    slugs: Vec<&'static str>,
}

impl Served {
    /// Whether this answer already holds everything `wanted` asks for, so no fetch is needed.
    fn covers(&self, wanted: &Served) -> bool {
        self.query == wanted.query
            && self.parts.covers(wanted.parts)
            && wanted.slugs.iter().all(|slug| self.slugs.contains(slug))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p state search::tests::`
Expected: all `search::tests` PASS.

- [ ] **Step 5: Wire `Lens` and `Served` into `Search`**

In `struct Search`, replace `served: Option<String>,` with `served: Option<Served>,` and add `lens: Lens,`. In `Search::new`, initialise `served: None,` (unchanged) and `lens: Lens::default(),`.

Add accessors and `wanted` inside `impl Search`:

```rust
    pub fn lens(&self) -> &Lens {
        &self.lens
    }

    /// What answering `query` under the current lens asks for.
    fn wanted(&self, query: &str, cx: &Context<Self>) -> Served {
        Served {
            query: query.to_owned(),
            parts: Parts::of(self.lens.kind),
            slugs: self
                .providers(cx)
                .into_iter()
                .map(|(slug, _, _)| slug)
                .collect(),
        }
    }
```

Rewrite the top of `ask` and its spawn so skipped parts are never sent:

```rust
    pub fn ask(&mut self, query: &str, cx: &mut Context<Self>) {
        let query = query.trim().to_owned();
        let wanted = self.wanted(&query, cx);
        let answered =
            self.loading || self.served.as_ref().is_some_and(|served| served.covers(&wanted));
        if query == self.query && answered {
            return;
        }
        self.query = query.clone();
        self.error = None;

        if query.is_empty() {
            self.task = None;
            self.catalogs.clear();
            self.served = Some(wanted);
            self.loading = false;
            self.rank(cx);
            return;
        }

        self.rank(cx);

        let asking = self.providers(cx);
        if asking.is_empty() {
            self.loading = false;
            cx.notify();
            return;
        }

        self.loading = true;
        cx.notify();

        let io = self.io.clone();
        let parts = wanted.parts;
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;

            let sent: Vec<_> = asking
                .into_iter()
                .map(|(slug, name, client)| {
                    let songs = parts.songs.then(|| {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search(&asked).await })
                    });
                    let albums = parts.albums.then(|| {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search_albums(&asked).await })
                    });
                    let playlists = parts.playlists.then(|| {
                        let asked = query.clone();
                        io.spawn(async move { client.search_playlists(&asked).await })
                    });
                    (slug, name, songs, albums, playlists)
                })
                .collect();

            let mut answered = Vec::new();
            for (slug, name, songs, albums, playlists) in sent {
                answered.push((
                    slug,
                    name,
                    awaited(songs).await,
                    awaited(albums).await,
                    awaited(playlists).await,
                ));
            }
```

The `this.update` block stays the same except the success arm: replace `this.served = Some(query);` with `this.served = Some(wanted);` (move `wanted` into the closure; it is not used after the spawn).

Add the helper at the bottom of the file, above `#[cfg(test)]`:

```rust
/// The rows a search part answered with, or none for a part the lens did not ask for.
async fn awaited<T>(handle: Option<tokio::task::JoinHandle<Result<Vec<T>>>>) -> Result<Vec<T>> {
    match handle {
        Some(handle) => join(handle).await,
        None => Ok(Vec::new()),
    }
}
```

In `crates/state/src/lib.rs`, extend the `search` re-export with `Axis, Lens` (find the line `pub use search::{…}` and add them).

- [ ] **Step 6: Check, test, lint**

Run: `cargo fmt && cargo test -p state search::tests:: && cargo clippy -p state --all-targets -- -D warnings`
Expected: tests PASS, clippy clean.

- [ ] **Step 7: Commit (ask first)**

```bash
git add crates/state/src/search.rs crates/state/src/lib.rs
git commit -m "feat(state): fetch only the search parts and providers a lens asks for"
```

---

### Task 2: Ranking takes a cap and marks library hits

**Files:**
- Modify: `crates/state/src/search.rs` (`Scored`, `Found`, `rank`, `songs`, `albums_of`, `playlists_of`, `artists`, `capped`, `Search::rank`, tests)

**Interfaces:**
- Consumes: `Lens` from Task 1.
- Produces: `pub struct Found { pub slug, pub hit, pub mine: bool }`; `const FOCUSED: usize = 50`; free fn `rank(library, albums, playlists, catalog, portraits, asked, limit: usize) -> Vec<Scored>`; `Scored.mine: bool`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests`:

```rust
    #[test]
    fn one_kind_lifts_the_cap() {
        let catalog = Catalog {
            tracks: (0..30)
                .map(|at| track(&format!("Song {at}"), "Band", "Record"))
                .collect(),
            ..Catalog::default()
        };
        let songs = |limit| {
            super::rank(&[], &[], &[], &catalog, &HashMap::new(), "song", limit)
                .iter()
                .filter(|scored| scored.hit.kind() == Kind::Song)
                .count()
        };
        assert_eq!(songs(LIMIT), 20);
        assert_eq!(songs(FOCUSED), 30);
    }

    #[test]
    fn library_hits_are_marked_mine() {
        let catalog = Catalog {
            tracks: vec![track("Bohemian Like You", "The Dandy Warhols", "Thirteen Tales")],
            ..Catalog::default()
        };
        let scored = super::rank(
            &[rhapsody()],
            &[],
            &[],
            &catalog,
            &HashMap::new(),
            "bohemian",
            LIMIT,
        );
        let mine = |name: &str| {
            scored.iter().find_map(|scored| match &scored.hit {
                Hit::Song(track) if track.name == name => Some(scored.mine),
                _ => None,
            })
        };
        assert_eq!(mine("Bohemian Rhapsody"), Some(true));
        assert_eq!(mine("Bohemian Like You"), Some(false));
    }
```

Update the existing test helper and `artist_pictures_come_from_portraits` to pass `LIMIT` as the new last argument of `super::rank`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p state search::tests::`
Expected: compile error, `FOCUSED` not found / `rank` takes 6 arguments.

- [ ] **Step 3: Implement**

- Add `const FOCUSED: usize = 50;` under `LIMIT` with `/// How many hits of one kind a lane keeps when the lens shows only that kind.`
- `struct Scored` gains `mine: bool`. `pub struct Found` gains `pub mine: bool` with `/// Whether the hit is in the user's library rather than only in the catalog.`
- `fn rank(…, asked: &str, limit: usize)`; pass `limit` to `songs`, `artists`, `albums_of`, `playlists_of`, each of which takes `limit: usize` as last parameter and ends with `capped(scored, limit)` / `capped(…, limit)`.
- `fn capped(mut scored: Vec<Scored>, limit: usize) -> Vec<Scored>` truncates to `limit`.
- In `songs`, `albums_of`, `playlists_of`: set `mine: rank.is_none()` on each `Scored`.
- In `artists`, build `Scored { score, popularity, mine: artist.saved > 0, hit: Hit::Artist(artist) }`.
- In `Search::rank`, compute `let limit = match self.lens.kind { None => LIMIT, Some(_) => FOCUSED };`, pass it to the free `rank`, and map to `Found { slug, mine: scored.mine, hit: scored.hit }`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p state search::tests::`
Expected: all PASS, including the existing ranking tests.

- [ ] **Step 5: Lint**

Run: `cargo fmt && cargo clippy -p state --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit (ask first)**

```bash
git add crates/state/src/search.rs
git commit -m "feat(state): lift the search cap for one kind and mark library hits"
```

---

### Task 3: The lens filters, sorts and refetches only when it widens

**Files:**
- Modify: `crates/state/src/search.rs` (`impl Lens`, `impl Axis`, `narrowed`, `sorted`, `measure`, `Search::{set_lens, sources, lanes, providers, rank, best}`, session subscription, tests)

**Interfaces:**
- Consumes: `Lens`, `Axis`, `Served`, `Found.mine`, `FOCUSED` from Tasks 1–2.
- Produces: `Axis::of(Option<Kind>) -> &'static [Axis]`, `Lens::with_kind`, `Lens::with_source(slug, on)`, `Lens::cycled(axis)`, `Lens::forget(slug)`, `Search::set_lens(Lens, cx)`, `Search::sources(&App) -> Vec<&'static str>` (every active non-local provider, hidden or not), `Search::lanes` (sources minus hidden), `Search::best()` returning `None` unless kind is All and order is relevance.

- [ ] **Step 1: Write the failing tests**

```rust
    fn found(hit: Hit, mine: bool) -> Found {
        Found {
            slug: "spotify",
            hit,
            mine,
        }
    }

    fn timed(name: &str, seconds: u64) -> Hit {
        Hit::Song(Track {
            duration: Duration::from_secs(seconds),
            ..track(name, "Band", "Record")
        })
    }

    fn record(name: &str) -> Hit {
        Hit::Album(AlbumHit {
            id: name.to_owned(),
            name: name.to_owned(),
            artists: "Band".to_owned(),
            artist_refs: Vec::new(),
            cover: None,
            year: 2001,
        })
    }

    fn names(hits: &[Found]) -> Vec<&str> {
        hits.iter()
            .map(|found| match &found.hit {
                Hit::Song(track) => track.name.as_str(),
                Hit::Album(album) => album.name.as_str(),
                Hit::Artist(artist) => artist.name.as_str(),
                Hit::Playlist(list) => list.name.as_str(),
            })
            .collect()
    }

    #[test]
    fn one_kind_keeps_only_that_kind() {
        let lens = Lens::default().with_kind(Some(Kind::Album));
        let hits = narrowed(vec![found(timed("Song", 60), false), found(record("Record"), false)], &lens);
        assert_eq!(names(&hits), ["Record"]);
    }

    #[test]
    fn mine_only_drops_catalog_hits() {
        let lens = Lens {
            mine_only: true,
            ..Lens::default()
        };
        let hits = narrowed(vec![found(timed("Saved", 60), true), found(timed("Found", 60), false)], &lens);
        assert_eq!(names(&hits), ["Saved"]);
    }

    #[test]
    fn duration_sorts_both_ways_and_valueless_hits_trail() {
        let hits = || {
            vec![
                found(timed("Long", 300), false),
                found(record("Record"), false),
                found(timed("Short", 60), false),
            ]
        };
        let up = Lens::default().cycled(Axis::Duration);
        assert_eq!(names(&narrowed(hits(), &up)), ["Short", "Long", "Record"]);
        let down = up.cycled(Axis::Duration);
        assert_eq!(names(&narrowed(hits(), &down)), ["Long", "Short", "Record"]);
    }

    #[test]
    fn cycling_an_axis_returns_to_relevance() {
        let lens = Lens::default().cycled(Axis::Title).cycled(Axis::Title).cycled(Axis::Title);
        assert_eq!(lens.order, None);
    }

    #[test]
    fn changing_kind_drops_an_axis_it_lacks() {
        let lens = Lens::default()
            .with_kind(Some(Kind::Song))
            .cycled(Axis::Duration)
            .with_kind(None);
        assert_eq!(lens.order, None);
    }

    #[test]
    fn signing_out_forgets_a_hidden_source() {
        let lens = Lens::default().with_source("youtube", false).forget("youtube");
        assert!(lens.hidden.is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p state search::tests::`
Expected: compile error, `with_kind` / `narrowed` not found.

- [ ] **Step 3: Implement the pure lens logic**

Add `use std::cmp::Ordering;` to the imports, then after `enum Axis`:

```rust
impl Axis {
    /// The axes that mean something for `kind`; `None` is the mixed All tab.
    pub fn of(kind: Option<Kind>) -> &'static [Axis] {
        match kind {
            None => &[Axis::Title, Axis::Artist],
            Some(Kind::Song) => &[Axis::Title, Axis::Artist, Axis::Duration],
            Some(Kind::Album) => &[Axis::Title, Axis::Artist, Axis::Year],
            Some(Kind::Artist | Kind::Playlist) => &[Axis::Title],
        }
    }
}

impl Lens {
    /// Shows `kind` only, dropping a sort axis that kind has no value for.
    pub fn with_kind(mut self, kind: Option<Kind>) -> Self {
        self.kind = kind;
        self.order = self
            .order
            .filter(|(axis, _)| Axis::of(kind).contains(axis));
        self
    }

    /// Lets `slug` take part (`on`) or hides it.
    pub fn with_source(self, slug: &'static str, on: bool) -> Self {
        let mut lens = self.forget(slug);
        if !on {
            lens.hidden.push(slug);
        }
        lens
    }

    /// Steps `axis` through ascending, descending and back to relevance.
    pub fn cycled(mut self, axis: Axis) -> Self {
        self.order = match self.order {
            Some((current, Sort::Ascending)) if current == axis => Some((axis, Sort::Descending)),
            Some((current, Sort::Descending)) if current == axis => None,
            _ => Some((axis, Sort::Ascending)),
        };
        self
    }

    /// Drops `slug` from the hidden sources, for a provider that signed out.
    pub fn forget(mut self, slug: &str) -> Self {
        self.hidden.retain(|known| *known != slug);
        self
    }
}
```

Add at the bottom of the file, above `awaited`:

```rust
/// The ranked hits the lens lets through, in the lens's order. Hidden providers are already
/// gone, since `lanes` leaves them out before ranking.
fn narrowed(mut hits: Vec<Found>, lens: &Lens) -> Vec<Found> {
    hits.retain(|found| {
        lens.kind.is_none_or(|kind| found.kind() == kind) && (!lens.mine_only || found.mine)
    });
    sorted(&mut hits, lens.order);
    hits
}

/// Orders `hits` by one axis. The sort is stable and hits without that value keep their
/// relevance order behind the ones that have it.
fn sorted(hits: &mut [Found], order: Option<(Axis, Sort)>) {
    let Some((axis, sort)) = order else {
        return;
    };
    hits.sort_by(|left, right| match (measure(&left.hit, axis), measure(&right.hit, axis)) {
        (Some(left), Some(right)) => match sort {
            Sort::Ascending => left.cmp(&right),
            Sort::Descending => right.cmp(&left),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Measure {
    Text(String),
    Number(u64),
}

fn measure(hit: &Hit, axis: Axis) -> Option<Measure> {
    let text = |value: &str| (!value.is_empty()).then(|| Measure::Text(value.to_lowercase()));
    match (axis, hit) {
        (Axis::Title, Hit::Song(track)) => text(&track.name),
        (Axis::Title, Hit::Artist(artist)) => text(&artist.name),
        (Axis::Title, Hit::Album(album)) => text(&album.name),
        (Axis::Title, Hit::Playlist(list)) => text(&list.name),
        (Axis::Artist, Hit::Song(track)) => text(&track.artists),
        (Axis::Artist, Hit::Album(album)) => text(&album.artists),
        (Axis::Artist, Hit::Artist(artist)) => text(&artist.name),
        (Axis::Artist, Hit::Playlist(list)) => text(&list.owner),
        (Axis::Duration, Hit::Song(track)) => (!track.duration.is_zero())
            .then(|| Measure::Number(track.duration.as_millis() as u64)),
        (Axis::Year, Hit::Album(album)) => {
            (album.year > 0).then(|| Measure::Number(album.year as u64))
        }
        (Axis::Duration | Axis::Year, _) => None,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p state search::tests::`
Expected: all PASS.

- [ ] **Step 5: Wire the lens into `Search`**

- `sources` / `lanes` (replace the current `lanes`):

```rust
    /// Every signed-in streaming provider, whether the lens hides it or not.
    pub fn sources(&self, cx: &App) -> Vec<&'static str> {
        self.session
            .read(cx)
            .active_slugs()
            .into_iter()
            .filter(|slug| *slug != music::tag::LOCAL)
            .collect()
    }

    /// The providers that get a results column: the sources the lens does not hide.
    pub fn lanes(&self, cx: &App) -> Vec<&'static str> {
        self.sources(cx)
            .into_iter()
            .filter(|slug| !self.lens.hidden.contains(slug))
            .collect()
    }
```

- `providers`: add `.filter(|slug| !self.lens.hidden.contains(slug))` before `.filter_map`.
- `Search::rank`: after building the `Vec<Found>`, assign `self.hits = narrowed(found, &self.lens);`.
- `best`:

```rust
    /// The top hit, shown only on the All tab in relevance order, where "top" means something.
    pub fn best(&self) -> Option<&Found> {
        match self.lens.kind.is_none() && self.lens.order.is_none() {
            true => self.hits.first(),
            false => None,
        }
    }
```

- `set_lens`:

```rust
    /// Applies a new lens. A narrower lens reranks what is already fetched; a wider one drops
    /// any fetch in flight and asks again.
    pub fn set_lens(&mut self, lens: Lens, cx: &mut Context<Self>) {
        if self.lens == lens {
            return;
        }
        self.lens = lens;
        let query = self.query.clone();
        let wanted = self.wanted(&query, cx);
        match self.served.as_ref().is_some_and(|served| served.covers(&wanted)) {
            true => self.rank(cx),
            false => {
                self.task = None;
                self.loading = false;
                self.query.clear();
                self.ask(&query, cx);
            }
        }
    }
```

- Session subscription in `Search::new`: split the arm so sign-out forgets the slug:

```rust
            SessionEvent::SignedIn(_) | SessionEvent::SignedOut(_) => {
                if let SessionEvent::SignedOut(slug) = event {
                    this.lens = std::mem::take(&mut this.lens).forget(slug);
                }
                let pending = this.query.clone();
                this.query.clear();
                this.catalogs.clear();
                this.ask(&pending, cx);
            }
```

- [ ] **Step 6: Check, test, lint**

Run: `cargo fmt && cargo test -p state && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, clean. `views` still compiles since `lanes` kept its signature.

- [ ] **Step 7: Commit (ask first)**

```bash
git add crates/state/src/search.rs
git commit -m "feat(state): narrow and sort search hits through a lens"
```

---

### Task 4: SearchHistory remembers finished searches

**Files:**
- Create: `crates/state/src/search_history.rs`
- Modify: `crates/state/src/lib.rs` (`mod`, `pub use`, `Sonora` field, `init`)

**Interfaces:**
- Produces: `pub struct SearchHistory` with `new(Database, Io, &mut Context<Self>)`, `entries(&self) -> &[String]`, `record(&mut self, &str, cx)`, `remove(&mut self, &str, cx)`, `clear(&mut self, cx)`; `Sonora::search_history: Entity<SearchHistory>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/state/src/search_history.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn entries(queries: &[&str]) -> Vec<String> {
        queries.iter().map(|query| (*query).to_owned()).collect()
    }

    #[test]
    fn a_record_goes_to_the_front_once() {
        let mut kept = entries(&["daft punk", "Queen"]);
        assert!(remembered(&mut kept, "queen"));
        assert_eq!(kept, entries(&["queen", "daft punk"]));
    }

    #[test]
    fn a_blank_query_is_not_kept() {
        let mut kept = entries(&["queen"]);
        assert!(!remembered(&mut kept, "   "));
        assert_eq!(kept, entries(&["queen"]));
    }

    #[test]
    fn only_the_newest_are_kept() {
        let mut kept = Vec::new();
        for at in 0..25 {
            remembered(&mut kept, &format!("query {at}"));
        }
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0], "query 24");
    }

    #[test]
    fn a_record_during_load_merges() {
        let merged = merged(entries(&["old", "queen"]), entries(&["Queen", "new"]));
        assert_eq!(merged, entries(&["Queen", "new", "old"]));
    }

    #[test]
    fn history_round_trips_through_sqlite() {
        let path = std::env::temp_dir().join(format!(
            "sonora-search-history-{}/state.sqlite",
            std::process::id()
        ));
        let store = Store::new(Database::at(path));
        store.save(&entries(&["queen", "abba"])).expect("history saves");
        assert_eq!(store.load().expect("history loads"), entries(&["queen", "abba"]));
    }
}
```

Add `mod search_history;` and `pub use search_history::SearchHistory;` to `crates/state/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p state search_history`
Expected: compile error, `remembered` / `Store` / `KEEP` not found.

- [ ] **Step 3: Implement**

Above the test module in `search_history.rs`:

```rust
use anyhow::{Context as _, Result};
use gpui::{Context, Task};
use rusqlite::{OptionalExtension as _, params};
use storage::Database;

use crate::{Io, join};

const KEEP: usize = 20;
const KEY: &str = "search-history";

/// The queries the user searched for, newest first, kept in the `app_state` table so they
/// survive a restart. Only finished searches are recorded, never every keystroke.
pub struct SearchHistory {
    entries: Vec<String>,
    loaded: bool,
    store: Store,
    io: Io,
    load: Option<Task<()>>,
    save: Option<Task<()>>,
}

impl SearchHistory {
    pub fn new(database: Database, io: Io, cx: &mut Context<Self>) -> Self {
        let store = Store::new(database);
        let reading = store.clone();
        let loading = io.spawn(async move {
            tokio::task::spawn_blocking(move || reading.load()).await?
        });
        let load = cx.spawn(async move |this, cx| {
            let stored = join(loading).await.unwrap_or_else(|error| {
                log::warn!("search: history: {error:#}");
                Vec::new()
            });
            this.update(cx, |this, cx| {
                let fresh = std::mem::take(&mut this.entries);
                let changed = !fresh.is_empty();
                this.entries = merged(stored, fresh);
                this.loaded = true;
                if changed {
                    this.persist(cx);
                }
                cx.notify();
            })
            .ok();
        });

        Self {
            entries: Vec::new(),
            loaded: false,
            store,
            io,
            load: Some(load),
            save: None,
        }
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Puts a finished search at the front.
    pub fn record(&mut self, query: &str, cx: &mut Context<Self>) {
        if remembered(&mut self.entries, query) {
            self.persist(cx);
        }
    }

    pub fn remove(&mut self, query: &str, cx: &mut Context<Self>) {
        let folded = query.to_lowercase();
        self.entries.retain(|entry| entry.to_lowercase() != folded);
        self.persist(cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.persist(cx);
    }

    /// Writes the list once the stored one is in, so a partial list never replaces it.
    fn persist(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if !self.loaded {
            return;
        }
        let store = self.store.clone();
        let entries = self.entries.clone();
        let saving = self.io.spawn(async move {
            tokio::task::spawn_blocking(move || store.save(&entries)).await?
        });
        self.save = Some(cx.spawn(async move |_, _| {
            if let Err(error) = join(saving).await {
                log::warn!("search: history: {error:#}");
            }
        }));
    }
}

/// The `search-history` row of the `app_state` table: the list as one JSON array.
#[derive(Clone)]
struct Store {
    database: Database,
}

impl Store {
    fn new(database: Database) -> Self {
        Self { database }
    }

    fn load(&self) -> Result<Vec<String>> {
        let encoded: Option<String> = self
            .database
            .open()?
            .query_row(
                "SELECT value FROM app_state WHERE key = ?",
                params![KEY],
                |row| row.get(0),
            )
            .optional()
            .context("cannot read search history")?;
        encoded
            .map(|encoded| serde_json::from_str(&encoded).context("cannot decode search history"))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn save(&self, entries: &[String]) -> Result<()> {
        let encoded = serde_json::to_string(entries).context("cannot encode search history")?;
        self.database
            .open()?
            .execute(
                "INSERT INTO app_state (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![KEY, encoded],
            )
            .context("cannot save search history")?;
        Ok(())
    }
}

/// Puts `query` at the front of `entries`, dropping an earlier spelling of it and anything
/// past `KEEP`. Returns false for a blank query, which is never kept.
fn remembered(entries: &mut Vec<String>, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    let folded = query.to_lowercase();
    entries.retain(|entry| entry.to_lowercase() != folded);
    entries.insert(0, query.to_owned());
    entries.truncate(KEEP);
    true
}

/// The stored list with the searches made while it loaded in front of it.
fn merged(stored: Vec<String>, fresh: Vec<String>) -> Vec<String> {
    let mut entries = stored;
    for query in fresh.iter().rev() {
        remembered(&mut entries, query);
    }
    entries
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p state search_history`
Expected: 5 PASS.

- [ ] **Step 5: Wire into `Sonora`**

In `crates/state/src/lib.rs`: add `pub search_history: Entity<SearchHistory>,` to `struct Sonora` (alphabetical, after `scrobbling`). In `init`, before `let usage = …` (which moves `database`):

```rust
    let search_history = cx.new(|cx| SearchHistory::new(database.clone(), io.clone(), cx));
```

and add `search_history,` to the `cx.set_global(Sonora { … })` literal.

- [ ] **Step 6: Check and lint**

Run: `cargo fmt && cargo test -p state && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit (ask first)**

```bash
git add crates/state/src/search_history.rs crates/state/src/lib.rs
git commit -m "feat(state): remember recent searches"
```

---

### Task 5: Tab bar, sort and filter pickers on the search screen

**Files:**
- Modify: `crates/views/src/screens/search.rs`
- Modify: `assets/i18n/en-US/main.ftl` (search block near line 411)

**Interfaces:**
- Consumes: `Search::{lens, set_lens, sources, lanes}`, `Lens::{with_kind, with_source, cycled}`, `Axis::of`, `state::{Axis, Lens}`; `chrome::tools::{self, Sliders}`; `ui::{Button, Filter, FilterChange, FlagAxis, Popovers, SortAxis, TabBar}`.
- Produces: `SearchView::relens(&mut self, impl FnOnce(Lens) -> Lens, cx)`, `SearchView::show(&mut self, Option<Kind>, cx)`, used by Tasks 6–7.

This task is view code with no unit tests; it is verified by `cargo check`, clippy and a manual run.

- [ ] **Step 1: Add the Fluent keys**

In `assets/i18n/en-US/main.ftl` after `search-albums-playlists`:

```ftl
search-all = All
search-albums = Albums
search-playlists = Playlists
search-mine-only = Only in your library
search-filtered-empty = Nothing matches these filters
```

- [ ] **Step 2: Add fields, constants and helpers**

Imports: add `use crate::chrome::tools::{self, Sliders};`, extend `state::{…}` with `Axis, Lens`, extend `ui::{…}` with `Button, Filter, FilterChange, FlagAxis, Popovers, SortAxis, TabBar`.

Constants under `GROUPS`:

```rust
/// The type tabs, in order, with their label key.
const TABS: [(Option<Kind>, &str); 5] = [
    (None, "search-all"),
    (Some(Kind::Song), "search-songs"),
    (Some(Kind::Artist), "search-artists"),
    (Some(Kind::Album), "search-albums"),
    (Some(Kind::Playlist), "search-playlists"),
];
/// The filter key of "Only in your library"; provider filters are keyed by their slug.
const MINE: &str = "mine";
```

Fields on `SearchView`: `popovers: Popovers,` and `sliders: Sliders,`, initialised with `Popovers::default()` and `Sliders::default()`.

Methods in `impl SearchView`:

```rust
    /// Applies a change to the search lens.
    fn relens(&mut self, change: impl FnOnce(Lens) -> Lens, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            let lens = change(search.lens().clone());
            search.set_lens(lens, cx);
        });
    }

    fn show(&mut self, kind: Option<Kind>, cx: &mut Context<Self>) {
        self.relens(|lens| lens.with_kind(kind), cx);
    }

    fn tabs(&self, cx: &Context<Self>) -> TabBar {
        let current = self.search.read(cx).lens().kind;
        TabBar::new("search-kinds")
            .flex_none()
            .items(TABS.iter().map(|&(kind, key)| {
                Button::new(SharedString::from(format!("{key}-tab")))
                    .label(i18n::lookup(key, None))
                    .small()
                    .ghost()
                    .selected(kind == current)
                    .on_click(cx.listener(move |this, _, _, cx| this.show(kind, cx)))
            }))
    }

    fn sorts(&self, cx: &Context<Self>) -> AnyElement {
        let lens = self.search.read(cx).lens();
        let axes = Axis::of(lens.kind)
            .iter()
            .map(|&axis| SortAxis {
                key: axis_key(axis),
                label: i18n::lookup(axis_label(axis), None),
                order: lens
                    .order
                    .filter(|(current, _)| *current == axis)
                    .map(|(_, sort)| sort),
            })
            .collect();
        let me = cx.entity().downgrade();
        tools::sorts(
            &self.popovers,
            axes,
            move |key, cx| {
                let Some(axis) = axis_of(key) else {
                    return;
                };
                me.update(cx, |this, cx| this.relens(|lens| lens.cycled(axis), cx))
                    .ok();
            },
            cx,
        )
    }

    fn filters(&self, cx: &Context<Self>) -> AnyElement {
        let search = self.search.read(cx);
        let lens = search.lens();
        let session = Sonora::global(cx).session.read(cx);
        let mut axes: Vec<Filter> = search
            .sources(cx)
            .into_iter()
            .map(|slug| {
                Filter::Flag(FlagAxis {
                    key: slug,
                    label: SharedString::from(session.provider_name_for(slug).unwrap_or(slug)),
                    on: !lens.hidden.contains(&slug),
                })
            })
            .collect();
        axes.push(Filter::Flag(FlagAxis {
            key: MINE,
            label: t!("search-mine-only"),
            on: lens.mine_only,
        }));
        let me = cx.entity().downgrade();
        tools::filters(
            &self.popovers,
            &self.sliders,
            axes,
            move |change, cx| {
                me.update(cx, |this, cx| {
                    this.relens(
                        |lens| match change {
                            FilterChange::Flag(MINE, on) => Lens {
                                mine_only: on,
                                ..lens
                            },
                            FilterChange::Flag(slug, on) => lens.with_source(slug, on),
                            FilterChange::Reset => Lens::default(),
                            FilterChange::Range(..) => lens,
                        },
                        cx,
                    )
                })
                .ok();
            },
            cx,
        )
    }

    /// What a lane or the mixed list shows when nothing is left: a plain "No matches", or,
    /// when the lens narrowed it, why, with a way back.
    fn empty(&self, cx: &Context<Self>) -> AnyElement {
        match *self.search.read(cx).lens() == Lens::default() {
            true => vacant(t!("search-no-matches"), cx).flex_none().into_any_element(),
            false => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(vacant(t!("search-filtered-empty"), cx).flex_none())
                .child(
                    Button::new("search-filters-reset")
                        .label(t!("filter-reset"))
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.relens(|_| Lens::default(), cx);
                        })),
                )
                .into_any_element(),
        }
    }
```

Free helpers at the bottom (above `fn after`):

```rust
fn axis_key(axis: Axis) -> &'static str {
    match axis {
        Axis::Title => "title",
        Axis::Artist => "artist",
        Axis::Duration => "duration",
        Axis::Year => "year",
    }
}

fn axis_label(axis: Axis) -> &'static str {
    match axis {
        Axis::Title => "column-title",
        Axis::Artist => "column-artist",
        Axis::Duration => "column-length",
        Axis::Year => "column-year",
    }
}

fn axis_of(key: &str) -> Option<Axis> {
    [Axis::Title, Axis::Artist, Axis::Duration, Axis::Year]
        .into_iter()
        .find(|axis| axis_key(*axis) == key)
}
```

Note on the filter icon tint: `tools::filters` tints the funnel when any `Flag` is on, so it stays tinted while sources are on. Accept this for now and list it in the final report. Do not change `ui::filters` in this task.

- [ ] **Step 3: One flat list per lane when one kind is shown**

In `seats`, at the top after `let hits = search.hits();`:

```rust
        let single = search.lens().kind.is_some();
```

In the `lane == None` branch use `compact: !single`. After the `let Some(lane) = lane else {…};`, add:

```rust
        if single {
            return hits
                .iter()
                .enumerate()
                .filter(|(_, found)| found.slug == lane)
                .map(|(at, _)| Seat::Item { at, compact: false })
                .collect();
        }
```

In `deck`, replace `let empty = vacant(t!("search-no-matches"), cx).flex_none();` with `let empty = self.empty(cx);` (both arms already accept an `AnyElement`; adjust `.into_any_element()` calls accordingly).

- [ ] **Step 4: Lay out the toolbar and tabs**

In `render`, replace the input child with:

```rust
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .px(gutter)
                    .child(div().flex_1().min_w_0().child(self.input.clone()))
                    .when(asked, |this| this.child(self.sorts(cx)).child(self.filters(cx))),
            )
            .when(asked, |this| this.child(div().flex_none().px(gutter).child(self.tabs(cx))))
```

When every source is hidden there are no lanes: in the wide branch of `render`, when `lanes.is_empty()`, render `div().flex_1().px(gutter).child(self.empty(cx))` instead of the empty row.

- [ ] **Step 5: Check and lint**

Run: `cargo fmt && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Manual check**

Run: `cargo run --locked --package sonora`, then search "queen" and check each of these:
- the tabs switch kinds without a loading flash when narrowing
- Albums shows more than 20 per column when the provider sends more
- the sort menu cycles ↑ ↓ off
- a hidden provider's column disappears, and turning it back on refetches
- "Only in your library" leaves only saved items
- hiding every source shows "Nothing matches these filters" with Reset
- Best match is gone while sorted or on a single tab

- [ ] **Step 7: Commit (ask first)**

```bash
git add crates/views/src/screens/search.rs assets/i18n/en-US/main.ftl
git commit -m "feat(views): filter and sort search results"
```

---

### Task 6: Recent searches on the empty search screen

**Files:**
- Modify: `crates/views/src/screens/search.rs`
- Modify: `crates/input/src/lib.rs` (Enter in the search field)
- Modify: `assets/i18n/en-US/main.ftl`

**Interfaces:**
- Consumes: `Sonora::search_history`, `SearchHistory::{entries, record, remove, clear}`, `ui::Submit`.
- Produces: `SearchView::remember(&self, &mut App)`.

- [ ] **Step 1: Fluent keys**

```ftl
search-recent = Recent searches
search-recent-clear = Clear
search-recent-remove = Remove from recent searches
```

- [ ] **Step 2: Enter in the search field submits**

In `crates/input/src/lib.rs` `shared()`, add `let typing = format!("{SEARCH_CONTEXT} && {INPUT_CONTEXT}");` next to `results`, and in the vec after the `results` bindings:

```rust
        KeyBinding::new("enter", Submit, Some(&typing)),
```

- [ ] **Step 3: Record finished searches**

In `SearchView`: field `history: Entity<SearchHistory>` from `Sonora::global(cx).search_history.clone()` in `new`, with `cx.observe(&history, |_, _, cx| cx.notify()).detach();`. Add `SearchHistory` to the `state::{…}` import and `Submit` to `ui::{…}`.

```rust
    /// Keeps the current query in the recent searches, once the user acted on it.
    fn remember(&self, cx: &mut App) {
        let query = self.search.read(cx).query().to_owned();
        self.history.update(cx, |history, cx| history.record(&query, cx));
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.remember(cx);
    }
```

Call `this.remember(cx);` / `self.remember(cx);` at:
- the start of `activate_hit`, after the hit is found
- inside `pressed`'s `me.update` closure, before the `match`
- inside both `Play` closures in `transport`, before `this.playback.update`

Register the action in `render`: `.on_action(cx.listener(Self::submit))`.

- [ ] **Step 4: Show the section**

```rust
    /// The recent searches above Browse all, or nothing when there are none.
    fn recent(&self, gutter: Pixels, cx: &Context<Self>) -> Option<AnyElement> {
        let entries = self.history.read(cx).entries();
        if entries.is_empty() {
            return None;
        }
        let rows = entries.iter().enumerate().map(|(at, query)| {
            let asked = query.clone();
            let gone = query.clone();
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new(("search-recent", at))
                        .label(SharedString::from(query.clone()))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.fill(&asked, window, cx);
                        })),
                )
                .child(
                    Button::new(("search-recent-remove", at))
                        .icon("icons/x.svg")
                        .small()
                        .ghost()
                        .tooltip("search-recent-remove")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.history.update(cx, |history, cx| history.remove(&gone, cx));
                        })),
                )
        });

        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1()
                .px(gutter)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(eyebrow(t!("search-recent"), cx))
                        .child(
                            Button::new("search-recent-clear")
                                .label(t!("search-recent-clear"))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history.update(cx, |history, cx| history.clear(cx));
                                })),
                        ),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// Searches for a recent query again.
    fn fill(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_owned();
        self.input.update(cx, |input, cx| {
            input.set_text(query.clone(), cx);
            input.focus(window, cx);
        });
        self.history.update(cx, |history, cx| history.record(&query, cx));
    }
```

In `browse`, add `.children(self.recent(gutter, cx))` as the first child, before the `search-browse` eyebrow.

- [ ] **Step 5: Check and lint**

Run: `cargo fmt && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Manual check**

Run the app and check each of these:
- typing "queen" letter by letter records nothing
- Enter records "queen"
- playing a hit records its query
- an empty field shows "Recent searches", and a click fills and searches
- ✕ removes one entry, Clear empties the list
- the list survives a restart

- [ ] **Step 7: Commit (ask first)**

```bash
git add crates/views/src/screens/search.rs crates/input/src/lib.rs assets/i18n/en-US/main.ftl
git commit -m "feat(views): show recent searches on the empty search screen"
```

---

### Task 7: alt-1 … alt-5 switch the search tab

**Files:**
- Modify: `crates/input/src/lib.rs`
- Modify: `crates/views/src/screens/search.rs`

**Interfaces:**
- Consumes: `SearchView::show` (Task 5).
- Produces: actions `input::{SearchAll, SearchSongs, SearchArtists, SearchAlbums, SearchPlaylists}`.

- [ ] **Step 1: Actions and bindings**

Add `SearchAll, SearchSongs, SearchArtists, SearchAlbums, SearchPlaylists` to the `actions!(sonora, [...])` list. In `shared()` after the search bindings:

```rust
        KeyBinding::new("alt-1", SearchAll, search),
        KeyBinding::new("alt-2", SearchSongs, search),
        KeyBinding::new("alt-3", SearchArtists, search),
        KeyBinding::new("alt-4", SearchAlbums, search),
        KeyBinding::new("alt-5", SearchPlaylists, search),
```

- [ ] **Step 2: Handlers**

Import the five actions from `input` in `search.rs` and register them in `render`:

```rust
            .on_action(cx.listener(|this, _: &SearchAll, _, cx| this.show(None, cx)))
            .on_action(cx.listener(|this, _: &SearchSongs, _, cx| this.show(Some(Kind::Song), cx)))
            .on_action(cx.listener(|this, _: &SearchArtists, _, cx| this.show(Some(Kind::Artist), cx)))
            .on_action(cx.listener(|this, _: &SearchAlbums, _, cx| this.show(Some(Kind::Album), cx)))
            .on_action(cx.listener(|this, _: &SearchPlaylists, _, cx| this.show(Some(Kind::Playlist), cx)))
```

- [ ] **Step 3: Check, lint, manual check**

Run: `cargo fmt && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings`. Then in the app, with the field focused and with a result selected, press alt-1 … alt-5. Each should switch tabs and type no character into the field.

- [ ] **Step 4: Commit (ask first)**

```bash
git add crates/input/src/lib.rs crates/views/src/screens/search.rs
git commit -m "feat(views): switch search tabs with alt and a digit"
```

---

### Task 8: Changelog, architecture notes and full verification

**Files:**
- Modify: `CHANGELOG.md` (`## [Unreleased]` → `### Added`)
- Modify: `docs/architecture.md` (search section; add one if none exists, next to the Toolbar/`Searchable` notes around line 926)

- [ ] **Step 1: Changelog**

```markdown
- Search can now be narrowed to songs, artists, albums or playlists, limited to chosen
  providers or your library, and sorted by title, artist, duration or year.
- Recent searches show up when the search field is empty.
```

- [ ] **Step 2: Architecture note**

A short paragraph on the search screen:
- `state::Search` holds a `Lens`.
- `Parts`/`Served` decide whether a lens change refetches or only reranks.
- `narrowed` applies kind, library and order after ranking, and hidden providers leave through `lanes`.
- `state::SearchHistory` keeps 20 queries in `app_state` under `search-history` and records only acted-on searches.

- [ ] **Step 3: Full verification**

Run: `cargo fmt --check && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all PASS, clippy clean.

- [ ] **Step 4: Commit (ask first)**

```bash
git add CHANGELOG.md docs/architecture.md
git commit -m "docs: describe search filters and recent searches"
```
