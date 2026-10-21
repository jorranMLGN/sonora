# YouTube provider — Design

A second YouTube provider beside YouTube Music, for what YouTube Music does not carry: DJ sets,
live recordings, covers, podcasts and other long uploads. It reads plain youtube.com, plays the
audio through the shared engine, and treats a video as a track. This is project A. Watching the
video (large in fullscreen, small in a corner, both switchable) is project B and gets its own
spec; A only has to leave room for it.

## Why

The listener searches YouTube for a set or a live recording, finds it, and then has to leave
Sonora to play it. A provider for plain YouTube keeps that inside the app, with the queue,
scrobbling and the rest of the player behaving as they do for every other source.

## Goals

1. **Find**: search videos, channels and playlists on youtube.com, signed in or not.
2. **Play**: a video plays as audio through `music::engine`, seekable, in the queue like any
   track.
3. **Your account**: signed in, the library shows subscriptions, own playlists, Watch Later
   and liked videos.
4. **Ready for video**: the provider can later hand out an H.264 video stream for the same
   video id without reworking A.

## Non-goals

- Showing video. That is project B.
- Uploading, commenting, live chat, Shorts as a feed, community posts.
- Mobile (`crates/mobile`, `apps/ios`).
- Merging with the YouTube Music provider. Both stay separate providers with separate sign-ins.

## Anti-goals

- It must not feel like a stripped-down YouTube website inside Sonora. It is a music source:
  queue, radio, scrobbles, sorting and the library pages behave as for the other providers.
- No YouTube Data API, no API key, no client secret. Data comes from innertube, as YouTube
  Music's does.

## What exists (verified 2026-10-19)

- **YouTube Music provider** `crates/music/src/youtube/`, slug `"youtube"`:
  - webview sign-in through `accounts.google.com` with `COOKIE_DOMAIN = "youtube.com"`
    (`mod.rs:37-38`, `web_sign_in` at `:453`); pasted header (`SignIn::Secret`); Firefox-family
    `cookies.sqlite` (`browser.rs`);
  - brand accounts via `identities()` (`accounts.rs:33-56`), stored as
    `Saved::{Cookies{cookies, authuser, page_id}, Guest}` (`mod.rs:43`), cookies rotated every
    10 minutes (`keep_fresh`, `:261`);
  - `engine::Fetch` in `playback.rs`: `open_audio` with a 15 s timeout and two attempts, a rodio
    decoder over DASH fMP4, `segments.rs` and `trim` for seeking, `loudness` from `loudnessDb`,
    `gated` for sign-in-only videos, `reopen_to_seek = true`, `stream::Plain` as the body.
- **ytmusic** (`Cargo.toml:173`, our fork `sonorahq/ytmusic-rs` at `8b044c4`):
  - `Client::{Music, Web, Tv, VisionOs}`; non-Music clients hit `www.youtube.com/youtubei/v1/`
    with a WEB context and SAPISIDHASH for origin `https://www.youtube.com`;
  - public `execute(endpoint, Client::Web, payload)` returns raw JSON over the shared auth;
  - `open_audio`: VISIONOS guest formats first, then signed WEB_REMIX with PO token and
    `signatureTimestamp`; signature and n-param solved in rquickjs (`deobf.rs`);
  - audio only: `ciphered` and `audio_format` drop every non-`audio/*` format, `Client::Music`
    forces `isAudioOnly`, `AudioFormat` has no width, height or fps;
  - every typed parser targets WEB_REMIX shapes; helpers `find_renderers`, `thumbnails`,
    `runs_text` in `nav.rs`/`parse.rs` are generic.
- **PO tokens**: `music::potoken` and `state::potoken` mint in a hidden webview on
  `www.youtube.com`, so they serve plain YouTube unchanged.
- **Registration**: the provider list in `crates/sonora/src/main.rs:103-110`; `music::tag::SLUGS`
  (`crates/music/src/tag.rs:19`) is what `tag::split` and `Shelf::of` resolve; a feature in
  `crates/music/Cargo.toml` and the `mod` in `crates/music/src/lib.rs`;
  `views::shared::provider_logo` (`crates/views/src/shared/mod.rs:48`) and a brand mark in
  `assets/icons/common/` (the one folder that never follows the pack, `docs/architecture.md:443`;
  YouTube Music's is `youtubemusic.svg`). `music::tagged::new(slug, api)` and
  `Destination::Library(slug, tab)` are generic.
- `LibraryTab` is `Songs`, `Albums`, `Playlists`, `Artists` (`crates/router/src/lib.rs:14`). The
  sidebar lists `LibraryTab::ALL` for every provider (`crates/views/src/chrome/sidebar_left.rs:606`);
  nothing hides a tab per provider.
- **Search** has `MusicApi::{search, search_albums, search_playlists}`
  (`crates/music/src/lib.rs:325-331`) and no artist search: `state::search` builds artist hits from
  `artist_refs` on track and album results (`crates/state/src/search.rs:875-950`), and `Parts` has
  no artists flag (`:123-127`). `tagged/mod.rs` forwards every trait method explicitly.
- **Streaming** (`crates/music/src/stream.rs`): one in-order download into one spool per track,
  `PATIENCE = 20 s` for a read past what has arrived (`:35`), `CEILING = 256 MiB` per track
  (`:39`). The fork's `AudioStream` reads 1 MiB ranges in sequence. Seeking past the download
  waits for it.
- **ytmusic-rs**: `sonorahq/ytmusic-rs` is read-only for this account; `jorranMLGN/ytmusic-rs`
  does not exist yet.
- OpenH264 decodes YouTube's H.264 (spike 2026-10-19): Main (`avc1.4D…`, 144p–720p) and High
  (`avc1.64…`, 1080p) with B-frames, zero errors, frames equal to ffmpeg's (41–44 dB PSNR),
  114–131 fps for 1080p on one thread. YouTube has no H.264 above 1080p.

## Decisions

1. **A new provider module `music::youtube_video`**, slug `"youtube-video"`, display name
   "YouTube", behind a `youtube-video` cargo feature that is in `default` and depends on
   `youtube`. It reuses the YouTube Music module's sign-in, account picking, cookie rotation,
   PO token and playback code by calling into it or moving shared parts to a `youtube::common`
   submodule. It does not copy them. (Name of the slug and module: my call.)
2. **Own sign-in and own credentials dir** (`credentials::dir("youtube-video")`). Signing in to
   one YouTube provider does not sign in the other. (My call: the two are separate rows in the
   sidebar and the account picker, and a listener may want only one.)
3. **Signed out works.** Search, channel pages, playlists and playback work as guest.
   Library tabs and the home feed need sign-in and show the usual sign-in prompt.
4. **A video is a `Track`**: title is the video title, artist is the channel (its id is the
   channel id), cover is the best thumbnail, duration from `lengthSeconds`. No album.
5. **A channel is an `Artist`**. The artist page lists the channel's uploads as its tracks.
   Its `albums` stay empty: a channel's playlists are not shown on the artist page in A, because
   that row opens the album page, which a playlist does not fit. Subscribing is following
   (`Capabilities::follow_artists`). (My call.)
6. **Library, `Shape::Saved`:**
   - Songs: liked videos (`VLLL`);
   - Artists: subscriptions (`FEchannels` / `FEsubscriptions`);
   - Playlists: own and saved playlists, with Watch Later (`VLWL`) pinned first;
   - Albums: the tab stays and shows its normal empty state. Hiding a tab per provider is out
     of scope. (My call.)
7. **Search** fills the existing kinds: videos as songs, channels as artists, playlists as
   playlists. Albums return empty. Channels come from a **new `MusicApi::search_artists`**
   (default: empty), forwarded in `tagged/mod.rs`, with an artists part in `state::search` that
   merges its hits with the `artist_refs`-derived ones. This is phase 2 of the search filters
   spec for artists; other providers may implement it later, YouTube does in A. (Decided by
   the user.)
8. **Home** (signed in): the `FEwhat_to_watch` feed mapped to the existing home shelves.
   Signed out: nothing, as other providers without a guest home.
9. **Radio**: the related videos of the playing video fill the queue (`Capabilities::radio`).
10. **Saving**: liking a video is `set_track_saved`; adding to a playlist and Watch Later use
    the existing playlist mutation methods.
11. **Parsers for youtube.com** (`videoRenderer`, `compactVideoRenderer`, `lockupViewModel`,
    `channelRenderer`, `playlistRenderer`, `gridPlaylistRenderer`, continuations) go into a
    personal fork, **`jorranMLGN/ytmusic-rs`**, forked from `sonorahq/ytmusic-rs` at `8b044c4`,
    as a `web` module next to the WEB_REMIX parsers, using the generic helpers, and are called
    through `Client::Web`. `Cargo.toml` repoints `ytmusic` to that fork at a pinned `rev`; the
    sonora PR carries that change. The fork stays the one place that knows innertube shapes, and
    its changes can go upstream to sonorahq later. (Decided by the user.)
12. **Audio** goes through the existing `open_audio` path with a `videoId` from plain YouTube.
    `loudness` uses `loudnessDb` when present.
12a. **Seek anywhere at once.** A seek past what has been downloaded restarts the download at
    the byte range for the target instead of waiting for the in-order spool. This lives in
    `music::stream` (a ranged restart any `Body` whose source supports ranges can use), not in
    the YouTube provider, so every provider and project B benefit. The 256 MiB ceiling applies
    per spooled run instead of per track, so a four-hour podcast plays. The YouTube `Fetch`
    keeps `reopen_to_seek` and `segments.rs` for finding the fMP4 fragment that holds the
    target time. (Decided by the user.)
12b. **No dedup** between `youtube:` and `youtube-video:` hits for the same video: each provider
    keeps its own rows, as for any song that exists on two services. **Scrobbles** go out as for
    other providers, channel as artist and video title as title. (My call.)
13. **Room for B**: in the fork, `ciphered` and the format filtering become generic over a
    predicate, and a `VideoFormat` (itag, mime, codecs, width, height, fps, bitrate, url,
    init and index ranges) is parsed next to `AudioFormat`. A does not expose it through
    `MusicApi` yet; B adds that seam.
14. **Logo**: YouTube's play-button mark as one file, `assets/icons/common/youtube.svg`,
    distinct from `youtubemusic.svg`.
15. **Strings** for the provider name, sign-in copy and empty states go into
    `assets/i18n/en-US/main.ftl`.

## Left to the builder

- Whether shared YouTube code moves into `youtube::common` or is reached through `pub(crate)`
  items of `youtube`.
- Which innertube endpoints and continuation tokens back each list, and page sizes.
- How Shorts in search results are treated (dropped or kept as tracks).
- Mapping details for live streams and premieres that have no fixed length: skipping them in
  lists is acceptable.
- The test split: parsers get fixture tests in the fork from recorded JSON, the provider gets
  `#[cfg(test)]` tests in the file they cover. Live tests are `#[ignore]`d and read-only.

## Done when

- Signed out: searching "boiler room" lists videos, the Boiler Room channel and playlists;
  playing a two-hour set starts within a few seconds, and a jump to 1:30:00 plays within a few
  seconds without waiting for the download to get there.
- Signed in: subscriptions, playlists with Watch Later, and liked videos show in the library;
  liking a video and adding it to Watch Later work.
- `cargo clippy --workspace --all-targets` stays clean and `CHANGELOG.md` has an `Added` entry.
