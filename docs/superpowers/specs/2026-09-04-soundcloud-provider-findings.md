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
