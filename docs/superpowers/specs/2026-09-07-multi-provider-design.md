# Several providers connected at once

Status: design approved, not yet planned
Branch: `feat/multi-provider`, cut from `feat/soundcloud-provider`
Audience: this fork only. Not intended for upstream.

## What this changes

Today Sonora keeps one streaming provider connected. Signing into SoundCloud
replaces Spotify. Local Music is the exception: it runs alongside, with its own
client, catalog and playback factory.

After this change every provider a user has signed into stays connected. Each
gets its own section in the sidebar with its own favourites, albums, playlists
and artists. One queue may hold tracks from any mix of them, and playback moves
between them the way it already moves between a local file and a streamed one.
Home and Search query every connected provider and merge the results.

## The observation this design rests on

The app already does all of this — for exactly two providers, with the second
one written out by hand.

| Shape that already exists | Where | Hardcoded as |
| --- | --- | --- |
| Per-track engine selection | `state::playback::engine_for` | `is_local_id(id)` |
| Silencing the provider not in use | `state::playback::silence_other` | "the other one" |
| A list of connected providers | `state::session::active_slugs` | two `Option` fields |
| Mapping an id to its provider | `state::session::slug_for` | `is_local_id(id)` |
| Sifting a mixed queue on sign-out | `state::queue::sift` | predicate `local` |
| One `LibraryView` per shelf | `views::root` | fields `library` and `local` |

`sift` is already generic over a predicate. `active_slugs` already returns a
`Vec`. `TrackKey { provider, id }` already exists in `music::models`, built from
`slug_for`, used only by the lyrics subsystem.

So this is largely one refactor stated six times: replace the binary
`is_local_id(id)` with a provider lookup. There are **34** direct calls to
`is_local_id` outside its definition — 11 in `state/library.rs`, 5 in
`views/shared/menus/context.rs`, 4 in `state/playback.rs` — against **5** calls
to the `slug_for` seam that was meant to carry them.

The genuinely new work is the id tagging layer. Everything else is
generalisation.

## Approach: ids carry their provider

An id becomes `<slug>:<id>` — `spotify:7etD5lFGaYcsKmFTmutVYO`,
`soundcloud:284873455`. This is the convention `LOCAL_TRACK_PREFIX` (`local:`)
already uses; local ids stop being a special case and become one instance of
the rule. Local ids are file paths, so the tail may contain both `:` and `/`:
parsing is always `split_once(':')`, never a general split.

`Destination`, `ui::Pin`, `settings.json` and `history.sqlite3` all carry ids as
strings and keep working untouched.

### Why not a typed pair

`TrackKey { provider: &'static str, id: String }` already exists, and threading
it everywhere would let the compiler find each site instead of relying on
review. It was rejected for two reasons.

The first is failure mode. Fifteen model types carry an id, several nested
inside others. With a prefix, a site that forgets to tag produces a bare id,
`slug_for` returns `None`, and the failure is visible. With a typed field, a
site that forgets to populate it produces a plausible but wrong provider, and
the request silently goes to the wrong API.

The second is that this is a fork tracking upstream. A prefix concentrates the
change in `state` plus one new file; a typed pair spreads it through the model
and provider crates, which is where upstream moves most.

Inferring the provider from the shape of an id (SoundCloud numeric, YouTube
eleven characters, Spotify base62) was rejected outright: it is a guess that
fails silently, and the first provider with numeric ids breaks it.

### The one hazard, and the obligation it creates

`spotify:` is a live URI scheme in this app:

```rust
crates/router/src/uri.rs:3:  const SCHEME: &str = "spotify:";
```

Sonora handles `spotify:track:…` deep links. A tagged id `spotify:<base62>` has
the same shape with a different meaning.

Checked: `uri::destination("spotify:<base62>")` returns `None`, because
`from_uri` needs a second segment. A tagged id reaching the link parser fails
safely.

The other direction is sharp. A real deep link reaching the tag parser splits
into `provider = "spotify"`, `id = "track:xyz"` — plausible, wrong, and silent.

**Obligation:** `router::uri::destination` tags the ids it produces. It is the
only place a Spotify URI turns into an id, so tagging there makes the two
vocabularies disjoint by construction. Any other code path that accepts a raw
URI where an id is expected is a bug, not a case to defend against.

### Where tagging happens

A new `music::tagged` module holds `Tagged { slug, inner: Arc<dyn MusicApi> }`,
itself a `MusicApi`. It strips the tag from ids on the way in and applies it to
ids in the models on the way out. `Session` wraps each provider before handing
its client out.

Size, measured rather than estimated: `MusicApi` has 43 methods, 26 of which
take an id. Fifteen model types carry an id — `UserProfile`, `UserDetail`,
`Contributor`, `ArtistRef`, `Credit`, `Track`, `Playlist`, `Album`,
`AlbumDetail`, `Genre`, `PlaylistDetail`, `ArtistProfile`, `SavedArtist` among
them — and they nest: `AlbumDetail` holds `Track`s, `ArtistProfile` holds
`Album`s. Expect 700–900 lines in one file, mechanical but not small.

The payoff is that `music::spotify`, `music::youtube`, `music::soundcloud` and
`music::local` do not change at all.

## Session

```rust
struct Connected {
    client: Arc<dyn MusicApi>,        // wrapped in Tagged
    catalog: Arc<CatalogSource>,
    playback: Arc<dyn PlaybackFactory>,
    profile: UserProfile,
    authenticated: bool,
}

struct Session {
    connected: HashMap<&'static str, Connected>,
    providers: Vec<Arc<dyn MusicProvider>>,
    awaiting: Option<&'static str>,   // one sign-in at a time, unchanged
}
```

`connected` replaces `client`/`catalog`/`playback` and their four `local_*`
twins. `active_slugs` becomes `connected.keys()`; `slug_for` becomes: read the
tag, check the provider is connected. `local_slug`, `local_client`,
`local_playback` and the separate `clear_local_folder` path all disappear.

**`providers` stays a separate list.** `LocalProvider::sign_in_options()`
returns an empty `Vec` — it is signed in through `SignIn::Path` from the folder
picker, never from the login screen. `Session::providers()` is what the login
screen iterates to build one tab per provider, so folding local into that list
would give it a tab with an empty pane.

### Events

`SessionEvent::{SignedIn, SignedOut}` become `SignedIn(slug)` /
`SignedOut(slug)`. `LocalChanged` disappears into them.

Thirteen modules subscribe. Nine arms handle `LocalChanged` by doing nothing,
but only one of those (`history.rs:201`) stands alone; the other eight are
combined arms such as `SignedIn | Reconnected | LocalChanged => {}`, which stay
and merely lose a variant. The arms that matter are the three that act
globally:

```rust
queue.rs:     SignedOut => this.purge(cx)      // drops the whole queue
playback.rs:  SignedOut => this.teardown(cx)   // tears down playback state
cover.rs:     SignedOut => this.forget(cx)     // forgets every cover
```

## Signing out of one provider

Checked: this is a predicate swap, not new behaviour.

```rust
fn sift<T>(past, current, upcoming, source, keep: impl Fn(&T) -> bool) -> bool
```

`queue::purge` already calls `sift` with `local` as the predicate, and
`queue.rs` already carries a test named `signing_out_leaves_only_imported_tracks`.
`playback::teardown` already drops only the streaming engine, and only clears
global state `if !self.local_active()`.

| Now | After |
| --- | --- |
| `purge(cx)`, `keep = local` | `purge(slug, cx)`, `keep = \|t\| provider_of(t) != slug` |
| `local_active()` | `current_track_belongs_to(slug)` |
| `teardown(cx)` drops `engine` | `teardown(slug, cx)` drops `engines[slug]` |

`sift` itself does not change.

The one decision: when the track playing belongs to the provider being signed
out, stop it and advance to the next surviving track in the queue, reusing the
path `Playback` already takes for a track that fails to load.

## Library

```rust
struct Library {
    shelves: HashMap<&'static str, Shelf>,
    pending: HashMap<String, Task<()>>,          // keyed by id — stay flat
    pending_albums: HashMap<String, Task<()>>,
    pending_artists: HashMap<String, Task<()>>,
    contents: HashMap<String, HashSet<String>>,  // playlist id -> track ids
    reading: HashMap<String, Task<()>>,
    mosaics: HashMap<String, Task<()>>,
}

struct Shelf {
    state: LibraryState,
    favorites: Option<Vec<Track>>,   // only when all_tracks differs from saved_tracks
    awaited: Vec<LibraryPart>,
    tasks: Vec<Task<()>>,
}
```

The six id-keyed maps do not need splitting per provider, and that is the
tagging paying for itself: `pending` keyed on a bare `123` would let a Spotify
album and a SoundCloud album cancel each other's load. `spotify:123` and
`soundcloud:123` are distinct for free.

`LibraryState` is unchanged. Its `Ready { .., problems }` variant already models
partial failure, which stops being an edge case: with several providers
connected, one failing while the others work is the normal condition.

### The tab set is not fixed

```rust
enum LibraryTab { Songs, Albums, Playlists, Artists }              // 4
enum LocalTab   { Songs, Favorites, Albums, Playlists, Artists }   // 5
```

For a streaming provider the Songs tab *is* favourites
(`LibraryTab::Songs => Section::Favorites`). Local has both, because
`MusicApi::all_tracks` defaults to `saved_tracks` and only the local provider
answers differently.

So the number of tabs is a property of the provider, not of "is it local".
`MusicApi` gains a capability — `fn has_all_tracks(&self) -> bool { false }` —
and the tab set follows from it. `Section::ALL` is already `[Self; 5]`, sized
for the superset, so the fixed-size arrays in `LibraryView` (`views`,
`sliders`, `tables()`) do not grow: they are indexed by section within one
shelf, not by provider.

## Playback and the queue

```rust
engine: Option<Box<dyn Player>>,        →  engines: HashMap<&'static str, Box<dyn Player>>
local_engine: Option<Box<dyn Player>>,

engine_for(id)      →  engines.get(tag_of(id))
silence_other(id)   →  pause every engine but this id's
local_active()      →  current_track_belongs_to(slug)
```

`preload`, `preload_next` and `restart_engine` already route through
`engine_for` and follow without change. There is no crossfade anywhere in the
codebase, so nothing has to be preserved across an engine switch; `BlazingSink`
ramps gain within one engine.

## Sidebar, router and settings

`views::root` already holds one `LibraryView` per shelf (`library` and
`local`); that becomes `HashMap<slug, Entity<LibraryView>>`.

```rust
Destination::Library(LibraryTab)     ┐
Destination::Local(LocalTab)         ┘→  Destination::Library(slug, Tab)

NavEntry::ALL: [Self; 5]             →  built at runtime: Home, Search, History,
                                        plus one entry per connected provider
```

`NavEntry::ALL` can no longer be a `const`: its length depends on who is
connected.

Settings need no schema change. `startup: String` and `hidden_nav: Vec<String>`
already hold string ids, so `"spotify:playlists"` and hiding a whole provider
by slug both fit.

## Home and Search merged

`HomeFeed { listen_again, quick_picks, sections }` merges field by field.
`listen_again` and `quick_picks` interleave round-robin across providers, so a
slow or content-rich provider cannot crowd the others out. `sections` are
concatenated with the provider name in the label — otherwise two shelves both
called "New releases" are indistinguishable.

Search merges the same way, per category.

Every provider is queried in parallel under a deadline. One that is slow or
refuses must not hold up the rest: show what has arrived and record the rest in
`problems`.

Known imbalance, accepted: SoundCloud does not implement `home()` and falls
back to the trait default, an empty `HomeFeed`. A merged Home is Spotify plus
YouTube, with SoundCloud appearing only in `listen_again` once something has
been played from it.

## Migration

Nothing to do for `history.sqlite3`: its primary key is already
`(scope, provider, track_id, played_at)`. Nothing to do for pinned items:
`settings.json` already stores them as `HashMap<String, Vec<Pin>>` keyed by
provider slug, and `sidebar_left.rs` already reads them with
`pinned(&session.active_slugs())`.

Two things need a one-time pass on load:

- **The saved queue.** `Resume` carries `provider: String` alongside untagged
  ids, so each stub's id takes that provider's tag.
- **Column layouts.** Keys are bare for streaming (`songs`, `albums`) and
  prefixed for local (`local-songs`). They become `<slug>-songs` throughout.
  `settings.json` records `provider: String`, the last active provider, so the
  bare keys move to exactly that slug. `local-*` keys are already correct.

## Testing

This codebase has no UI or network harness; tests are pure functions at the
bottom of the file they cover. What that covers here:

- `tag_of` / `untag`, including a real `spotify:track:abc` deep link, a local id
  whose path contains both `:` and `/`, and an untagged id
- the round-robin interleave used by merged Home and Search
- `sift` with a provider predicate, extending the existing queue tests
- the two migrations above

**Known gap:** `Tagged` cannot be covered this way. It needs a fake `MusicApi`,
and no such harness exists. Its correctness rests on review, and the failure
mode is a bare id reaching `slug_for` — visible, not silent, which is why the
prefix approach was chosen.

## Out of scope

- Upstream contribution. This diverges from upstream's one-provider model
  deliberately.
- Playing from two providers simultaneously. One queue, one engine sounding at
  a time.
- Crossfade. It does not exist today and is not added here.
- Deduplicating the same track offered by several providers.
