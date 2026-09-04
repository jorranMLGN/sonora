# SoundCloud provider — verification spike findings

Spike performed 2026-09-04 against the live `api-v2.soundcloud.com` API using a
`client_id` harvested from the public web player bundle (no personal login used
except where noted). All commands and raw evidence below; fixtures captured under
`crates/music/src/soundcloud/fixtures/`.

## Client ID harvest (A1)

```
curl -sL https://soundcloud.com/discover -A "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36" -o /tmp/sc.html
grep -oE 'https://a-v2\.sndcdn\.com/assets/[^"]+\.js' /tmp/sc.html | sort -u
```

returned 8 bundle URLs (`0-a583ec81.js` … `59-ac0a49ce.js`). Fetching each and
grepping for `client_id` (`grep -oc 'client_id' <file>`) showed the identifier
assigned in `55-70f3b3d1.js`:

```
dcloud.com",client_application_id:46941,client_id:"Pb72ranhoyt6gw7hM7TkzUItXlMWSNSo",client_is_expiring:!1,env:"production",...
```

Confirmed live by calling `GET /tracks/293?client_id=Pb72ranhoyt6gw7hM7TkzUItXlMWSNSo`
→ HTTP 200 with a full track body. The same `client_id` was reused for every
subsequent probe in this spike.

## Assumption table

| # | Assumption | Verdict | Evidence | Consequence |
|---|---|---|---|---|
| A1 | A `client_id` can be harvested from the public web player bundle and used against `api-v2.soundcloud.com` | **CONFIRMED** | `client_id:"Pb72ranhoyt6gw7hM7TkzUItXlMWSNSo"` found in `a-v2.sndcdn.com/assets/55-70f3b3d1.js`; `GET /tracks/293?client_id=...` → HTTP 200 | Approach is viable; no fallback to the official OAuth-only API needed. |
| A2 | Albums vs. plain playlists are distinguished by `set_type` on `/playlists/<id>` | **CONFIRMED** | `/playlists/2220481535` ("Yo Te Canto Debut Album") → `set_type: "album"`, `is_album: true`. `/playlists/2293462332` (a plain playlist) → `set_type: ""` (empty string, present but empty), `is_album: false` | `saved_albums`/`album`/`album_tracks` can filter on `set_type == "album"` (or `is_album == true`, which is redundant but present); treat `set_type: ""` as "not an album", not as a missing field. |
| A3 | `/tracks/<id>/related` is a usable endpoint | **CONFIRMED** | `GET /tracks/293/related?client_id=...` → HTTP 200, body starts with a `collection` of full track objects | Related-tracks feature can be built as designed. |
| A4 | `/mixed-selections` is a usable endpoint | **CONFIRMED** | `GET /mixed-selections?client_id=...` → HTTP 200, body is a `collection` of selection objects (e.g. `urn: "soundcloud:selections:buzzing"`, `title: "Artists to watch out for"`) with a nested `items` collection | Home/discovery feed feature can be built as designed. |
| A5 | `/charts?kind=trending&genre=soundcloud:genres:house` is a usable endpoint | **UNVERIFIED / PARTIAL** | `GET /charts?kind=trending&client_id=...` (no genre) → HTTP 200 with a real trending list. Adding `genre=soundcloud:genres:house` (both raw and percent-encoded `soundcloud%3Agenres%3Ahouse`), and also `genre=soundcloud:genres:electronic`, all → HTTP 404 `{}`. `/charts/genres` also → 404. | The base `/charts?kind=trending` (and presumably `kind=top`) works as designed. The exact genre-filter URN format in the spec is wrong or the genre catalog has moved; a follow-up spike should harvest real genre URNs (e.g. from `/mixed-selections` or track `genre` fields) before Task 8 wires up genre-filtered charts. Ship the un-filtered chart first; treat genre filtering as its own small follow-up. |
| A6 | At least one `media.transcodings[]` entry has `format.protocol == "progressive"` | **CONFIRMED** | Track 293 (`Flickermood`) has 5 transcodings; the last one is `{"preset":"mp3_0_0","format":{"protocol":"progressive","mime_type":"audio/mpeg"},"quality":"sq","is_legacy_transcoding":true}` | Progressive playback path is viable as designed; note it is marked `is_legacy_transcoding: true` and coexists with `hls`-protocol entries for the same preset — the picker must filter on `format.protocol`, not assume one entry per preset. |
| A7 | `/me` with `Authorization: OAuth <token>` authenticates a personal session | **UNVERIFIED** | Not probed — no personal `oauth_token` was supplied, and per task instructions no browser profile/cookie store/keychain was searched for one. A future run needs: a real signed-in SoundCloud `oauth_token` (from a browser's `Authorization` header on an XHR to `api-v2.soundcloud.com`, supplied by the user), then `curl -s "https://api-v2.soundcloud.com/me?client_id=$CID" -H "Authorization: OAuth $TOKEN"` to check (a) HTTP 200 and (b) whether `client_id` is still required alongside the header. | The authenticated path (liked tracks, own playlists, follows) cannot be built or tested until this is confirmed. Task order should treat `SignIn::OAuthToken` as blocked on a follow-up spike with a supplied token, independent of everything else in this plan. |
| A8 | `ytmusic::browser::cookies()` can be reused (or is host-parameterizable) for a SoundCloud browser sign-in | **FALSE** | `~/.cargo/git/checkouts/ytmusic-rs-2639ca41da9a171c/0b2b13a/src/browser.rs`: `WANTED` is a hard-coded list of YouTube/Google cookie names (`SID`, `__Secure-1PSID`, `VISITOR_INFO1_LIVE`, `LOGIN_INFO`, …); `firefox_cookies` queries `moz_cookies WHERE host LIKE '%youtube.com'` — the host filter is a string literal, not a parameter. `chromium_cookies` reads the same `WANTED` list. There is no way to pass a different host or cookie set in. | `SignIn::Browser` cannot call into `ytmusic::browser::cookies()` as-is. Either write a small SoundCloud-specific cookie harvester in `crates/music/src/soundcloud/` (same sqlite-read technique, different host/cookie names), or defer `SignIn::Browser` entirely and ship only `SignIn::OAuthToken` for v1. Recommend deferring: it is a genuinely separate piece of work, not a one-line parameterization. |

## Fixtures captured

All pretty-printed with `python3 -m json.tool`, all from the anonymous
`client_id` (no personal token used).

- `crates/music/src/soundcloud/fixtures/track.json` — track 293, "Flickermood" by forss. `policy: "MONETIZE"`, `streamable: true`. Has 5 `media.transcodings[]` entries (2×`hls` aac, 1×`hls` abr, 1×`hls` mp3, 1×`progressive` mp3).
- `crates/music/src/soundcloud/fixtures/playlist_album.json` — playlist 2220481535, "Yo Te Canto Debut Album". `set_type: "album"`, `is_album: true`, `track_count: 17`.
- `crates/music/src/soundcloud/fixtures/playlist_set.json` — playlist 2293462332, a plain (non-album) playlist. `set_type: ""`, `is_album: false`, `track_count: 7`.
- `crates/music/src/soundcloud/fixtures/user.json` — user 293 ("Soundssupreme").
- `crates/music/src/soundcloud/fixtures/search_tracks.json` — `GET /search/tracks?q=test&limit=5`, 5 results.

**Not captured as a fixture** (found during Step 3, recorded here as evidence
only, not saved to disk): track 523994058, "thank u, next" — `policy: "SNIP"`,
`monetization_model: "SUB_HIGH_TIER"`, `streamable: true`, both its
`media.transcodings[]` entries have `snipped: true`. This is the restricted-policy
value `Track::playable` must reject; no bare `SNIP` fixture was written because
the design brief only asked for track/playlist_album/playlist_set/user/search_tracks
fixtures, but the field values above should be used directly when writing the
`policy` handling logic and its tests.

## Redactions

`track.json` contains a `track_authorization` field — a per-track, per-anonymous-session
signed token issued by the API itself (not a personal credential; it is required
to fetch the track's stream URLs and is returned to every anonymous caller of this
endpoint). It was left in the fixture as captured since it is intrinsic to the
resource shape that Task 4/5's serde structs need to parse, not a secret tied to
a personal account. No `oauth_token`, cookie value, or other personal credential
appears in any fixture or in this document.

## Real field names vs. the design's mapping table

- `set_type` is present and is an **empty string** `""` for non-album playlists,
  not absent/null. Code that checks "is this an album" must compare against the
  literal string `"album"`, not merely check for the field's presence.
- `is_album: bool` is also present alongside `set_type` and is redundant with it
  (`true` exactly when `set_type == "album"` in both captured fixtures) — either
  can be used, but `set_type` is the field named in the design doc.
- `media.transcodings[]` entries carry `is_legacy_transcoding: bool`; the
  `progressive` entry observed had `is_legacy_transcoding: true` and shared its
  `preset` (`mp3_0_0`) with an `hls`-protocol entry for the same underlying
  encode. Selection logic must filter by `format.protocol == "progressive"`
  rather than assuming a 1:1 preset-to-protocol mapping.
- `policy` observed values: `MONETIZE` (normal, playable) and `SNIP` (Go+/
  restricted, not fully playable) — matches the design's expectation of a
  non-`ALLOW` restricted value, except the actual "everything is fine" value is
  `MONETIZE`, not `ALLOW`, in every fixture captured. The design's table should
  be corrected if it assumed `ALLOW` as the healthy-state value.
- `/charts` genre filtering: the URN shape `soundcloud:genres:<name>` from the
  design brief returns HTTP 404 against the live API as of this spike (see A5).
  The un-filtered `kind=trending`/presumably `kind=top` calls work.

## Cache directory correction

Not applicable to this task's probes, but noted per the global constraints doc:
the cache directory is `dirs::cache_dir()/sonora/soundcloud`, matching the
pattern in `crates/music/src/youtube/mod.rs`, not `config_dir()`.

## A7 resolved, and a correction to the library endpoints

A7 was probed with a real token supplied by the user, from a script that read it through a
hidden prompt, never wrote it to disk, and printed only status codes and JSON field names.

| Probe | Result |
| ----- | ------ |
| `GET /me` with `Authorization: OAuth <token>` and `client_id` | 200 |
| `GET /me` with the header, no `client_id` | 200 |
| `GET /me` with `client_id` only, no header | 401 |

**A7 CONFIRMED.** The header authenticates on its own; `client_id` is not required
alongside it, though sending it anyway is harmless and keeps one code path.

### The `/me/...` library routes in the design do not exist

| Path the design assumed | Status |
| ----------------------- | ------ |
| `/me/likes/tracks` | 404 |
| `/me/followings` | 404 |
| `/me/library/albums_and_playlists` | 404 |
| `/me/playlists` | 404 |
| `/me/play-history/tracks` | 200 |

The working shape is `/users/{id}/...`, and those routes need no token at all:

| Path | Status without any Authorization header |
| ---- | --------------------------------------- |
| `/users/{id}/track_likes` | 200 |
| `/users/{id}/playlists` | 200 |
| `/users/{id}/followings` | 200 |

This matches what the web player itself does: a network capture of the signed-in likes
page shows it calling `/users/282025950/track_likes?...&client_id=...` with no
Authorization header.

**Consequence for the design.** Authentication is needed to learn *who you are* and to
write; it is not needed to read a library. The flow is: sign in, `GET /me` once to obtain
`id`, then every library read goes through `/users/{id}/...`. `SoundCloudClient` must
therefore hold the user id, not just an HTTP client. An anonymous session can still read
any public profile's likes, sets and follows — which is more than the design assumed it
could do.

### Envelope and wrapper shapes

Every listing returns `{ collection, next_href, query_urn }`. The design's `Page<T>` names
`collection` and `next_href`; `query_urn` is extra and is dropped harmlessly.

`/users/{id}/track_likes` does **not** return bare tracks. Each item is a wrapper:

```
{ "created_at": "...", "kind": "like", "track": { ...the track... } }
```

`/users/{id}/followings` returns bare user objects. `search/tracks` returns bare tracks.
So the collection element type differs per endpoint and cannot be assumed uniform.

This wrapper is useful rather than annoying: its `created_at` is what fills
`Track.added_at`, which the design's mapping table had left as `None`.

### Privacy note on `/me`

The `/me` response carries substantial personal data beyond identity: `primary_email`,
`primary_email_sha256`, `date_of_birth`, `gender`, `city`, `country_code`, `ppid`,
`analytics_id`, `marketing_ids` and `consent_management_jwt`.

The wire struct for `/me` must name only the fields the provider needs — `id`, `username`,
`avatar_url`, `permalink_url` — so everything else is dropped at deserialisation. Never
deserialise this response into a `serde_json::Value`, never log it whole, and never write
it to a fixture. The fixtures in this branch contain no `/me` response for that reason.

## Playlists return only five real tracks, and the batch endpoint reshuffles

Found while preparing the playlist tasks, by reading the committed fixtures rather than
trusting the design.

`GET /playlists/{id}` embeds a `tracks[]` array, but only the **first five entries are full
track objects**. Every entry after that is a stub carrying exactly four fields:

```
{ "id": …, "kind": "track", "monetization_model": "…", "policy": "…" }
```

Measured on the two committed fixtures: the album (17 tracks) returns 5 full and 12 stubs;
the plain set (7 tracks) returns 5 full and 2 stubs.

A `playlist_tracks` implementation that simply converts `tracks[]` would therefore return
five real tracks followed by a tail of nameless, duration-less placeholders — and it would
look correct on any playlist of five tracks or fewer.

### Resolving the stubs

`GET /tracks?ids=<comma-separated>&client_id=…` returns HTTP 200 and a **bare JSON array**
of full track objects — not a `{ collection, next_href }` envelope like every listing
endpoint. Verified against 11 stub ids from the album fixture: 11 ids in, 11 full tracks
out.

**It does not preserve the order of the ids you send.** Verified directly: the same set came
back in a different sequence. The caller must re-order the response against the playlist's
own `tracks[]` sequence, or an album plays in an arbitrary order.

This is the same shape `CLAUDE.md` describes for the Spotify provider — "uris →
`collection::metadata` → `Track`" — and the same discipline applies: resolve ids through one
batch call and reuse it everywhere, rather than re-parsing track fields per endpoint.

### Other playlist fields, measured

| Field | Album fixture | Plain set fixture |
| ----- | ------------- | ----------------- |
| `set_type` | `"album"` | `""` |
| `is_album` | `true` | `false` |
| `sharing` | `"public"` | `"public"` |
| `release_date` | `"2026-06-04T00:00:00Z"` | `null` |
| `published_at` | present | present |

`is_album` is redundant with `set_type` and less informative — `set_type` also distinguishes
ep, single and compilation, which `ReleaseType` already models. Prefer `set_type`.

`release_date` is absent on a plain set, so it must be optional. `published_at` is present on
both and is the better source for a "last modified" style timestamp, but converting its ISO
string to the `i64` the shared model wants needs a date parser that `crates/music` does not
currently depend on directly — left as `None` rather than adding a dependency for it.

## The full library endpoint map

Probed against a signed-in user's public profile, with a `client_id` and no Authorization
header — every one of these reads works unauthenticated.

| Path | Status | Holds |
| ---- | ------ | ----- |
| `/users/{id}/track_likes` | 200 | liked tracks |
| `/users/{id}/playlist_likes` | 200 | liked sets, albums among them |
| `/users/{id}/playlists` | 200 | sets the user created |
| `/users/{id}/albums` | 200 | albums the user created |
| `/users/{id}/followings` | 200 | users they follow |
| `/users/{id}/followers` | 200 | users following them |
| `/users/{id}/reposts` | 404 | — |

`/users/{id}/albums` existing as its own route is worth noting: the design assumed albums
could only be found by filtering playlists on `set_type`. For a user's *own* albums there is
a dedicated endpoint, and it is cheaper and clearer than filtering.

### Both like endpoints return wrappers

`/users/{id}/track_likes` gives `{ created_at, kind, track }` and `/users/{id}/playlist_likes`
gives `{ created_at, kind, playlist }`. The `created_at` on the wrapper is when the user liked
it, which is what `Track.added_at` and a playlist's date-added want — not the `created_at`
inside the track, which is when it was uploaded.

The two are easy to confuse and they differ by years on old material.

### What each library method maps to

A user's library is their own creations plus what they have liked, so the natural mapping
takes two requests each and splits `playlist_likes` by `is_album`:

| `MusicApi` method | Requests |
| ----------------- | -------- |
| `saved_tracks` | `/users/{id}/track_likes`, unwrapped |
| `playlists` | `/users/{id}/playlists`, plus the non-album half of `/users/{id}/playlist_likes` |
| `saved_albums` | `/users/{id}/albums`, plus the album half of `/users/{id}/playlist_likes` |
| `saved_artists` | `/users/{id}/followings` |

`playlists` and `saved_albums` read the same `playlist_likes` response and split it two ways,
exactly as `search_albums` splits a search page. Neither should fetch it twice within one
call, though sharing it *between* the two methods is not worth a cache.

The user probed against had created no sets of their own, so both `/playlists` and `/albums`
returned an empty collection. Their shapes are therefore unverified — an implementation must
tolerate an empty collection, and should not assume the element type without checking against
a user who has published something.

## The write endpoints are probably wrong, and cannot be settled without writing

The design's write paths were never verified, because verifying one modifies a real account.
They can, however, be probed for *existence* with a GET, which changes nothing.

Calibration first, so the codes mean something:

| Probe | Status |
| ----- | ------ |
| `GET /definitely/not/a/route` | 404 |
| `GET /me` | 401 |
| `GET /users/{id}/track_likes` | 200 |

So this API answers 401 for a route that exists but needs authentication, and 404 for one
that does not exist. That distinction is what makes the next table readable.

| Write path the design assumes | GET status |
| ----------------------------- | ---------- |
| `/likes/tracks/{id}` | 404 |
| `/me/followings/{id}` | 404 |
| `/me/library/albums_and_playlists/{id}` | 404 |

All three behave like the nonsense route rather than like `/me`. Two of them are also `/me/…`
paths, and every `/me/…` path except `/me` itself has already been shown not to exist.

**This is suggestive, not conclusive.** A route registered only for PUT and DELETE could in
principle answer 404 to a GET rather than 405. But the design had no evidence for these paths
to begin with, and the evidence there is now points away from them.

The likely shape, by analogy with the reads, is `/users/{me}/track_likes/{track}` — the same
`/users/{id}/…` prefix every library read uses. That is an inference, not a finding, and it is
recorded here as an inference.

Settling this needs one of two things, both requiring a real account's owner to agree:

1. A self-reversing live test: like a track, confirm, unlike it, confirm. Net zero change, but
   still a write against someone's account.
2. Capturing what the web player sends when the like button is pressed, from a browser's
   network panel.

Until then, every write method in the provider is built on an unverified path that current
evidence suggests is wrong. They will fail loudly with an HTTP error rather than corrupting
anything, but they will fail.
