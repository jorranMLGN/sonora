# SoundCloud's undocumented v2 API

SoundCloud has no public API program any more; the web player talks to
`https://api-v2.soundcloud.com` and that is what `crates/music/src/soundcloud/`
also talks to. Nothing here is officially documented — this page records what
was actually measured against the live API, so the next person touching this
provider does not have to re-derive it from scratch.

## Client id

Every request, authenticated or not, carries a `client_id` query parameter.
It is not a personal credential — it identifies the web player build, and
SoundCloud rotates it periodically. A fresh one can always be harvested from
the public site:

1. `GET https://soundcloud.com/discover` and collect every
   `https://a-v2.sndcdn.com/assets/*.js` bundle URL referenced in the page.
2. Fetch each bundle and search for `client_id:"…"` (or the JSON-key form
   `"client_id":"…"`) — a 20+ character alphanumeric value follows.

`crates/music/src/soundcloud/auth.rs` implements exactly this
(`harvest_client_id`), and `ClientId` (same file) caches the id to disk,
shares it across every `Http` clone in a session, and re-harvests it once,
with a single-flight guard, whenever the API answers 401/403.

## Authenticating

`GET /me` with an `Authorization: OAuth <token>` header authenticates as a
personal account; `client_id` is not required alongside the header, though
sending it anyway is harmless and keeps one request-building code path for
both authenticated and anonymous calls.

The `/me` response carries substantial personal data beyond identity:
`primary_email`, `primary_email_sha256`, `date_of_birth`, `gender`, `city`,
`country_code`, `ppid`, `analytics_id`, `marketing_ids`,
`consent_management_jwt`. The wire struct for `/me` names only the fields the
provider actually needs (`id`, `username`, `avatar_url`, `permalink_url`), so
everything else is dropped at deserialization; this response should never be
logged whole or written to a fixture.

## Reading a library needs no token at all

The `/me/...` routes a design might reach for first mostly don't exist:

| Path | Status |
| ---- | ------ |
| `/me/likes/tracks` | 404 |
| `/me/followings` | 404 |
| `/me/library/albums_and_playlists` | 404 |
| `/me/playlists` | 404 |
| `/me/play-history/tracks` | 200 |

The working shape is `/users/{id}/...`, and — this is the part worth
remembering — those routes need no `Authorization` header at all:

| Path | Status without any Authorization header |
| ---- | --------------------------------------- |
| `/users/{id}/track_likes` | 200 |
| `/users/{id}/playlists` | 200 |
| `/users/{id}/followings` | 200 |

This matches what the web player itself does: its own signed-in likes page
calls `/users/{id}/track_likes?...&client_id=...` with no `Authorization`
header. Authentication is only needed to learn *who you are* (`GET /me` once,
for `id`) and to write; every library read afterwards goes through
`/users/{id}/...`, which also means an anonymous session can read any public
profile's likes, sets and follows.

### The full endpoint map

Probed against a signed-in user's public profile, with only a `client_id` and
no `Authorization` header — every one of these reads works unauthenticated:

| Path | Status | Holds |
| ---- | ------ | ----- |
| `/users/{id}/track_likes` | 200 | liked tracks |
| `/users/{id}/playlist_likes` | 200 | liked sets, albums among them |
| `/users/{id}/playlists` | 200 | sets the user created |
| `/users/{id}/albums` | 200 | albums the user created |
| `/users/{id}/followings` | 200 | users they follow |
| `/users/{id}/followers` | 200 | users following them |
| `/users/{id}/reposts` | 404 | — |

`/users/{id}/albums` being its own route is worth noting on its own: it would
be tempting to assume albums can only be found by filtering playlists on
`set_type == "album"`. For a user's *own* albums there is a dedicated
endpoint instead, cheaper and clearer than filtering.

A user's library is therefore their own creations plus what they've liked,
mapped like this:

| `MusicApi` method | Requests |
| ----------------- | -------- |
| `saved_tracks` | `/users/{id}/track_likes`, unwrapped |
| `playlists` | `/users/{id}/playlists`, plus the non-album half of `/users/{id}/playlist_likes` |
| `saved_albums` | `/users/{id}/albums`, plus the album half of `/users/{id}/playlist_likes` |
| `saved_artists` | `/users/{id}/followings` |

`playlists` and `saved_albums` read the same `playlist_likes` response and
split it two ways by `set_type` (`"album"` vs. everything else, including the
empty string `""` a plain playlist carries) — exactly the way `search_albums`
splits a search page. Neither method should fetch `playlist_likes` twice
within itself, though sharing it *between* the two calls is not worth caching.

## Envelope and wrapper shapes

Every listing endpoint returns `{ collection, next_href, query_urn }`; only
`collection` and `next_href` matter, and `query_urn` is dropped harmlessly.

The element type inside `collection` is **not uniform across endpoints** —
some wrap their items, some don't:

- `/users/{id}/track_likes` wraps: `{ created_at, kind: "like", track }`.
- `/users/{id}/playlist_likes` wraps the same way with `playlist` instead of
  `track`.
- `/users/{id}/followings` and `search/tracks` return bare objects.

The wrapper's `created_at` is when the *like* happened, and is exactly what
fills a track or playlist's "date added" — it must not be confused with the
`created_at` field inside the track itself, which is upload time; the two can
differ by years on old material.

## Playlists: only the first five tracks are real

`GET /playlists/{id}` embeds a `tracks[]` array, but only the **first five
entries are full track objects**. Every entry after that is a stub carrying
exactly four fields:

```json
{ "id": 123, "kind": "track", "monetization_model": "…", "policy": "…" }
```

Measured on two playlists: a 17-track album returned 5 full tracks and 12
stubs; a 7-track plain set returned 5 full tracks and 2 stubs. A naive
`tracks[]` conversion therefore produces five real tracks followed by
nameless, duration-less placeholders — and looks correct on any playlist of
five tracks or fewer, which makes it an easy bug to ship unnoticed.

### Resolving the stubs

`GET /tracks?ids=<comma-separated>&client_id=…` returns a **bare JSON array**
of full track objects — not the `{ collection, next_href }` envelope every
listing endpoint uses. Verified with 11 stub ids from a real playlist: 11 ids
in, 11 full tracks out.

**It does not preserve the order of the ids sent.** The same set of ids came
back in a different sequence on a repeated call. A caller must re-order the
response against the playlist's own `tracks[]` sequence, or an album plays
back in an arbitrary order.

### Other measured playlist fields

| Field | Album | Plain set |
| ----- | ----- | --------- |
| `set_type` | `"album"` | `""` (empty, not absent) |
| `is_album` | `true` | `false` |
| `sharing` | `"public"` | `"public"` |
| `release_date` | present | `null` |
| `published_at` | present | present |

`is_album` is redundant with `set_type` and less informative — `set_type`
also distinguishes `ep`, `single` and `compilation`. `release_date` is absent
on a plain set, so it must be optional; `published_at` is present on both and
is the better source for a "last modified" timestamp.

## Write endpoints are unverified, and likely wrong

No write endpoint has ever been exercised against a real account — doing so
modifies that account, and this was never authorized. What can be checked
without changing anything is *existence*, with a GET.

Calibration first, so the status codes below mean something:

| Probe | Status |
| ----- | ------ |
| `GET /definitely/not/a/route` | 404 |
| `GET /me` (no `Authorization`) | 401 |
| `GET /users/{id}/track_likes` | 200 |

So this API answers 401 for a route that exists but needs authentication, and
404 for one that doesn't exist at all. Against that scale:

| Assumed write path | GET status |
| ------------------- | ---------- |
| `/likes/tracks/{id}` | 404 |
| `/me/followings/{id}` | 404 |
| `/me/library/albums_and_playlists/{id}` | 404 |

All three answer like a nonexistent route, not like `/me`. This is
suggestive, not conclusive — a route registered only for PUT/DELETE could in
principle answer 404 to a GET rather than 405 — but there is no positive
evidence for any of these paths.

The likely shape, by analogy with the reads, is `/users/{me}/track_likes/{track}`
— the same `/users/{id}/...` prefix every library read uses — but that is an
inference, not a finding. Settling it needs either a self-reversing live test
against a real, consenting account (like, confirm, unlike, confirm) or a
capture of what the web player itself sends when its like button is pressed.
Until then, every write method in this provider calls a path current evidence
suggests is wrong; it will fail loudly with an HTTP error rather than corrupt
anything, but it will fail.

## Playback: the transcoding chain

A track's `media.transcodings[]` lists the available encodes. One measured
track offered five:

| Protocol | Preset | Quality | Legacy |
| -------- | ------ | ------- | ------ |
| hls | `aac_160k` | sq | no |
| hls | `aac_96k` | lq | no |
| hls | `abr_sq` | sq | no |
| hls | `mp3_0_0` | sq | yes |
| progressive | `mp3_0_0` | sq | yes |

**The only progressive entry is a legacy 128 kbps MP3; everything better is
HLS-only.** A picker that prefers progressive for simplicity ends up serving
exactly the bitrate the official API offers — prefer the best HLS entry and
keep progressive only as the fallback when no HLS entry is usable. The
picker must filter by `format.protocol`, not assume a 1:1 mapping between
preset and protocol: the progressive and one HLS entry above share the same
`mp3_0_0` preset.

### Resolving a transcoding

`GET <transcoding.url>?client_id=…` answers with a single-key object,
`{"url": "…"}` — the actual playable location. The progressive entry
resolves to a plain `.mp3` on SoundCloud's CDN; an HLS entry resolves to a
`playlist.m3u8`.

### The HLS playlist is fragmented MP4, not raw AAC

The media playlist looks like:

```
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:10
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="…/init.mp4?expires=…&Signature=…"
#EXTINF:10.007800,
…/data000.m4s?expires=…&Signature=…
```

The media segments are `.m4s`, preceded by an **initialization segment named
on the `#EXT-X-MAP` line**. That init segment carries the `moov` box —
without it the concatenated media segments are headerless and no decoder can
read them. A playlist parser that simply skips every line starting with `#`
discards `#EXT-X-MAP` along with the genuine comments, producing a buffer
that decodes to nothing.

The correct sequence: fetch the URI named on `#EXT-X-MAP` first, then every
`.m4s` in listed order, and concatenate init + media. That yields a valid
fragmented MP4, which `rodio` can already decode via `symphonia-isomp4` and
`symphonia-aac`.

### Segment URLs expire

Every segment URL carries `expires=` and a signed `Signature`. The playlist
must be fetched fresh for each playback rather than cached across sessions —
a long pause before a seek can outlive the signatures on already-fetched
segment URLs.
