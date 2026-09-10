# Several Providers Connected At Once — Implementation Plan (part 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep every provider a user has signed into connected at the same time, each with its own sidebar section, and let one queue hold tracks from any mix of them.

**Architecture:** Every id carries its provider as a `<slug>:` prefix, the way `local:` already does. A `Tagged` wrapper applies and strips that prefix at the `MusicApi` boundary so no provider module changes. `Session`, `Playback` and `Library` replace their hardcoded streaming/local pairs with maps keyed by slug.

**Tech Stack:** Rust 1.97.1 (pinned in `rust-toolchain.toml`), edition 2024, GPUI, tokio via `state::Io`, `anyhow`.

**Spec:** `docs/superpowers/specs/2026-09-07-multi-provider-design.md`

**Scope:** This plan covers the spec through "Sidebar, router and settings" plus "Migration". Merged Home and Search ("Home and Search merged") is part 2, a separate plan, and depends on this one.

## Global Constraints

- Branch is `feat/multi-provider`, cut from `feat/soundcloud-provider`. This work is for this fork only and is never offered upstream.
- Never run `git push`. Ask the user before every commit; the commit commands in this plan are proposals, not authorisations.
- Commit messages: Conventional Commits, English, imperative, lowercase, no trailing period, **no body**, and never a `Co-Authored-By` trailer or any assistant attribution.
- Never hardcode a colour, radius or size; read from `cx.theme()`. Never write a bare English literal in the UI; add a Fluent key to `assets/i18n/en-US/main.ftl`.
- Network work runs on the tokio runtime (`state::Io`), never on GPUI's executor. Store the returned `Task` in a field; never `.detach()` a data load.
- `music` must never depend on `gpui`. Dependency direction stays `sonora → views → state → music`.
- The repo's rule is "do not add tests unless asked". The tests in this plan are the ones the approved spec's Testing section names, and no others: tag parsing, the sift predicate, and the two migrations. Do not extend a neighbouring `mod tests` beyond what a task specifies.
- Tests are `#[cfg(test)] mod tests` at the bottom of the file they cover. There is no UI or network harness.
- After each task: `cargo fmt --all -- --check` clean, `cargo clippy --workspace --all-targets` introduces no new warning. One warning already exists on `main` (`crates/music/src/spectrum.rs:76`, `useless_vec`); leave it.
- The slug strings are exactly `"spotify"`, `"youtube"`, `"soundcloud"`, `"local"` — `MusicProvider::slug()` is the source of truth.

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/music/src/tag.rs` (new) | Pure tag/untag helpers. No I/O, no async. The only place the `<slug>:<id>` grammar is known. |
| `crates/music/src/tagged/mod.rs` (new) | `Tagged`: a `MusicApi` that untags ids going in and tags ids in models coming out. |
| `crates/music/src/tagged/models.rs` (new) | The per-model walkers `Tagged` uses. Split out because fifteen types nest into each other and one file would be unwieldy. |
| `crates/music/src/lib.rs` | Re-exports `tag`, `tagged`; `is_local_id` reimplemented on top of `tag`; new `MusicApi::has_all_tracks`. |
| `crates/router/src/uri.rs` | Tags the ids it produces from Spotify URIs. |
| `crates/router/src/lib.rs` | `Destination::Library(slug, Tab)`; `NavEntry` list built at runtime. |
| `crates/state/src/session.rs` | `Connected` struct and the `connected` map; per-slug `SessionEvent`. |
| `crates/state/src/playback.rs` | `engines` map; selective teardown. |
| `crates/state/src/queue.rs` | Selective purge via the existing `sift`. |
| `crates/state/src/library.rs` | `shelves` map and the `Shelf` struct. |
| `crates/state/src/settings.rs` | Per-slug layout keys and the two load-time migrations. |
| `crates/views/src/root.rs` | One `LibraryView` per connected provider. |
| `crates/views/src/screens/library/mod.rs` | Tab set derived from `has_all_tracks` instead of `Shelf`. |
| `crates/views/src/chrome/sidebar_left.rs` | One library group per connected provider. |

---

### Task 1: The tag grammar

The one place that knows `<slug>:<id>`. Everything else calls these.

**Files:**
- Create: `crates/music/src/tag.rs`
- Modify: `crates/music/src/lib.rs` (add `pub mod tag;`, reimplement `is_local_id`)

**Interfaces:**
- Produces:
  - `pub fn tag(slug: &str, id: &str) -> String`
  - `pub fn split(id: &str) -> Option<(&str, &str)>` — `None` when the id carries no tag
  - `pub fn slug_of(id: &str) -> Option<&str>`
  - `pub fn untag(id: &str) -> &str` — the bare id, or the input unchanged when untagged
  - `pub const SLUGS: [&str; 4] = ["spotify", "youtube", "soundcloud", "local"]`
- Consumed by: every later task.

- [ ] **Step 1: Write the failing tests**

Create `crates/music/src/tag.rs` with only the test module and empty function bodies that `todo!()`:

```rust
#[cfg(test)]
mod tests {
    use super::{slug_of, split, tag, untag};

    #[test]
    fn tags_and_splits_a_plain_id() {
        assert_eq!(tag("soundcloud", "284873455"), "soundcloud:284873455");
        assert_eq!(split("soundcloud:284873455"), Some(("soundcloud", "284873455")));
    }

    #[test]
    fn an_untagged_id_has_no_slug() {
        assert_eq!(split("284873455"), None);
        assert_eq!(slug_of("284873455"), None);
        assert_eq!(untag("284873455"), "284873455");
    }

    #[test]
    fn an_unknown_head_is_not_a_tag() {
        assert_eq!(split("track:abc"), None);
        assert_eq!(untag("track:abc"), "track:abc");
    }

    // A local id is a file path, so the tail carries both ':' and '/'.
    #[test]
    fn a_local_path_keeps_every_separator_after_the_first() {
        let id = "local:/home/j/My Music/a:b/x.mp3";
        assert_eq!(split(id), Some(("local", "/home/j/My Music/a:b/x.mp3")));
        assert_eq!(untag(id), "/home/j/My Music/a:b/x.mp3");
    }

    // The hazard from the spec: `spotify:` is also a live URI scheme.
    #[test]
    fn a_spotify_deep_link_is_not_mistaken_for_a_tagged_id() {
        assert_eq!(split("spotify:track:6rqhFgbbKwnb9MLmUQDhG6"), None);
        assert_eq!(split("spotify:album:1DFixLWuPkv3KT3TnV35m3"), None);
        assert_eq!(split("spotify:user:x:playlist:y"), None);
    }

    #[test]
    fn a_tagged_spotify_id_still_splits() {
        assert_eq!(
            split("spotify:7etD5lFGaYcsKmFTmutVYO"),
            Some(("spotify", "7etD5lFGaYcsKmFTmutVYO"))
        );
    }

    #[test]
    fn tagging_is_idempotent() {
        let once = tag("spotify", "abc");
        assert_eq!(tag("spotify", &once), once);
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p music --lib tag::`
Expected: every test panics at `todo!()`.

- [ ] **Step 3: Implement**

```rust
//! The `<slug>:<id>` grammar every id in the app carries.
//!
//! `spotify:` is also the URI scheme sonora accepts for deep links
//! (`router::uri`), so a head is only a tag when the tail is not itself a
//! Spotify URI body. `URI_KINDS` is that guard.

pub const SLUGS: [&str; 4] = ["spotify", "youtube", "soundcloud", "local"];

const URI_KINDS: [&str; 5] = ["track", "album", "playlist", "artist", "user"];

pub fn tag(slug: &str, id: &str) -> String {
    match split(id) {
        Some(_) => id.to_owned(),
        None => format!("{slug}:{id}"),
    }
}

pub fn split(id: &str) -> Option<(&str, &str)> {
    let (head, rest) = id.split_once(':')?;
    if !SLUGS.contains(&head) {
        return None;
    }
    let kind = rest.split_once(':').map_or(rest, |(kind, _)| kind);
    if URI_KINDS.contains(&kind) {
        return None;
    }
    Some((head, rest))
}

pub fn slug_of(id: &str) -> Option<&str> {
    split(id).map(|(slug, _)| slug)
}

pub fn untag(id: &str) -> &str {
    split(id).map_or(id, |(_, rest)| rest)
}
```

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p music --lib tag::`
Expected: 7 passed, output pristine.

- [ ] **Step 5: Reimplement `is_local_id` on top of it**

In `crates/music/src/lib.rs`, add `pub mod tag;` in module order (alphabetical: after `spotify`, before `youtube` — check `cargo fmt` agrees) and replace the body:

```rust
pub fn is_local_id(id: &str) -> bool {
    tag::slug_of(id) == Some("local")
}
```

Leave `LOCAL_TRACK_PREFIX` and its siblings as they are: `music::local::wire` still builds ids with them, and `local:` is exactly what `tag` produces for that slug.

- [ ] **Step 6: Verify nothing regressed**

Run: `cargo test --workspace`
Expected: the pre-existing 346 pass, plus the 7 new ones.

- [ ] **Step 7: Commit**

```bash
git add crates/music/src/tag.rs crates/music/src/lib.rs
git commit -m "feat(music): add the provider tag grammar"
```

---

### Task 2: Deep links produce tagged ids

The spec's obligation: `uri::destination` is the only place a Spotify URI becomes an id, so it tags there and the two vocabularies never meet.

**Files:**
- Modify: `crates/router/src/uri.rs`
- Modify: `crates/router/Cargo.toml` (add `music.workspace = true` if absent)

**Interfaces:**
- Consumes: `music::tag::tag` from Task 1.
- Produces: nothing new; `destination` keeps its signature.

- [ ] **Step 1: Write the failing test**

The file already has tests of the form `destination("spotify:track:…")`. Add to that module:

```rust
#[test]
fn a_link_produces_a_tagged_id() {
    assert_eq!(
        destination("spotify:track:6rqhFgbbKwnb9MLmUQDhG6"),
        Some(Destination::Song("spotify:6rqhFgbbKwnb9MLmUQDhG6".into()))
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p router --lib uri::`
Expected: FAIL — the id comes back bare.

- [ ] **Step 3: Implement**

`route` is the one function that builds a `Destination` from a `kind` and an `id`. Tag there, once:

```rust
fn route(kind: &str, id: &str) -> Option<Destination> {
    let id = SharedString::from(music::tag::tag("spotify", id));
    match kind {
        // arms unchanged, each using `id.clone()` as before
    }
}
```

Check the existing arms: if any already clones `id`, leave that alone. Do not change `from_url`'s parsing — it feeds the same `route`.

- [ ] **Step 4: Run the whole uri suite**

Run: `cargo test -p router --lib uri::`
Expected: PASS, including the pre-existing link tests (update their expected ids to the tagged form in the same commit — they assert on `Destination`, so each needs the `spotify:` prefix added).

- [ ] **Step 5: Commit**

```bash
git add crates/router/src/uri.rs crates/router/Cargo.toml
git commit -m "feat(router): tag the ids a spotify link resolves to"
```

---

### Task 3: The model walkers

Fifteen model types carry an id and they nest, so the walkers live in their own file. This task is mechanical; the risk is missing a field, so the checklist below is the deliverable.

**Files:**
- Create: `crates/music/src/tagged/models.rs`
- Create: `crates/music/src/tagged/mod.rs` (module declaration only in this task)

**Interfaces:**
- Consumes: `music::tag::tag`.
- Produces, one per type, all `pub(crate)`:
  - `fn track(slug: &str, value: &mut Track)`
  - `fn album(slug: &str, value: &mut Album)`
  - `fn album_detail(slug: &str, value: &mut AlbumDetail)`
  - `fn playlist(slug: &str, value: &mut Playlist)`
  - `fn playlist_detail(slug: &str, value: &mut PlaylistDetail)`
  - `fn artist_ref(slug: &str, value: &mut ArtistRef)`
  - `fn artist_profile(slug: &str, value: &mut ArtistProfile)`
  - `fn saved_artist(slug: &str, value: &mut SavedArtist)`
  - `fn contributor(slug: &str, value: &mut Contributor)`
  - `fn credit(slug: &str, value: &mut Credit)`
  - `fn genre(slug: &str, value: &mut Genre)`
  - `fn user_profile(slug: &str, value: &mut UserProfile)`
  - `fn user_detail(slug: &str, value: &mut UserDetail)`
  - `fn home_feed(slug: &str, value: &mut HomeFeed)`
  - `fn genre_section(slug: &str, value: &mut GenreSection)`

- [ ] **Step 1: Enumerate the fields before writing anything**

Run this and paste the output into your report — it is the checklist the reviewer will hold you to:

```bash
grep -n "pub struct" crates/music/src/models.rs | while read -r line; do
  n=${line%%:*}; name=$(echo "$line" | sed 's/.*pub struct //;s/ .*//')
  echo "--- $name"
  sed -n "${n},$((n+30))p" crates/music/src/models.rs | grep -n "pub .*id\|Vec<\|Option<Arc<"
done
```

- [ ] **Step 2: Write `track`, the one with every shape in it**

`Track` is the hard case: a direct id, a foreign id, a `Vec` of id-bearing structs, an `Arc`, and another `Vec`.

```rust
pub(crate) fn track(slug: &str, value: &mut Track) {
    if let Some(id) = value.id.as_mut() {
        *id = tag::tag(slug, id);
    }
    if let Some(id) = value.album_id.as_mut() {
        *id = tag::tag(slug, id);
    }
    for reference in &mut value.artist_refs {
        artist_ref(slug, reference);
    }
    if let Some(contributor) = value.added_by.as_mut() {
        self::contributor(slug, Arc::make_mut(contributor));
    }
    for credit in &mut value.credits {
        self::credit(slug, credit);
    }
}
```

`added_by` is `Option<Arc<Contributor>>`; `Arc::make_mut` clones only when the `Arc` is shared, which is what we want.

- [ ] **Step 3: Write the remaining fourteen**

Same shape throughout: tag every `id`-suffixed `Option<String>`/`String` field, recurse into every nested model. Two that are easy to miss:

```rust
pub(crate) fn album_detail(slug: &str, value: &mut AlbumDetail) {
    album(slug, &mut value.album);
    for entry in &mut value.tracks {
        track(slug, entry);
    }
}

pub(crate) fn home_feed(slug: &str, value: &mut HomeFeed) {
    for entry in &mut value.listen_again {
        track(slug, entry);
    }
    for entry in value.quick_picks.iter_mut().flatten() {
        track(slug, entry);
    }
    for section in &mut value.sections {
        genre_section(slug, section);
    }
}
```

Adapt the field names to what Step 1 printed — do not guess them.

- [ ] **Step 4: Verify by compiling**

Run: `cargo check -p music`
Expected: clean. There is no test here: these are covered through Task 4.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/tagged/
git commit -m "feat(music): walk every model that carries an id"
```

---

### Task 4: The `Tagged` client

**Files:**
- Modify: `crates/music/src/tagged/mod.rs`
- Modify: `crates/music/src/lib.rs` (add `pub mod tagged;`)

**Interfaces:**
- Consumes: `crate::tagged::models` from Task 3; `music::tag::untag`.
- Produces:
  - `pub struct Tagged { slug: &'static str, inner: Arc<dyn MusicApi> }`
  - `pub fn new(slug: &'static str, inner: Arc<dyn MusicApi>) -> Arc<dyn MusicApi>`

- [ ] **Step 1: Write the struct and the rule**

```rust
/// A `MusicApi` that speaks tagged ids to the app and bare ids to the
/// provider underneath.
///
/// Every method follows the same two steps: `untag` each id argument on the
/// way in, walk each returned model on the way out. A method that carries no
/// id in either direction delegates unchanged.
pub struct Tagged {
    slug: &'static str,
    inner: Arc<dyn MusicApi>,
}

pub fn new(slug: &'static str, inner: Arc<dyn MusicApi>) -> Arc<dyn MusicApi> {
    Arc::new(Tagged { slug, inner })
}
```

- [ ] **Step 2: Implement the three representative shapes**

```rust
#[async_trait]
impl MusicApi for Tagged {
    // in and out
    async fn track(&self, track_id: &str) -> Result<Track> {
        let mut value = self.inner.track(tag::untag(track_id)).await?;
        models::track(self.slug, &mut value);
        Ok(value)
    }

    // out only
    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let mut values = self.inner.saved_tracks(limit).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    // in only
    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        self.inner.set_track_saved(tag::untag(track_id), saved).await
    }
}
```

- [ ] **Step 3: Work through all 43 methods**

Generate the checklist and tick every line in your report:

```bash
grep -n "async fn " crates/music/src/lib.rs | sed -n '/trait MusicApi/,/^}/p'
grep -n "    async fn " crates/music/src/lib.rs
```

26 take an id argument. Every one of those untags. Any method returning a model or a collection of models walks it. A method that does neither — a `bool`, a `u32`, a `()` with no id — is a plain delegation.

Do not use the trait's default bodies: `Tagged` must forward every method explicitly, or a provider's own implementation is silently replaced by the default. This is the one mistake in this task that produces no error.

- [ ] **Step 4: Verify**

Run: `cargo check -p music && cargo clippy -p music --all-targets`
Expected: clean, no new warnings.

Then confirm nothing was left to a default body:

```bash
diff <(grep -o 'async fn [a-z_]*' crates/music/src/lib.rs | sort -u) \
     <(grep -o 'async fn [a-z_]*' crates/music/src/tagged/mod.rs | sort -u)
```

Expected: only the `LyricsProvider::search` line differs (it belongs to a different trait). Paste this output into your report.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/tagged/mod.rs crates/music/src/lib.rs
git commit -m "feat(music): wrap a provider so its ids carry its slug"
```

---

### Task 5: `Session` keeps a map

**Files:**
- Modify: `crates/state/src/session.rs`

**Interfaces:**
- Consumes: `music::tagged::new` from Task 4; `music::tag::slug_of`.
- Produces:
  - `pub struct Connected { pub client: Arc<dyn MusicApi>, pub catalog: Arc<CatalogSource>, pub playback: Arc<dyn PlaybackFactory>, pub profile: UserProfile, pub authenticated: bool }`
  - `pub fn connected(&self, slug: &str) -> Option<&Connected>`
  - `pub fn client_for_slug(&self, slug: &str) -> Option<Arc<dyn MusicApi>>`
  - `active_slugs` and `slug_for` keep their signatures.

- [ ] **Step 1: Replace the paired fields**

Delete `client`, `catalog`, `playback`, `authenticated`, `local_client`, `local_catalog`, `local_playback` and `local_task`. Add:

```rust
connected: HashMap<&'static str, Connected>,
```

Keep `providers: Vec<Arc<dyn MusicProvider>>` and `local_provider` exactly as they are. `LocalProvider::sign_in_options()` returns an empty `Vec`, and `Session::providers()` is what the login screen turns into tabs — folding local into that list gives it an empty tab.

- [ ] **Step 2: Wrap on the way in**

Wherever a `ProviderSession` lands (`signed_in`, `local_signed_in`, the restore path), wrap before storing:

```rust
let client = music::tagged::new(slug, session.api);
self.connected.insert(slug, Connected {
    catalog: Arc::new(CatalogSource::new(client.clone())),
    client,
    playback: session.playback,
    profile,
    authenticated: session.authenticated,
});
```

`CatalogSource` must be built from the wrapped client, not the raw one, or the catalog caches bare ids.

- [ ] **Step 3: Rewrite the three accessors**

```rust
pub fn active_slugs(&self) -> Vec<&'static str> {
    let mut slugs: Vec<&'static str> = self.connected.keys().copied().collect();
    slugs.sort_unstable_by_key(|slug| self.order_of(slug));
    slugs
}

pub fn slug_for(&self, id: &str) -> Option<&'static str> {
    let slug = music::tag::slug_of(id)?;
    self.connected.keys().copied().find(|known| *known == slug)
}
```

`active_slugs` must be stable across frames or the sidebar reorders itself every render. `order_of` returns the provider's index in `self.providers`, and `usize::MAX` for local so it sorts last — matching where Local Music sits in the sidebar today.

- [ ] **Step 4: Delete what the map replaced**

Remove `local_slug`, `local_client`, `local_playback`, and fold `clear_local_folder` into the same path a sign-out takes: `self.connected.remove("local")`.

- [ ] **Step 5: Verify**

Run: `cargo check -p state`
Expected: errors only at call sites the later tasks own (`playback.rs`, `library.rs`). List them in your report; do not fix them here.

- [ ] **Step 6: Commit**

```bash
git add crates/state/src/session.rs
git commit -m "refactor(state): keep every connected provider in one map"
```

---

### Task 6: Events name their provider

**Files:**
- Modify: `crates/state/src/session.rs`
- Modify: `crates/state/src/{artist,cover,detail,genre,history,home,library,playback,profile,queue,search,song,usage}.rs`

**Interfaces:**
- Produces: `SessionEvent::{SignedIn(&'static str), SignedOut(&'static str), Reconnected(&'static str)}`. `LocalChanged` is removed.

- [ ] **Step 1: Change the enum**

```rust
pub enum SessionEvent {
    SignedIn(&'static str),
    SignedOut(&'static str),
    Reconnected(&'static str),
}
```

- [ ] **Step 2: Update the ten emit sites**

Run `grep -n "cx.emit(SessionEvent" crates/state/src/session.rs` — there are ten. Each now passes the slug it acted on. The two that emitted `LocalChanged` emit `SignedIn("local")` and `SignedOut("local")`.

- [ ] **Step 3: Work through the thirteen subscribers**

Run `grep -rn "SessionEvent::" crates/state/src/*.rs` and handle every arm. Nine arms currently handle `LocalChanged` by doing nothing; eight of those are combined arms such as `SignedIn | Reconnected | LocalChanged => {}` which stay and simply lose the variant. One (`history.rs:201`) stands alone and is deleted.

For each remaining arm, decide whether it is still right when only one provider changed. The default for a screen that shows one provider's data is: act only when `slug` is the provider it is showing. `queue.rs` and `playback.rs` are handled in Task 8; leave their arms compiling but unchanged here and say so in your report.

- [ ] **Step 4: Verify**

Run: `cargo check --workspace`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/state/src/
git commit -m "refactor(state): name the provider in every session event"
```

---

### Task 7: A provider says whether it has more than favourites

**Files:**
- Modify: `crates/music/src/lib.rs`
- Modify: `crates/music/src/local/client.rs`

**Interfaces:**
- Produces: `fn has_all_tracks(&self) -> bool { false }` on `MusicApi`; `true` on the local client.

- [ ] **Step 1: Add the capability**

In the `MusicApi` trait, beside `all_tracks`:

```rust
/// Whether `all_tracks` answers with something other than `saved_tracks`.
///
/// The local provider scans a folder, so it has both a Songs tab holding
/// everything and a separate Favorites tab. A streaming provider's Songs
/// tab *is* its favourites, so it shows one tab fewer.
fn has_all_tracks(&self) -> bool {
    false
}
```

- [ ] **Step 2: Override it for local**

In the local client's `impl MusicApi`, next to its `all_tracks`:

```rust
fn has_all_tracks(&self) -> bool {
    true
}
```

- [ ] **Step 3: Forward it from `Tagged`**

In `crates/music/src/tagged/mod.rs`:

```rust
fn has_all_tracks(&self) -> bool {
    self.inner.has_all_tracks()
}
```

Missing this is the silent-default mistake Task 4 warns about: local would lose its Songs tab and nothing would report an error.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace`
Expected: unchanged pass count.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/lib.rs crates/music/src/local/client.rs crates/music/src/tagged/mod.rs
git commit -m "feat(music): let a provider declare it has more than favourites"
```

---

### Task 8: One engine per provider, and a selective sign-out

**Files:**
- Modify: `crates/state/src/playback.rs`
- Modify: `crates/state/src/queue.rs`

**Interfaces:**
- Consumes: `Session::connected`, `SessionEvent::SignedOut(slug)`.
- Produces: `Playback::teardown(&mut self, slug: &str, cx)`; `Queue::purge(&mut self, slug: &str, cx)`.

- [ ] **Step 1: Write the failing test for the queue predicate**

`crates/state/src/queue.rs` already has `signing_out_leaves_only_imported_tracks`. Add beside it:

```rust
#[test]
fn signing_out_of_one_provider_leaves_the_others() {
    let mut past = vec![track("spotify:a"), track("soundcloud:b")];
    let mut current = Some(track("spotify:c"));
    let mut upcoming = VecDeque::from([track("soundcloud:d")]);
    let mut source = vec![track("spotify:a"), track("soundcloud:b")];

    assert!(sift(&mut past, &mut current, &mut upcoming, &mut source, |t| {
        t.id.as_deref().and_then(music::tag::slug_of) != Some("spotify")
    }));

    assert_eq!(past.len(), 1);
    assert!(current.is_none());
    assert_eq!(upcoming.len(), 1);
    assert_eq!(source.len(), 1);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p state --lib queue::`
Expected: FAIL — `music::tag` is not yet imported in this file.

- [ ] **Step 3: Make it pass**

Add the import. `sift` itself does not change — it is already generic over the predicate.

- [ ] **Step 4: Give `purge` the slug**

```rust
fn purge(&mut self, slug: &str, cx: &mut Context<Self>) {
    let suggested = self.similar > 0;
    self.upcoming.truncate(self.queued());
    self.similar = 0;
    let sifted = sift(
        &mut self.past,
        &mut self.current,
        &mut self.upcoming,
        &mut self.source,
        |track| track.id.as_deref().and_then(music::tag::slug_of) != Some(slug),
    );
    if suggested || sifted {
        self.changed(cx);
    }
}
```

Delete the free function `local` at `queue.rs:125` — nothing calls it any more.

- [ ] **Step 5: Turn the two engines into a map**

In `playback.rs`, replace `engine` and `local_engine` with:

```rust
engines: HashMap<&'static str, Box<dyn Player>>,
```

```rust
fn engine_for(&self, id: &str) -> Option<&dyn Player> {
    let slug = music::tag::slug_of(id)?;
    self.engines.get(slug).map(Box::as_ref)
}

fn silence_others(&self, id: &str) {
    let keep = music::tag::slug_of(id);
    for (slug, engine) in &self.engines {
        if Some(*slug) != keep {
            engine.pause();
        }
    }
}
```

`preload`, `preload_next`, `active_engine` and `restart_engine` already route through `engine_for` and need no change beyond the field rename. `silence_other` becomes `silence_others`, so update its call sites — `grep -n "silence_other" crates/state/src/playback.rs` names them.

- [ ] **Step 6: Make teardown selective**

```rust
fn teardown(&mut self, slug: &str, cx: &mut Context<Self>) {
    self.engines.remove(slug);
    if !self.current_track_belongs_to(slug) {
        return;
    }
    // the block that clears load/fetch/track/origin/state/position, unchanged
}
```

`current_track_belongs_to` replaces `local_active`:

```rust
fn current_track_belongs_to(&self, slug: &str) -> bool {
    self.track
        .as_ref()
        .and_then(|track| track.id.as_deref())
        .and_then(music::tag::slug_of)
        == Some(slug)
}
```

When the track playing belonged to the provider signing out, clear it and advance: call the same path `Playback` already takes for a track that fails to load, so the queue moves on to the next surviving track rather than stopping.

- [ ] **Step 7: Verify**

Run: `cargo test -p state`
Expected: all pass including the new queue test.

- [ ] **Step 8: Commit**

```bash
git add crates/state/src/playback.rs crates/state/src/queue.rs
git commit -m "feat(state): hold one playback engine per provider"
```

---

### Task 9: `Library` keeps a shelf per provider

**Files:**
- Modify: `crates/state/src/library.rs`

**Interfaces:**
- Produces:
  - `pub struct Shelf { pub state: LibraryState, pub favorites: Option<Vec<Track>>, awaited: Vec<LibraryPart>, tasks: Vec<Task<()>> }`
  - `pub fn shelf(&self, slug: &str) -> Option<&Shelf>`

- [ ] **Step 1: Replace the paired fields**

Delete `state`, `local`, `local_favorites`, `local_favorites_loading`, `awaited`, `local_awaited`, `tasks`, `local_tasks`. Add:

```rust
shelves: HashMap<&'static str, Shelf>,
```

Leave `pending`, `pending_albums`, `pending_artists`, `contents`, `reading` and `mosaics` exactly as they are. They are keyed by id, and tagged ids are already unique across providers — splitting them per provider would be redundant work that also breaks `contents`, which maps a playlist id to its track ids across the same keyspace.

- [ ] **Step 2: Populate `favorites` only when the provider has it**

```rust
favorites: match client.has_all_tracks() {
    true => Some(Vec::new()),
    false => None,
},
```

For a streaming provider, `LibraryState::Ready.tracks` *is* the favourites list and `favorites` stays `None`.

- [ ] **Step 3: Load per slug**

`load` and `load_local` collapse into one `load(slug, client, cx)`. The only difference between them was which fields they wrote; those are now one `Shelf`.

- [ ] **Step 4: React per slug**

The `SessionEvent` arms from Task 6 now insert or remove a single entry in `shelves` rather than clearing everything.

- [ ] **Step 5: Verify**

Run: `cargo check --workspace && cargo test -p state`
Expected: clean; errors remaining only in `views`, which Tasks 10–12 own.

- [ ] **Step 6: Commit**

```bash
git add crates/state/src/library.rs
git commit -m "refactor(state): keep one library shelf per provider"
```

---

### Task 10: Routes name their provider

**Files:**
- Modify: `crates/router/src/lib.rs`
- Modify: every call site the compiler names

**Interfaces:**
- Produces:
  - `Destination::Library(&'static str, LibraryTab)` — replaces both `Library(LibraryTab)` and `Local(LocalTab)`
  - `enum LibraryTab { Songs, Favorites, Albums, Playlists, Artists }` — the superset; a provider without `has_all_tracks` never shows `Songs`
  - `NavEntry::entries(slugs: &[&'static str]) -> Vec<NavEntry>` — replaces `NavEntry::ALL`

- [ ] **Step 1: Merge the two tab enums**

`LibraryTab` gains `Favorites`; `LocalTab` is deleted. `Section::from(LibraryTab)` maps `Songs => Section::Songs` and `Favorites => Section::Favorites`. For a provider without `has_all_tracks`, the tab list omits `Songs` and `Favorites` is the first tab — which is what its Songs tab already meant.

- [ ] **Step 2: Make the nav list dynamic**

```rust
impl NavEntry {
    pub fn entries(slugs: &[&'static str]) -> Vec<Self> {
        let mut entries = vec![Self::Home, Self::Search, Self::History];
        entries.extend(slugs.iter().copied().map(Self::Library));
        entries
    }
}
```

`NavEntry::Library(&'static str)` replaces both `NavEntry::Library` and `NavEntry::Local`. `NavEntry::ALL` is deleted; `hidden_nav` keeps storing string ids, so a whole provider is hidden by its slug.

- [ ] **Step 3: Extend `Screen`**

`Screen::ALL` keeps its eight entries for the picker, but `Screen::destination` needs a slug. Store `startup` as `"<slug>:<screen>"`, falling back to the first connected provider when the stored value carries no slug.

- [ ] **Step 4: Verify**

Run: `cargo check --workspace`
Expected: a list of call sites in `views`. Fix the mechanical ones here; leave `root.rs` and `library/mod.rs` to Tasks 11 and 12 if the change is more than renaming.

- [ ] **Step 5: Commit**

```bash
git add crates/router/src/lib.rs crates/views/src/
git commit -m "feat(router): name the provider in a library route"
```

---

### Task 11: One library view per provider

**Files:**
- Modify: `crates/views/src/root.rs`
- Modify: `crates/views/src/screens/library/mod.rs`

**Interfaces:**
- Consumes: `Destination::Library(slug, tab)`, `MusicApi::has_all_tracks`.
- Produces: `LibraryView::new(slug, library, playback, window, cx)` — `Shelf` is deleted.

- [ ] **Step 1: Replace the two fields with a map**

`root.rs` holds `library: Entity<LibraryView>` and `local: Entity<LibraryView>`. Replace with:

```rust
libraries: HashMap<&'static str, Entity<LibraryView>>,
```

built from `session.active_slugs()` and rebuilt when a provider connects or disconnects.

- [ ] **Step 2: Delete `Shelf`**

`Shelf::Saved` / `Shelf::Local` becomes the slug. Every `shelf.local()` call becomes a `has_all_tracks` check or a slug comparison — read each one and pick which it meant.

- [ ] **Step 3: Key the settings per slug**

`Section::key(shelf)` becomes `Section::key(slug)` returning `format!("{slug}-{section}")`. The fixed-size `[_; 5]` arrays (`views`, `sliders`, `tables()`, `Section::ALL`) are indexed by section within one view and do not change.

- [ ] **Step 4: Verify by running the app**

Run: `cargo build --bin sonora` then launch it. Sign into two providers and confirm both appear in the sidebar with their own tabs.

There is no UI test harness; this step is manual and its evidence is a description of what you saw.

- [ ] **Step 5: Commit**

```bash
git add crates/views/src/
git commit -m "feat(views): give every connected provider its own library"
```

---

### Task 12: One sidebar group per provider

**Files:**
- Modify: `crates/views/src/chrome/sidebar_left.rs`
- Modify: `assets/i18n/en-US/main.ftl`

- [ ] **Step 1: Build the groups from the connected list**

The sidebar renders a fixed Your Library group and a fixed Local Music group. Both become one loop over `session.active_slugs()`, each group labelled with the provider's `name()` and expanded by `expanded(&Destination)` as before.

- [ ] **Step 2: Keep the existing expand behaviour**

A group opens when the route enters it and closes only on the chevron. Do not reintroduce route-driven collapsing.

- [ ] **Step 3: Add the one new string**

The group label is the provider's own name, which is not translatable. Only add a Fluent key if a group needs a word around that name.

- [ ] **Step 4: Verify by running the app**

Confirm each connected provider has its own group, and that signing out of one removes its group without touching the others.

- [ ] **Step 5: Commit**

```bash
git add crates/views/src/chrome/sidebar_left.rs assets/i18n/en-US/main.ftl
git commit -m "feat(views): list every connected provider in the sidebar"
```

---

### Task 13: Migrate what is already on disk

**Files:**
- Modify: `crates/state/src/settings.rs`
- Modify: `crates/state/src/queue.rs`

**Interfaces:**
- Produces:
  - `fn migrate_resume(resume: &mut Resume)`
  - `fn migrate_keys<T>(keys: &mut HashMap<String, T>, slug: &str)`
  - Both called from the settings load path.

**Which maps carry section keys.** Four, all in `Values`, all keyed the same way:
`hidden_columns: HashMap<String, Vec<String>>`, `tables: HashMap<String, Layout>`,
`sorting: HashMap<String, Option<Sorting>>`, `views: HashMap<String, Mode>`.
`pins` is `HashMap<String, Vec<Pin>>` keyed by provider slug and is already
correct — do not touch it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_saved_queue_takes_the_tag_of_the_provider_it_was_saved_under() {
    let mut resume = Resume {
        provider: "spotify".into(),
        current: Some(Stub { id: "7etD5lFGaYcsKmFTmutVYO".into(), ..Default::default() }),
        ..Default::default()
    };
    migrate_resume(&mut resume);
    assert_eq!(resume.current.unwrap().id, "spotify:7etD5lFGaYcsKmFTmutVYO");
}

#[test]
fn an_already_tagged_queue_is_left_alone() {
    let mut resume = Resume {
        provider: "spotify".into(),
        current: Some(Stub { id: "spotify:abc".into(), ..Default::default() }),
        ..Default::default()
    };
    migrate_resume(&mut resume);
    assert_eq!(resume.current.unwrap().id, "spotify:abc");
}

#[test]
fn bare_layout_keys_move_to_the_last_active_provider() {
    let mut keys: HashMap<String, Vec<String>> =
        HashMap::from([("albums".into(), vec!["year".into()])]);
    migrate_keys(&mut keys, "soundcloud");
    assert!(keys.contains_key("soundcloud-albums"));
    assert!(!keys.contains_key("albums"));
}

#[test]
fn local_layout_keys_are_already_correct() {
    let mut keys: HashMap<String, Vec<String>> =
        HashMap::from([("local-albums".into(), vec!["year".into()])]);
    migrate_keys(&mut keys, "soundcloud");
    assert!(keys.contains_key("local-albums"));
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p state --lib settings::`
Expected: FAIL — neither function exists.

- [ ] **Step 3: Implement**

```rust
fn migrate_resume(resume: &mut Resume) {
    let slug = resume.provider.clone();
    for stub in resume.past.iter_mut()
        .chain(resume.current.iter_mut())
        .chain(resume.upcoming.iter_mut())
    {
        stub.id = music::tag::tag(&slug, &stub.id);
    }
}

fn migrate_keys<T>(keys: &mut HashMap<String, T>, slug: &str) {
    const SECTIONS: [&str; 5] = ["songs", "favorites", "albums", "playlists", "artists"];
    for section in SECTIONS {
        if let Some(value) = keys.remove(section) {
            keys.insert(format!("{slug}-{section}"), value);
        }
    }
}
```

`music::tag::tag` is idempotent, so a second run is harmless. Call `migrate_resume` once and `migrate_keys` four times from the settings load path — on `hidden_columns`, `tables`, `sorting` and `views` — each with `values.provider` as the slug.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p state --lib settings::`
Expected: 4 passed.

- [ ] **Step 5: Verify against the user's real settings**

Back up first, then launch and confirm the queue resumes and column layouts survive:

```bash
cp ~/.config/sonora/settings.json ~/.config/sonora/settings.json.bak
cargo run --bin sonora
```

- [ ] **Step 6: Commit**

```bash
git add crates/state/src/settings.rs crates/state/src/queue.rs
git commit -m "feat(state): migrate stored ids and layout keys to the tagged form"
```

---

## What this plan does not cover

Merged Home and Search are part 2, planned separately once this lands. Until then each screen shows the provider whose section you are in.

`Tagged` has no test coverage: it needs a fake `MusicApi` and this codebase has no such harness. The spec accepts that, on the grounds that a missed tag surfaces as a bare id that `slug_for` rejects — visible, not silent. Task 4's `diff` check is the substitute.
