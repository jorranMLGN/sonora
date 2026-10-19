# YouTube Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A second YouTube provider for plain youtube.com (search, channels, playlists, account library) that plays a video's audio through the shared engine, can seek anywhere in a long upload at once, and prepares H.264 video formats for project B.

**Architecture:** The innertube parsers for youtube.com and the `VideoFormat` model land in a personal fork of `ytmusic-rs`. In sonora, `music::stream` learns a ranged restart. `MusicApi` gains `search_artists`. The existing YouTube Music provider is parameterised by a `Site`, so the new `music::youtube_video` module only adds a `MusicApi` client and its model mapping.

**Tech Stack:** Rust 2024, tokio, `ytmusic` (fork), serde_json, rodio/symphonia, GPUI (state and views only).

**Spec:** `docs/superpowers/specs/2026-10-19-youtube-provider-design.md`

## Global Constraints

- **Never run `git commit` or `git push` without the user's explicit approval for that one action.** This covers subagents too. Each "Commit" step below means: show the exact command and files, ask, and wait.
- Commit messages follow Conventional Commits (`type(scope): description`), lowercase and imperative, with no body and no `Co-Authored-By` or other assistant attribution. Fork commits use scope `web` or `player`.
- No YouTube Data API, no API key, no client secret. Use innertube only, through `YtMusic::execute` / `execute_with`.
- `state` and `views` never name a provider module. Only `crates/sonora/src/main.rs` names `music::youtube_video`.
- Slug `"youtube-video"`, display name `"YouTube"`, credentials dir `credentials::dir("youtube-video")`, cargo feature `youtube-video = ["youtube"]` in `default`.
- Logo: one file, `assets/icons/common/youtube.svg`.
- Every user-facing string comes from `assets/i18n/en-US/main.ftl`. Logs are prefixed `youtube:` or `playback:` and written in English.
- Never hardcode a color, radius or size in views (no view work is planned, but this holds if one turns up).
- Tests are TDD, in `#[cfg(test)] mod tests` at the bottom of the file they cover. Live tests are `#[ignore]` and **read-only**, and are run by exact name only.
- Fixtures recorded with a signed-in account must not carry personal data. Replace channel names, playlist titles, avatars and ids of the account's own items with placeholders before committing them.
- Clippy stays clean: `cargo clippy --workspace --all-targets`.
- `CHANGELOG.md` `## [Unreleased]` → `Added` gets one user-facing entry.

## Review Focus

1. **Shorts, live streams, premieres and members-only videos in any list** must not break a page or play as a broken track. Shorts and lengthless items are dropped from lists, and a members-only video is gated. Tests: Task 3 `shorts_and_live_items_are_dropped`, Task 9 `a_video_without_length_is_not_a_track`.
2. **Seeking back into a hole after a jump** (play at 0:10, jump to 1:30:00, jump back to 0:45:00) must restart there rather than wait for the frontier. Test: Task 6 `a_read_into_an_earlier_hole_restarts_there`.
3. **A signed-out listener opening library tabs** gets the sign-in prompt and empty lists, not an error toast per tab. Test: Task 4 `a_signed_out_browse_parses_to_nothing`.
4. **A region-locked, age-gated or removed video** in the queue is skipped as unavailable or gated and must not stall playback. Test: Task 2 `an_unplayable_response_reports_sign_in_or_unavailable`.
5. **Watch Later or a playlist with more than 100 videos** loads in full through continuations. Test: Task 4 `a_playlist_continuation_yields_the_next_page_and_token`.

---

## File Structure

**Fork `jorranMLGN/ytmusic-rs`** (local clone at `~/Documents/github/ytmusic-rs`):
- `src/models.rs`: add `VideoFormat`.
- `src/player.rs`: generic `ciphered`, `video_format`, `YtMusic::video_formats`, `playability` errors.
- `src/stream.rs`: `AudioStream::resumed`.
- `src/web/mod.rs`: `Video`, `ChannelRef`, `Channel`, `WebPlaylist`, `WebSearch`, `ChannelPage`, `PlaylistPage`, `Shelf`.
- `src/web/parse.rs`: pure `&Value → model` parsers.
- `src/web/api.rs`: `impl YtMusic { web_* }` calling `execute(.., Client::Web, ..)`.
- `tests/fixtures/web/*.json`: recorded responses.

**Sonora:**
- `crates/music/src/stream.rs`: `Ranged`, `Stream::pulling_ranged`, sparse spool.
- `crates/music/src/youtube/playback.rs`: `impl Ranged for AudioStream`, use `pulling_ranged`.
- `crates/music/src/lib.rs`, `crates/music/src/tagged/mod.rs`, `crates/state/src/search.rs`: `search_artists`.
- `crates/music/src/youtube/site.rs` (new) and `crates/music/src/youtube/mod.rs`: `Site`.
- `crates/music/src/youtube_video/{mod.rs, client.rs, wire.rs}` (new).
- `Cargo.toml`, `crates/music/Cargo.toml`, `crates/sonora/src/main.rs`, `crates/music/src/tag.rs`, `crates/views/src/shared/mod.rs`, `assets/icons/common/youtube.svg`, `assets/i18n/en-US/main.ftl`, `CHANGELOG.md`.
- `crates/music/src/live_tests/youtube_video.rs` (new).

---

### Task 0: Fork and local patch

**Files:**
- Modify: `Cargo.toml` (root, a `[patch]` section)

- [ ] **Step 1: Ask the user** before running `gh repo fork sonorahq/ytmusic-rs --clone=false`. This publishes a repo under `jorranMLGN`. Then `git clone https://github.com/jorranMLGN/ytmusic-rs ~/Documents/github/ytmusic-rs && git -C ~/Documents/github/ytmusic-rs checkout -b feat/web 8b044c4`.
- [ ] **Step 2: Point sonora at the clone** for development:

```toml
[patch."https://github.com/sonorahq/ytmusic-rs"]
ytmusic = { path = "../ytmusic-rs" }
```

- [ ] **Step 3: Verify.** Run `cargo check --workspace`. Expected: it finishes, and `cargo tree -p music -i ytmusic` shows the path source.

No commit. The patch is replaced in Task 11.

---

### Task 1 (fork): `VideoFormat` and a generic format filter

**Files:**
- Modify: `src/models.rs`, `src/player.rs`
- Test: `src/player.rs` tests, fixture `tests/fixtures/web/player_web.json` (a WEB-client `player` response for `dQw4w9WgXcQ`)

**Interfaces:**
- Produces:

```rust
pub struct VideoFormat {
    pub itag: u32, pub url: String, pub mime: String, pub codecs: String,
    pub width: u32, pub height: u32, pub fps: u32, pub bitrate: u32,
    pub content_length: Option<u64>, pub init_range: Option<Range<u64>>,
    pub index_range: Option<Range<u64>>, pub duration: Option<Duration>,
    pub user_agent: &'static str,
}
impl YtMusic { pub async fn video_formats(self: &Arc<Self>, video_id: &str) -> Result<Vec<VideoFormat>> }
```

- `ciphered(format: &Value, wanted: impl Fn(&str) -> bool) -> Option<Ciphered>`. The audio path passes `|mime| mime.starts_with("audio/")`.

- [ ] **Step 1: Record the fixture.** Add an `#[ignore] record_player_fixture` test that writes the raw WEB `player` JSON for `dQw4w9WgXcQ` as a guest. Run it by exact name.
- [ ] **Step 2: Write the failing test** `h264_video_formats_parse_with_size_and_ranges`. Parse every `adaptiveFormats` entry of the fixture with `video_format`, keep the `avc1` ones, and assert:
  - there are at least 5;
  - every one has `mime.starts_with("video/mp4")` and `codecs.starts_with("avc1")`;
  - one has `height == 1080`, and its `index_range` and `init_range` are `Some`;
  - none is audio.
- [ ] **Step 3: Write the failing test** `audio_formats_are_unchanged_by_the_generic_filter`. `audio_format` over the same fixture returns the same itags as before the change (pin the list read from the fixture).
- [ ] **Step 4: Run** `cargo test -p ytmusic h264_video_formats audio_formats_are_unchanged`. Expected: FAIL (`video_format` not defined).
- [ ] **Step 5: Implement** `VideoFormat`, `video_format(format: &Value, client: Client) -> Option<VideoFormat>`, and the predicate on `ciphered`.
  - `YtMusic::video_formats` reuses the guest-then-signed order of `open_audio`, deciphers, and returns the `avc1` formats sorted by height ascending.
  - Do not set `isAudioOnly` for this call.
- [ ] **Step 6: Run the tests.** Expected: PASS. Run the whole fork suite with `cargo test -p ytmusic`. Expected: PASS.
- [ ] **Step 7: Commit (fork, after approval):** `feat(player): parse h264 video formats beside audio`

---

### Task 2 (fork): resuming a stream at a byte, and playability errors

**Files:**
- Modify: `src/stream.rs`, `src/player.rs`

**Interfaces:**
- Produces: `impl AudioStream { pub fn resumed(&self, at: u64) -> AudioStream }`. It keeps the same url, agent, total and renewal (a fresh `Weak`), sets `offset = at`, and has no open response. The first `chunk()` sends `Range: bytes=at-…`.

- [ ] **Step 1: Write the failing test** `a_resumed_stream_asks_from_its_offset`. Use a local `tokio` TCP server that serves a fixed 3 MiB body with range support and records each `Range` header. Open an `AudioStream`, call `resumed(2 * 1024 * 1024)`, and read one chunk from the resumed stream. Assert:
  - the recorded header starts with `bytes=2097152-`;
  - the chunk equals `body[2097152..2097152 + chunk.len()]`.
- [ ] **Step 2: Write the failing test** `an_unplayable_response_reports_sign_in_or_unavailable`. Feed three handwritten `playabilityStatus` objects to the function that classifies them: `LOGIN_REQUIRED`, `UNPLAYABLE` (members-only) and `ERROR` (removed). Assert:
  - `LOGIN_REQUIRED` and members-only come back as `SignInRequired`;
  - removed comes back as a plain error whose message contains the reason.
- [ ] **Step 3: Run both tests.** Expected: FAIL.
- [ ] **Step 4: Implement** `resumed`, and extend the existing playability check to cover members-only if it does not already.
- [ ] **Step 5: Run** `cargo test -p ytmusic`. Expected: PASS.
- [ ] **Step 6: Commit (fork, after approval):** `feat(player): resume an audio stream at a byte`

---

### Task 3 (fork): web models and search

**Files:**
- Create: `src/web/mod.rs`, `src/web/parse.rs`, `src/web/api.rs`; `pub mod web;` in `src/lib.rs`
- Fixtures: `tests/fixtures/web/search_boiler_room.json`, `tests/fixtures/web/search_shorts.json`

**Interfaces:**
- Produces, all `Clone + Debug`:

```rust
pub struct ChannelRef { pub id: String, pub name: String }
pub struct Video { pub id: String, pub title: String, pub channel: Option<ChannelRef>,
                   pub duration: Option<Duration>, pub thumbnails: Vec<Thumbnail> }
pub struct Channel { pub id: String, pub name: String, pub thumbnails: Vec<Thumbnail>,
                     pub subscribed: Option<bool> }
pub struct WebPlaylist { pub id: String, pub title: String, pub owner: Option<ChannelRef>,
                         pub thumbnails: Vec<Thumbnail>, pub count: Option<u32> }
pub struct WebSearch { pub videos: Vec<Video>, pub channels: Vec<Channel>,
                       pub playlists: Vec<WebPlaylist> }
pub fn parse::search(response: &Value) -> WebSearch
impl YtMusic { pub async fn web_search(&self, query: &str) -> Result<WebSearch> }
```

- Renderers to read: `videoRenderer`, `channelRenderer`, `playlistRenderer`, `lockupViewModel` (newer playlist and video cards). Use `parse::find_renderers`, `nav::Nav`, `runs_text` and `thumbnails` from the crate.
- Shorts (`reelItemRenderer`, `shortsLockupViewModel`, or a `/shorts/` url) and items without a length (live, upcoming) are dropped from `videos`.

- [ ] **Step 1: Record fixtures.** Add an `#[ignore] record_web_fixtures` guest test that writes the raw `search` responses for "boiler room" and for "shorts funny". Later tasks extend it.
- [ ] **Step 2: Write the failing test** `a_search_lists_videos_channels_and_playlists`. Parse `search_boiler_room.json` and assert:
  - `videos.len() >= 10`;
  - every video id has 11 characters;
  - every video has `duration.is_some()` and a channel;
  - `channels` contains one named `"Boiler Room"` whose id starts with `UC`;
  - `playlists` is not empty, and no playlist id starts with `VL`, because the browse prefix is stripped.
- [ ] **Step 3: Write the failing test** `shorts_and_live_items_are_dropped`. Parse `search_shorts.json` and assert that no video has `duration` under 1 s or `None`, and that the parser saw at least one shorts renderer (count it with `find_renderers` in the test).
- [ ] **Step 4: Run** `cargo test -p ytmusic web::`. Expected: FAIL.
- [ ] **Step 5: Implement** the models, `parse::search` and `web_search`. The query payload is `{ "query": query }` on endpoint `search` with `Client::Web`.
- [ ] **Step 6: Run** `cargo test -p ytmusic`. Expected: PASS.
- [ ] **Step 7: Commit (fork, after approval):** `feat(web): search videos, channels and playlists on youtube`

---

### Task 4 (fork): channels, playlists, continuations, library

**Files:**
- Modify: `src/web/mod.rs`, `src/web/parse.rs`, `src/web/api.rs`
- Fixtures:
  - guest: `channel_boiler_room.json` (the videos tab), `playlist_long.json` with `playlist_long_continuation.json` (a public playlist of more than 200 videos);
  - signed-in and scrubbed: `subscriptions.json` (`FEchannels`), `library_playlists.json` (`FEplaylist_aggregation`), `watch_later.json` (`VLWL`), `liked.json` (`VLLL`);
  - `signed_out_library.json` (a guest `browse` of `FEchannels`).

**Interfaces:**
- Produces:

```rust
pub struct ChannelPage { pub channel: Channel, pub description: Option<String>,
                         pub uploads: Vec<Video>, pub continuation: Option<String> }
pub struct PlaylistPage { pub playlist: WebPlaylist, pub videos: Vec<Video>,
                          pub continuation: Option<String> }
parse::channel(&Value) -> Option<ChannelPage>
parse::playlist(&Value) -> Option<PlaylistPage>
parse::continuation(&Value) -> (Vec<Video>, Option<String>)
parse::subscriptions(&Value) -> Vec<Channel>
parse::library_playlists(&Value) -> Vec<WebPlaylist>
impl YtMusic {
    pub async fn web_channel(&self, channel_id: &str) -> Result<ChannelPage>;
    pub async fn web_playlist(&self, playlist_id: &str) -> Result<PlaylistPage>;
    pub async fn web_continuation(&self, token: &str) -> Result<(Vec<Video>, Option<String>)>;
    pub async fn web_subscriptions(&self) -> Result<Vec<Channel>>;
    pub async fn web_library_playlists(&self) -> Result<Vec<WebPlaylist>>;
    pub async fn set_subscribed(&self, channel_id: &str, subscribed: bool) -> Result<()>;
}
```

- `web_channel` browses `channel_id` with the videos tab `params` (read it from the tab's `browseEndpoint`).
- `web_playlist` browses `VL{id}`.
- `set_subscribed` posts `subscription/subscribe` or `subscription/unsubscribe` with `{ "channelIds": [id] }` on `Client::Web`.

- [ ] **Step 1: Extend `record_web_fixtures`.**
  - Guest fixtures are recorded as they come.
  - Signed-in ones are recorded behind `YTMUSIC_COOKIES`, then scrubbed by hand per Global Constraints.
- [ ] **Step 2: Write the failing tests:**
  - `a_channel_page_lists_its_uploads`: name `"Boiler Room"`, `uploads.len() >= 20`, `continuation.is_some()`.
  - `a_playlist_continuation_yields_the_next_page_and_token`: the first page holds 100 videos and a token, and the continuation fixture parses to more than 0 videos.
  - `subscriptions_parse_to_channels`: count equals the scrubbed fixture's channel count, ids start with `UC`.
  - `the_library_lists_watch_later_and_own_playlists`: one id is `WL`.
  - `liked_videos_parse_as_a_playlist`: `LL` parses with videos.
  - `a_signed_out_browse_parses_to_nothing`: `subscriptions` and `library_playlists` over `signed_out_library.json` return empty vectors.
- [ ] **Step 3: Run** `cargo test -p ytmusic web::`. Expected: FAIL.
- [ ] **Step 4: Implement** the parsers and calls.
- [ ] **Step 5: Run** `cargo test -p ytmusic`. Expected: PASS.
- [ ] **Step 6: Commit (fork, after approval):** `feat(web): read channels, playlists and the signed-in library`

---

### Task 5 (fork): home, related videos, video details

**Files:**
- Modify: `src/web/*`
- Fixtures: `home.json` (signed-in `FEwhat_to_watch`, scrubbed), `next.json` (guest `next` for `dQw4w9WgXcQ`)

**Interfaces:**
- Produces:

```rust
pub struct Shelf { pub title: String, pub videos: Vec<Video> }
parse::home(&Value) -> Vec<Shelf>
parse::related(&Value) -> Vec<Video>
parse::video_details(&Value) -> Option<Video>   // from a player response's videoDetails
impl YtMusic {
    pub async fn web_home(&self) -> Result<Vec<Shelf>>;
    pub async fn web_related(&self, video_id: &str) -> Result<Vec<Video>>;
    pub async fn web_video(&self, video_id: &str) -> Result<Video>;
}
```

- `home` flattens a feed without shelves (a plain `richGridRenderer`) into one shelf whose title is empty. Sonora names it from Fluent.

- [ ] **Step 1: Write the failing tests:**
  - `the_home_feed_parses_to_shelves_of_videos`: at least one shelf, with every video id 11 characters.
  - `related_videos_exclude_the_playing_one`: no id equals `dQw4w9WgXcQ`, and the list has at least 10 entries.
  - `video_details_carry_title_channel_and_length`: over `player_web.json` from Task 1, the title is not empty, the channel id starts with `UC`, and the duration is 213 s ± 1.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.** Related videos come from `next` (`compactVideoRenderer` and `lockupViewModel` under the secondary results).
- [ ] **Step 4: Run** `cargo test -p ytmusic`. Expected: PASS.
- [ ] **Step 5: Commit (fork, after approval):** `feat(web): read the home feed, related videos and video details`

---

### Task 6: ranged restart in `music::stream`

**Files:**
- Modify: `crates/music/src/stream.rs`, `crates/music/src/youtube/playback.rs`
- Test: `crates/music/src/stream.rs` tests

**Interfaces:**
- Produces:

```rust
/// A source that can start again at any byte of the same file.
pub trait Ranged: Source + Sized {
    fn resumed(&self, at: u64) -> Self;
}
impl Stream<Plain> {
    pub fn pulling_ranged(source: impl Ranged, total: Option<u64>) -> Self;
}
```

- Consumes: `ytmusic::AudioStream::resumed` (Task 2).

Behaviour:
- **Spool.** The spool records filled ranges instead of one length, on the file store. The memory fallback and `total == None` keep today's in-order behaviour.
- **Jumping.** A `Reader` read at byte `p` that is not filled and lies before the current run's start, or more than `LEAP = 2 MiB` past its frontier, stops that run and starts one from `source.resumed(p)`.
- **Finishing.** When a run reaches the end of the file or an already-filled range, the pump continues at the lowest remaining hole. `finished()` and `done()` mean every byte of `0..total` is filled.
- **Ceiling.** `CEILING` counts the bytes of one run, not the whole spool.
- **Callers.** `Stream::pulling` stays as it is for every other caller.

- [ ] **Step 1: Write a test source** `Fake { data: Arc<Vec<u8>>, at: u64, log: Arc<Mutex<Vec<u64>>>, gate: Option<Arc<Semaphore>> }` that implements `Source` and `Ranged`, and logs every `resumed` offset.
- [ ] **Step 2: Write the failing tests:**
  - `a_read_far_ahead_restarts_the_download_there`: total 16 MiB, gated so that only 1 MiB arrives. Seek a reader to 12 MiB and read 64 KiB. The bytes equal `data[12 MiB..+64 KiB]` and the log is `[12 MiB]`.
  - `a_read_just_ahead_waits_instead_of_restarting`: seek to the frontier + 512 KiB, open the gate, and the log stays empty.
  - `bytes_before_a_jump_stay_readable`: after the jump, reading 0..1 MiB returns the original bytes with no new log entry.
  - `a_read_into_an_earlier_hole_restarts_there`: jump to 12 MiB, then read at 6 MiB. The log is `[12 MiB, 6 MiB]`.
  - `the_holes_fill_once_the_end_is_reached`: ungated, after one jump `finished().await` returns, every byte matches, and `done()` is true.
  - `the_ceiling_counts_per_run`: with a test-only ceiling of 4 MiB over a 6 MiB file and one jump at 3 MiB, the stream finishes without the ceiling error.
- [ ] **Step 3: Run** `cargo test -p music stream::`. Expected: FAIL.
- [ ] **Step 4: Implement** in `stream.rs`. The existing tests must keep passing.
- [ ] **Step 5: Wire YouTube.** Add `impl Ranged for ytmusic::AudioStream` in `youtube/playback.rs`. `attempt_open` uses `Stream::pulling_ranged(audio, total)` when `total.is_some()`, and falls back to `pulling` otherwise.
- [ ] **Step 6: Run** `cargo test -p music` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 7: Commit (after approval):** `feat(music): restart a stream's download where a seek lands`

---

### Task 7: `MusicApi::search_artists`

**Files:**
- Modify: `crates/music/src/lib.rs` (beside `search_playlists`, `:331`), `crates/music/src/tagged/mod.rs` (after `:283`), `crates/state/src/search.rs` (`Parts` `:123`, `Catalog` `:241`, fan-out `:456-490`, `artists()` `:875`)
- Test: `crates/state/src/search.rs` tests

**Interfaces:**
- Produces:
  - `async fn search_artists(&self, _query: &str) -> Result<Vec<SavedArtist>> { Ok(Vec::new()) }` on `MusicApi`;
  - `Parts.artists: bool`;
  - `Catalog.artists: Vec<SavedArtist>`.
- `tagged` forwards it and tags with `models::saved_artist`.
- `Parts::of(None)` and `Parts::of(Some(Kind::Artists))` set `artists: true`. The other kinds set it to false.
- `artists()` takes `found: &[SavedArtist]`. Each one is recorded with `favored(named(&artist.name, query), None)` and its `cover`, so it merges by id with the `artist_refs` hits.

- [ ] **Step 1: Write the failing test** `a_searched_artist_appears_without_any_of_its_tracks`. Give `artists()` no tracks and no albums, plus `found = [SavedArtist { id: "youtube-video:UCx", name: "Boiler Room", cover: Some("c"), added_at: None }]` and the query "boiler". Assert one `Hit::Artist` with that id and cover.
- [ ] **Step 2: Write the failing test** `a_searched_artist_merges_with_the_same_artist_from_tracks`. Give one track with `artist_refs` id `youtube-video:UCx` plus the same `found`. Assert exactly one artist hit.
- [ ] **Step 3: Run** `cargo test -p state search::`. Expected: FAIL.
- [ ] **Step 4: Implement** the trait method, the forwarding, the parts, the fan-out and the merge. `crates/mobile` compiles unchanged.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 6: Commit (after approval):** `feat(state): search artists directly where a provider can`

---

### Task 8: a `Site` for the YouTube provider

**Files:**
- Create: `crates/music/src/youtube/site.rs`
- Modify: `crates/music/src/youtube/mod.rs`

**Interfaces:**
- Produces:

```rust
/// Which YouTube a provider signs in to and talks to.
pub(crate) struct Site {
    pub name: &'static str,          // "YouTube Music" | "YouTube"
    pub slug: &'static str,          // "youtube" | "youtube-video"
    pub reach: &'static str,         // "music.youtube.com" | "www.youtube.com"
    pub sign_in_url: &'static str,
    pub landing: &'static str,
    pub guest_name: &'static str,
    pub client: fn(Arc<YtMusic>, Option<String>) -> Arc<dyn MusicApi>,
}
pub(crate) const MUSIC: Site;
impl YouTubeProvider { pub(crate) fn for_site(site: &'static Site, cache: PathBuf) -> Self }
```

- `YouTubeProvider::new()` and `at()` keep their signatures and use `MUSIC`.
- `name`, `slug`, `reach`, `web_sign_in` (url and landing), `guest_session` (display name) and both sessions' `api` read from the site. The `Option<String>` is the owner's display name (`owned_by`).
- The plain-YouTube sign-in url: `https://accounts.google.com/ServiceLogin?service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26next%3Dhttps%253A%252F%252Fwww.youtube.com%252F`, with landing `www.youtube.com`.

- [ ] **Step 1: Write the failing test** `each_site_keeps_its_own_credentials`. Build `for_site(&MUSIC, tmp/a)` and a test site with slug `"youtube-video"` at `tmp/b`. Store a guest on one, and assert `stored()` is true for that one and false for the other.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Implement.** No behaviour change for YouTube Music.
- [ ] **Step 4: Run** `cargo test -p music youtube::` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 5: Commit (after approval):** `refactor(music): let the youtube provider serve more than one site`

---

### Task 9: `music::youtube_video`, the client and its mapping

**Files:**
- Create: `crates/music/src/youtube_video/mod.rs`, `client.rs`, `wire.rs`
- Modify: `crates/music/src/lib.rs` (`#[cfg(feature = "youtube-video")] pub mod youtube_video;`), `crates/music/Cargo.toml` (feature `youtube-video = ["youtube"]`, added to `default`)

**Interfaces:**
- Consumes: Tasks 3–5 `web_*`, Task 7 `search_artists`, Task 8 `Site`.
- Produces:
  - `pub fn provider() -> crate::youtube::YouTubeProvider`, which is `for_site(&VIDEO, credentials::dir("youtube-video"))`;
  - `pub(crate) const VIDEO: Site`;
  - `pub struct YouTubeVideoClient` implementing `MusicApi`.
- `wire`:
  - `track(Video) -> Option<Track>`: `None` without a duration. Artist and `artist_refs` come from the channel, the cover is the widest thumbnail, there is no album.
  - `artist(Channel) -> SavedArtist`.
  - `playlist(WebPlaylist) -> Playlist`.

`MusicApi` mapping (anything not listed returns its trait default):

| Method | Source |
|---|---|
| `search` / `search_artists` / `search_playlists` | `web_search` videos / channels / playlists. Fetch once and memoise per query for 30 s, since all three run per keystroke. |
| `search_albums`, `saved_albums`, `album_tracks` | `Ok(Vec::new())` |
| `album`, `set_album_saved` | `bail!("youtube has no albums")` |
| `artist` | `web_channel` uploads as `top_tracks`, `albums` empty |
| `artist_profile` | `web_channel` description and avatar |
| `artist_images` | `Ok(HashMap::new())` |
| `saved_artists` / `set_artist_saved` | `web_subscriptions` / `set_subscribed` |
| `saved_tracks` / `set_track_saved` | `web_playlist("LL")` plus every continuation / `rate_track` |
| `playlists` | `web_library_playlists`, with `WL` moved first |
| `playlist` / `playlist_continuation` / `playlist_tracks` / `playlist_covers` | `web_playlist` / `web_continuation` |
| `create_playlist`, `rename_playlist`, `delete_playlist`, `set_playlist_public`, `add_track_to_playlist`, `remove_track_from_playlist`, `add_playlist_to_library`, `remove_playlist_from_library` | the existing `ytmusic` mutations |
| `track` | `web_video` |
| `track_playcount` | `Ok(None)` |
| `track_radio` | `web_related` |
| `home` | `web_home` shelves as `HomeFeed` sections. An empty shelf title gets a Fluent key resolved in views. |
| `share_url` | `https://www.youtube.com/watch?v=`, `/channel/`, `/playlist?list=` |

- Capabilities: `Capabilities { playcounts: false, ..Capabilities::ALL }`. Shape: `Shape::Saved`.

- [ ] **Step 1: Write the failing tests** in `wire.rs`:
  - `a_video_becomes_a_track_by_its_channel`: the title, artist name and `artist_refs[0].id == "UCx"`, a cover of the widest thumbnail, and the duration are all kept.
  - `a_video_without_length_is_not_a_track`.
  - `a_channel_becomes_an_artist_with_its_avatar`.
- [ ] **Step 2: Write the failing test** `watch_later_comes_first` in `client.rs`, on the ordering helper `fn ordered(playlists: Vec<Playlist>) -> Vec<Playlist>`.
- [ ] **Step 3: Run** `cargo test -p music youtube_video::`. Expected: FAIL.
- [ ] **Step 4: Implement** the module.
- [ ] **Step 5: Run** `cargo test -p music` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 6: Commit (after approval):** `feat(music): add a youtube provider for videos`

---

### Task 10: registration, logo, strings, live tests

**Files:**
- Modify: `crates/sonora/src/main.rs:103-110` (add `Arc::new(music::youtube_video::provider())` after the YouTube Music provider), `crates/music/src/tag.rs:19` (`SLUGS` gains `"youtube-video"`), `crates/views/src/shared/mod.rs:48` (`"youtube-video" => "icons/youtube.svg"`), `assets/i18n/en-US/main.ftl`, `CHANGELOG.md`
- Create: `assets/icons/common/youtube.svg`, `crates/music/src/live_tests/youtube_video.rs`

- [ ] **Step 1: Write the failing test** `a_youtube_video_tag_splits_to_its_slug` in `tag.rs`: `split("youtube-video:abc") == Some(("youtube-video", "abc"))` and `split("youtube:abc") == Some(("youtube", "abc"))`.
- [ ] **Step 2: Run it.** Expected: FAIL. Then implement `SLUGS`, and the test PASSES.
- [ ] **Step 3: Add the logo.** Use YouTube's play-button mark as a single-colour SVG, matching the style of `youtubemusic.svg`.
- [ ] **Step 4: Add the strings.**
  - Every Fluent key that names the `youtube` slug's sign-in copy gets a `youtube-video` twin. Find them with `grep -n youtube assets/i18n/en-US/main.ftl`.
  - Add the untitled home shelf key, e.g. `home-youtube-recommended = Recommended`.
- [ ] **Step 5: Add the CHANGELOG entry** under `Added`: "Sign in to YouTube, or play as a guest, to search and play videos, DJ sets and live recordings that YouTube Music does not have. Your subscriptions, playlists, Watch Later and liked videos are in your library."
- [ ] **Step 6: Write the read-only live tests** (`#[ignore]`):
  - `live_search_finds_the_boiler_room_channel`;
  - `live_a_long_set_seeks_to_ninety_minutes_quickly`: open `c0-hvjV2A5Y` through `Factory`, call `open(id, loaded, 90 min)`, and read 64 KiB within 5 s.
  
  Run each by exact name, for example `cargo test -p music --lib live_tests::youtube_video::live_search_finds_the_boiler_room_channel -- --ignored --exact`. Expected: PASS.
- [ ] **Step 7: Run the app.** `cargo run --locked --package sonora`. Sign in as a guest to "YouTube", search "boiler room", play a set and jump to 1:30:00. The audio starts within a few seconds. Then sign in and check the library tabs.
- [ ] **Step 8: Run** `cargo fmt`, `cargo test --workspace` and `cargo clippy --workspace --all-targets`. Expected: PASS and clean.
- [ ] **Step 9: Commit (after approval):** `feat(sonora): offer youtube as a provider`

---

### Task 11: pin the fork

**Files:**
- Modify: `Cargo.toml` (`ytmusic` line `:173`, remove the `[patch]`), `Cargo.lock`

- [ ] **Step 1: Ask the user** before pushing the fork branch: `git -C ~/Documents/github/ytmusic-rs push origin feat/web`. Name the remote `origin` (`jorranMLGN/ytmusic-rs`) and say that the branch is new.
- [ ] **Step 2: Repoint.** `ytmusic = { git = "https://github.com/jorranMLGN/ytmusic-rs", rev = "<pushed head>" }`, and delete the `[patch]` block.
- [ ] **Step 3: Run** `cargo check --workspace --locked` after `cargo update -p ytmusic`. Expected: it finishes from the git source.
- [ ] **Step 4: Run** `python3 scripts/generate-notices.py` if `THIRD-PARTY.md` lists ytmusic's source url. Regenerate it, never edit it by hand.
- [ ] **Step 5: Commit (after approval):** `build(music): take ytmusic from the fork that reads youtube.com`
