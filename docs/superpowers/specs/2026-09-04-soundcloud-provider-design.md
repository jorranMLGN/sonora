# SoundCloud provider — design

Date: 2026-09-04
Status: approved, pending implementation plan
Target: upstream PR to `nolight132/sonora`
Branch: `feat/soundcloud-provider`

## Goal

Add SoundCloud as a third streaming provider next to Spotify and YouTube Music, at
feature parity with the other two wherever the platform allows it: search, playback,
likes (read and write), sets (read, create, edit, delete), followed artists, artist
pages, and related-track radio.

This document is a working design. It is not part of the upstream PR.

## Non-goals

- Lyrics. SoundCloud exposes none; `MusicApi::track_lyrics` keeps its `Ok(None)` default.
- Tag editing. `track_tags` / `set_track_tags` keep their `bail!` defaults.
- Per-track loudness normalisation. See "Known limitations".
- Multi-provider-at-once (upstream issue #405). The design must not obstruct it, but
  does not implement it.

## API surface

SoundCloud's internal v2 API (`api-v2.soundcloud.com`) — the endpoints the web player
itself uses — authorised with a `client_id` harvested from the web player bundle.

### Why not the official public API

App registration reopened and `api.soundcloud.com` is documented and stable, but:

1. The OAuth 2.1 token exchange requires a `client_secret` even with PKCE. Sonora is
   GPL and ships binaries; an embedded secret is extractable. The alternative — each
   user registers their own app — is a barrier neither Spotify nor YouTube imposes.
2. `GET /tracks/{id}/stream` yields 128 kbps MP3. That is below what the SoundCloud
   website itself serves, in a client that advertises gapless playback and
   normalisation.
3. Rate limits are per app, therefore shared across every Sonora user.

The v2 route also matches the house precedent. `CLAUDE.md` on the Spotify provider:
"A developer-app client id will be refused at session connect … Don't 'fix' auth by
swapping in a registered app id." Sonora uses Spotify's own desktop client id and
YouTube's innertube. A registered-app SoundCloud provider would be the only one built
differently, and the only one with worse audio than its own website.

### Cost

The v2 API is undocumented and can change without notice. The `client_id` rotates.
Both are accepted risks, mitigated by harvest-and-cache with a re-harvest on 401 and by
keeping wire parsing tolerant of unknown fields.

## Unverified assumptions

A hands-on probe of the v2 API was not run. Everything below is inference from the
platform's public behaviour and must be confirmed before implementation proceeds.
Confirming them is step 1 of the implementation plan; each one that fails changes the
design.

| # | Assumption | If false |
| - | ---------- | -------- |
| A1 | `client_id` is extractable from the `a-v2.sndcdn.com` player bundle | The whole approach fails; fall back to the official API and revisit the design |
| A2 | Playlists carry a `set_type` of album/ep/single/compilation | `Album` methods return empty; albums become a non-feature |
| A3 | `GET /tracks/{id}/related` exists | `track_radio` returns empty |
| A4 | `/mixed-selections` backs a home feed | `home()` keeps its `HomeFeed::default()` |
| A5 | `/charts?genre=` backs genre browsing | `genres()` returns empty |
| A6 | `media.transcodings[]` offers a `progressive` variant | HLS-only path in `stream.rs` becomes mandatory rather than a fallback |
| A7 | An `oauth_token` works as `Authorization: OAuth <token>` | Library access needs a different credential; `Anonymous` still works |
| A8 | `ytmusic::browser::cookies()` is host-parameterisable | `SignIn::Browser` moves to a follow-up PR; ship `Anonymous` + `Secret` |

## Architecture

The provider abstraction already supports this. `crates/music/src/lib.rs` defines
`MusicProvider` (sign-in lifecycle), `MusicApi` (data access), and
`PlaybackFactory`/`Player` (audio). `CLAUDE.md`: "Only `sonora/src/main.rs` names a
concrete provider." Adding a provider is therefore a new module plus one registration
line.

### Module layout

`crates/music/src/soundcloud/`, modelled on `youtube/`:

| Module | Responsibility |
| ------ | -------------- |
| `mod.rs` | `SoundCloudProvider`: `MusicProvider` impl, wires client and playback factory |
| `auth.rs` | `client_id` harvest and cache, token storage, `restore` / `sign_out` |
| `client.rs` | `SoundCloudClient`: the `MusicApi` impl; delegation only, no HTTP |
| `wire.rs` | serde JSON to `models`; `artwork_url` size substitution |
| `search.rs` | `/search/tracks`, `/search/playlists`, `/search/users` |
| `library.rs` | likes, followed artists, saved sets |
| `playlists.rs` | sets: read, create, rename, delete, add/remove track, visibility |
| `users.rs` | artist profile, top tracks, related artists |
| `playback.rs` | transcoding selection, byte source, rodio engine |
| `stream.rs` | HLS m3u8 fetch and segment concatenation |

Roughly 2100 lines, the same order as the YouTube provider's 1994.

`client.rs` holds no HTTP calls. It implements the trait and delegates to a focused
module, following the pattern `CLAUDE.md` states for `LibrespotClient`.

## Model mapping

| Sonora model | SoundCloud source | Notes |
| ------------ | ----------------- | ----- |
| `Track.id` | numeric track id as a string | No prefix; does not collide with `is_local_id()` |
| `Track.playable` | `streamable && policy != "BLOCK"` | `policy == "SNIP"` (Go+ 30s preview) is also `false` |
| `Track.playcount` | `playback_count` | Direct |
| `Track.album` / `album_id` | empty | A track does not know its set |
| `Track.explicit` | absent | `false` |
| `Track.disc_number` | absent | `1` |
| `Album` | playlist with `set_type` in album/ep/single/compilation | Maps onto the existing `ReleaseType` |
| `Playlist` | playlist with any other `set_type` | |
| `SavedArtist` | `/me/followings` | |
| `track_radio` | `/tracks/{id}/related` | |

`set_type` is why albums are a real feature here rather than a stub. `models.rs`
already defines `ReleaseType::{Album, Single, Compilation, Ep}`; one `match` turns
`saved_albums()` and `album()` into genuine implementations.

The asymmetry to handle: a track fetched on its own has no album, but the same track
fetched through `album_tracks()` does. `album_tracks()` fills `album` and `album_id`
from the set it just read. Listings that mix both sources will show album names
inconsistently — accepted, and the same behaviour the SoundCloud website has.

## Authentication

`sign_in_options()` returns:

| `SignIn` | Behaviour | Library access |
| -------- | --------- | -------------- |
| `Anonymous` | Harvest `client_id` only; browse and play public content | No |
| `Secret` | User pastes their own `oauth_token` | Yes |
| `Browser(name)` | Read the cookie from a Firefox-family browser | Yes, subject to A8 |

Credential cache: `dirs::config_dir()/sonora/soundcloud`, mirroring `youtube/mod.rs`.

`client_id` lifecycle: harvest, cache to disk with a timestamp, use; on a 401 or 403,
re-harvest once; on a second failure, surface `SignInProblem::Network`.

### Required fix to shared code

`crates/music/src/lib.rs` hardcodes Spotify into three of the six `SignInFailure`
messages: "the account has no Spotify Premium", "Spotify could not be reached",
"Spotify refused the session". A SoundCloud network failure would render as "Spotify
could not be reached".

Two providers never exposed this because YouTube does not use those variants. The
third one does. This PR carries the fix, and the fix is provider-neutral wording:
"the account has no premium subscription", "the service could not be reached", "the
service refused the session". Neutral wording changes no call sites and no trait
signature, so it stays a contained edit to one `Display` impl. Threading a provider
name through `SignInFailure` would produce better messages but widens the PR into
every construction site, and is deliberately not done here.

No other refactoring is in scope.

## Playback

`playback.rs` mirrors `youtube/playback.rs`: a dedicated thread, a `Command` channel
(`Load`, `Preload`, `Play`, `Pause`, `Seek`, `Gain`), rodio driven through
`crate::audio::{Output, SmoothGain, Trimmed, Volume}`, and a `Spectrum`.

Byte path:

1. `GET /tracks/{id}` for `media.transcodings[]`.
2. Prefer a `progressive` transcoding; otherwise HLS.
3. Resolve the transcoding URL (authorised with `client_id`) to a CDN URL.
4. Progressive: read the body into `Cursor<Vec<u8>>` and hand it to rodio — what the
   YouTube engine already does.
5. HLS: `stream.rs` fetches the m3u8, downloads each segment, concatenates, then the
   same `Cursor`. Not adaptive; one quality, chosen up front.

Preload resolves and prefetches the next track's bytes.

Gapless reuses `youtube/trim.rs` — stripping encoder padding behaves the same for
MP3 and Opus.

### Known limitations

- **No per-track normalisation.** The v2 API carries no loudness metadata, so
  `PlaybackConfig.normalisation` can honour only the global gain. Spotify and YouTube
  do better. Measuring loudness during playback is possible but out of scope.
- **Go+ tracks are not playable.** `policy == "SNIP"` returns a 30-second preview.
  Marking such tracks `playable: false` is honest; playing a truncated track is not.

## UI, assets, i18n

| File | Change |
| ---- | ------ |
| `assets/icons/common/soundcloud.svg` | Simple Icons mark, CC0, same source as `spotify.svg` |
| `assets/icons/common/LICENSE` | Add the mark to the list of brand marks |
| `crates/views/src/shared/mod.rs` | `"soundcloud" => "icons/soundcloud.svg"` in `provider_logo` |
| `assets/i18n/en-US/main.ftl` | New sign-in strings |
| `crates/sonora/src/main.rs` | Register `SoundCloudProvider` |

`state/src/settings.rs` keys resume state by slug and needs no change.

The nine non-English `main.ftl` files are left to translators; the README coverage
table regenerates.

## Testing

- **Unit, no network.** `wire.rs` conversions and `client_id` extraction are pure
  functions driven by fixtures. `youtube/auth.rs` sets the precedent with eight unit
  tests over pure parsing. These are written first.
- **Live.** A `crates/music/src/live_tests/` module behind `#[cfg(test)]`, alongside
  the existing three, exercising search, a library read, and a stream resolve against
  a real account.
- **Manual.** Sign in all three ways, play a public track, play a track from a set,
  like and unlike, create and delete a set, verify the provider logo, and confirm a
  forced network failure no longer names Spotify.

Errors use `anyhow` with lowercase `.context("cannot …")`, per `CLAUDE.md`.

## Upstream process

No SoundCloud issue or PR exists on `nolight132/sonora`. A change of this size should
open an issue for maintainer buy-in before the code lands, particularly because the
v2-API choice is a judgement call the maintainers may want to make themselves.

The repository's AI policy requires that the reasoning and motivation in that issue and
in the PR come from the contributor, not from a model. This document is input to that,
not a substitute for it.
