# Sonora for iPhone — Design

A sideloaded iPhone app that plays Spotify, YouTube Music and SoundCloud through the same Rust
`music` crate the desktop app uses, with a React Native (Expo) interface. Built entirely on
GitHub's macOS runners and installed with SideStore on a free Apple ID; no Mac, no paid account.

## What the spike proved (2026-10-13, branch `spike/ios`)

- `cargo build --target aarch64-apple-ios` of `music` works on `macos-latest` once the
  desktop-only parts sit behind features (`spotify`, `youtube`, `video`, `opus`, all default on).
  The desktop dependency tree is unchanged.
- A Rust staticlib linked into an Expo SDK 57 app through a local Expo module (Swift, Clang module
  map, `@_implementationOnly`) builds unsigned with `xcodebuild CODE_SIGNING_ALLOWED=NO`; the
  `Payload/*.app` zip installs through SideStore.
- `cpal` → CoreAudio output is audible on the device; SoundCloud guest search and playback through
  `music::soundcloud`'s own `PlaybackFactory` work on the device.

## Goals

1. Spotify, YouTube Music and SoundCloud signed in at once, as on the desktop.
2. Home, Search, Library (songs, albums, artists, playlists per provider), album / playlist /
   artist pages, a full player, a queue, and an Accounts screen.
3. Background playback, lock-screen and Control Center controls, Now Playing artwork, correct
   behaviour on calls, Siri and headphone changes.
4. The desktop app and its crates `state`, `views`, `ui` stay untouched, so merging upstream never
   conflicts with the phone.

## Non-goals

- App Store distribution, TestFlight, a paid Apple account.
- Apple Music, Deezer, Tidal or any Widevine provider (no CDM on iOS), local files, Subsonic,
  jam, canvas, lyrics, scrobbling, equalizer, Discord.
- Android (the Expo app may later build for it; nothing here targets it).

## Decisions

1. **No GPUI-free extraction of `state`.** `state` is upstream's core code; carving a `core` crate
   out of it would turn every `git merge upstream/main` into a large conflict. The phone gets its
   own orchestration layer in Rust, `crates/mobile`, written against `music`'s public traits only.
2. **Orchestration lives in Rust, not TypeScript.** Queue, shuffle, repeat, sessions, library
   loading and playback ownership are in `crates/mobile`, so the JS side only renders state and
   sends intents. One source of truth on the phone, testable with `cargo`.
3. **UniFFI (proc-macro mode)** replaces the spike's C ABI: typed records, async functions on the
   crate's own tokio runtime, and a callback interface for events. Bindings are generated in CI
   with `uniffi-bindgen` into the Expo module; the Rust side ships as an XCFramework (device
   slice; a simulator slice is optional).
4. **Rust plays the audio.** Every provider plays through its own `PlaybackFactory`/`Player`
   (Spotify: librespot through `spotify::playback`; YouTube and SoundCloud: `music::engine`),
   output through `cpal`. Swift owns only the audio session, Now Playing and remote commands.
5. **Ids stay tagged.** `crates/mobile` wraps every provider in `music::tagged`, so an id names its
   provider wherever it travels (queue, persistence, JS) and routing needs no table.
6. **Sign-in uses the providers' own prompt flow.** `MusicProvider::sign_in` already reports a
   `SignInPrompt`; the phone answers each prompt natively:
   - `Url(url)` (Spotify OAuth): the app opens the URL in an in-app browser
     (`SFSafariViewController`). librespot-oauth's loopback listener on `127.0.0.1:8989` runs inside
     the app and receives the redirect while the app is in the foreground; the browser closes when
     the session reports signed in. If iOS blocks the loopback redirect, fall back to
     `ASWebAuthenticationSession` with the same URL and read the code from its callback URL.
   - `Secret` (YouTube cookie sign-in): a `WKWebView` over a non-persistent data store loads
     `WebSignIn::url`; once the cookies named in `WebSignIn::proof` exist on `WebSignIn::domain`,
     the cookie header is sent back as the prompt's input. Same contract as the desktop `webview`
     crate.
   - `Accounts`/`Code`: rendered as native pickers / a code screen.
7. **YouTube proof-of-origin tokens** are minted by a hidden `WKWebView` that runs the desktop's
   `crates/state/src/potoken.js` against the same page, driven by `music::potoken::{wanted, give,
   failed}` through the bridge. Nothing in `music` changes for this.
8. **Storage in the sandbox.** Credentials (`<provider>/credentials.json`), the SoundCloud client-id
   cache and `mobile.sqlite` (queue, settings, last library snapshot) live under the app's
   Application Support directory, passed in at `init`. SQLite is bundled on iOS.
9. **UI in Expo Router** with a tab bar (Home, Search, Library), a mini player above it, a full
   player sheet with the queue, and Accounts under Library. Dark theme only. Strings come from
   the repo's Fluent files (`assets/i18n/<locale>/main.ftl`) through `@fluent/bundle`, bundled into
   the JS at build time; phone-only keys use a `mobile-` prefix in `en-US`; other locales may lag,
   as on the desktop.
10. **CI builds every push** to `feat/ios` (and later `main`) on `macos-latest`, caches cargo and
    CocoaPods, and uploads `sonora.ipa`. The spike workflow is replaced. Desktop CI is untouched
    and must stay green.

## Architecture

```
apps/ios/                        Expo SDK 57, TypeScript, Expo Router
  app/                           screens (tabs, album, playlist, artist, player, queue, accounts)
  src/store.ts                   one zustand store fed by bridge events
  src/i18n.ts                    @fluent/bundle over assets/i18n/*.ftl
  modules/sonora-core/ios/       Swift: generated UniFFI bindings, SonoraCoreModule (Expo module),
                                 AudioSession (interruptions, routes), NowPlaying (info center,
                                 remote commands), SignIn (Safari / WKWebView), PoTokenMinter
crates/mobile/                   staticlib + UniFFI
  src/lib.rs                     #[uniffi::export] surface
  src/core.rs                    Core: runtime, providers, sessions, events
  src/queue.rs                   Queue: past / current / upcoming, shuffle, repeat
  src/player.rs                  owns one active Player, routes intents, preloads, reports position
  src/library.rs                 per-provider saved tracks / albums / artists / playlists, paged
  src/store.rs                   mobile.sqlite: queue + settings + snapshots
crates/music                     unchanged except features and an at(dir) constructor per provider
```

### Bridge surface (Rust → JS through Swift)

```text
init(data_dir, listener: EventListener)
providers() -> [ProviderInfo { slug, name, signed_in, account }]
sign_in(slug, option) / answer(slug, input) / sign_out(slug)
home() -> [Section]          search(query) -> SearchResults { tracks, albums, artists, playlists }
library(slug, part, page) -> Page
album(id) / playlist(id) / artist(id) -> detail records
play(list: [TrackId], index, origin)   toggle() / next() / previous() / seek(ms)
set_shuffle(bool) / cycle_repeat()     enqueue_next(ids) / enqueue_last(ids) / remove(index)
set_saved(id, bool)
audio_interrupted() / audio_resumed(should_resume) / route_changed()
po_token_wanted() -> Option<String>  /  po_token(binding, token?)

EventListener: on_state(PlayerState), on_position(ms), on_queue(QueueSnapshot),
               on_session(slug, SessionState), on_prompt(slug, Prompt), on_error(String)
```

Records are flat (`id`, `title`, `subtitle`, `artwork`, `duration_ms`, `explicit`, `provider`),
mapped from `music` models in `crates/mobile`; JS never sees a `music` type.

## Behaviour

- **Startup:** `init` restores every stored session in parallel, restores the queue paused at its
  last position, and emits the providers and the queue before any network answer.
- **Playback ownership:** one provider engine plays at a time; starting a track of another provider
  stops the current one first. The next track is preloaded when the current one has 20 s left.
- **Interruptions:** on `AVAudioSession.interruptionNotification` begin → pause; end with
  `.shouldResume` → resume. On route change (`oldDeviceUnavailable`, i.e. headphones out) →
  pause, and the engine is rebuilt on the next play so cpal opens the new output.
- **Now Playing:** title, artist, album, artwork, duration, elapsed and rate are published on every
  state change and every 5 s while playing; remote play, pause, toggle, next, previous and seek
  are wired.
- **Failure:** a track that cannot be played is skipped with an `on_error` toast; three in a row
  stops playback. A provider whose session fails to restore shows "Sign in again" on Accounts.
- **Sign-out** drops that provider's credentials, its library snapshot and its tracks from the
  queue.

## Milestones (each its own plan, executed and reviewed before the next)

1. **Core** — `crates/mobile` on UniFFI with providers, sessions (SoundCloud guest first), search,
   queue, player, store; XCFramework + generated Swift in CI; the spike UI ported to the new
   bridge. Done when: search + play + next/previous + queue survive an app restart on the device.
2. **Platform audio** — interruptions, route changes, Now Playing, remote commands. Done when:
   lock screen controls work and music continues with the screen off for 30 minutes.
3. **Sign-in** — Spotify (Url prompt), YouTube (Secret prompt + PO token minter), SoundCloud
   account; Accounts screen. Done when: all three sign in on the device and play a track.
4. **Library and pages** — Home, Library tabs per provider, album / playlist / artist pages,
   saved toggles. Done when: each provider's library lists and plays on the device.
5. **UI polish and distribution** — player sheet, queue editing, theme, i18n, app icon from
   `assets/icon.svg`, CI on `main`, README section on sideloading.

## Verification

Per milestone: `cargo check/clippy/test --workspace` and `cargo fmt --check` green on Linux
(desktop untouched), the iOS workflow green, and the user's device check listed under the
milestone. No new tests unless the user asks (house rule); `crates/mobile` logic is checked by
the device runs and by clippy.

## Risks

- **Spotify loopback redirect on iOS** may be refused or the listener suspended; fallback named in
  decision 6. If both fail, Spotify sign-in moves to a device-code style flow if librespot offers
  one, else Spotify ships without sign-in in milestone 3 and the gap is reported.
- **PO token minting in WKWebView** may not pass Google's attestation; then YouTube plays only
  what a cold-start token allows, and the gap is reported.
- **SideStore 7-day expiry and the 3-app limit** are inherent to a free Apple ID.
- **Memory:** `music::engine` holds a whole track in memory; long mixes on the phone may be killed.
  Milestone 2 measures it on the device.
