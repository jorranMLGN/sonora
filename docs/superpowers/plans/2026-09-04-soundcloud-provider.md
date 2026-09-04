# SoundCloud Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SoundCloud as a third music provider in Sonora, at feature parity with the Spotify and YouTube Music providers wherever the platform allows.

**Architecture:** A new `crates/music/src/soundcloud/` module implementing the three existing provider traits — `MusicProvider` (sign-in lifecycle), `MusicApi` (data access), `PlaybackFactory`/`Player` (audio). It talks to SoundCloud's internal v2 API (`api-v2.soundcloud.com`) using a `client_id` harvested from the web player bundle. `client.rs` implements the trait and delegates to focused modules; it contains no HTTP calls of its own. One registration line in `crates/sonora/src/main.rs` is the only change to existing application wiring.

**Tech Stack:** Rust 1.97.1 (pinned in `rust-toolchain.toml`), `reqwest` for HTTP, `serde`/`serde_json` for wire types, `rodio` for decoding, `anyhow` for errors, `async-trait` for the provider traits, `tokio` for async.

**Spec:** `docs/superpowers/specs/2026-09-04-soundcloud-provider-design.md`

## Global Constraints

- Rust edition and toolchain come from the workspace; do not add a `rust-version` or change `rust-toolchain.toml`.
- Errors use `anyhow` with lowercase context: `.context("cannot fetch the track")`. Never capitalise, never end with a period. Source: `CLAUDE.md`.
- Only `crates/sonora/src/main.rs` may name a concrete provider. No other crate imports `SoundCloudProvider`. Source: `CLAUDE.md`.
- `client.rs` implements `MusicApi` by delegating to a focused module. No `reqwest` call lives in `client.rs`.
- This plan adds one module the spec's table does not list: `http.rs`. Every other module needs the same
  authorised request shape, and putting it in `client.rs` would break the rule above. Record the addition
  when the branch is readied.
- The cache directory is `dirs::cache_dir()/sonora/soundcloud`. Note this corrects the design doc, which said `config_dir()`; `crates/music/src/youtube/mod.rs` uses `cache_dir()` and is the pattern to follow.
- Commit messages: Conventional Commits, no AI or assistant attribution trailer of any kind.
- No new workspace dependency without justification in the commit body. Everything this plan needs is already in `crates/music/Cargo.toml`.
- Brand assets come from Simple Icons (CC0), matching `assets/icons/common/LICENSE`.
- All wire structs use `#[serde(default)]` on optional fields. The v2 API adds and removes fields without notice; a missing field must never fail a whole page of results.

---

## Task 1: Verification spike

The design rests on eight assumptions (A1–A8 in the spec) that were never confirmed against the live API. This task confirms or kills them, and captures the JSON fixtures every later task tests against. **No later task may start until this one is done**, because Tasks 4, 5, 8 and 9 write serde structs whose field names come from these fixtures.

**Permission note:** the `client_id` harvest performs an HTTP fetch of the SoundCloud web player bundle and greps it for an identifier. An earlier attempt at exactly this was refused by the Claude Code auto-mode classifier, which read the pattern as credential harvesting. The user must grant this explicitly before the task can run. Do not route around the refusal — if it is refused again, stop and report; the fallback is approach A in the spec, which invalidates this plan.

**Files:**
- Create: `crates/music/src/soundcloud/fixtures/track.json`
- Create: `crates/music/src/soundcloud/fixtures/playlist_album.json`
- Create: `crates/music/src/soundcloud/fixtures/playlist_set.json`
- Create: `crates/music/src/soundcloud/fixtures/user.json`
- Create: `crates/music/src/soundcloud/fixtures/search_tracks.json`
- Create: `docs/superpowers/specs/2026-09-04-soundcloud-provider-findings.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the fixture files above, and a findings document recording, for each of A1–A8, `CONFIRMED` or `FALSE` with the observed evidence.

- [ ] **Step 1: Harvest a `client_id`**

```bash
UA='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36'
curl -sL -A "$UA" https://soundcloud.com/discover -o /tmp/sc.html
grep -oE 'https://a-v2\.sndcdn\.com/assets/[^"]+\.js' /tmp/sc.html | tail -5
```

Fetch each listed bundle and search it for the identifier the player sends as `client_id`. Record which bundle carried it and how it was formatted. This answers **A1**. If no bundle carries one, A1 is FALSE: stop the whole plan and report.

- [ ] **Step 2: Capture a track**

```bash
curl -s "https://api-v2.soundcloud.com/tracks/<id>?client_id=$CID" | python3 -m json.tool \
  > crates/music/src/soundcloud/fixtures/track.json
```

Pick a well-known public track. Confirm the response carries `id`, `title`, `duration`, `streamable`, `policy`, `playback_count`, `artwork_url`, `permalink_url`, `user`, and `media.transcodings`. Record the actual names if they differ.

Inspect `media.transcodings[]`. For each entry note `format.protocol` and `preset`. A `progressive` protocol answers **A6**.

- [ ] **Step 3: Capture a Go+ / snipped track**

Find a track whose `policy` is not `ALLOW` and record the value. This is what `Track::playable` must reject. If no `SNIP` value appears, record what the restricted policy is actually called.

- [ ] **Step 4: Capture an album and a plain set**

```bash
curl -s "https://api-v2.soundcloud.com/playlists/<album-id>?client_id=$CID" | python3 -m json.tool \
  > crates/music/src/soundcloud/fixtures/playlist_album.json
curl -s "https://api-v2.soundcloud.com/playlists/<set-id>?client_id=$CID" | python3 -m json.tool \
  > crates/music/src/soundcloud/fixtures/playlist_set.json
```

Record the exact `set_type` values observed. This answers **A2**. If `set_type` does not exist or is always empty, A2 is FALSE — `saved_albums`, `album` and `album_tracks` become empty stubs and Task 5 shrinks accordingly.

- [ ] **Step 5: Capture a user, and probe the remaining endpoints**

```bash
curl -s "https://api-v2.soundcloud.com/users/<id>?client_id=$CID" | python3 -m json.tool \
  > crates/music/src/soundcloud/fixtures/user.json
curl -s "https://api-v2.soundcloud.com/search/tracks?q=test&limit=5&client_id=$CID" | python3 -m json.tool \
  > crates/music/src/soundcloud/fixtures/search_tracks.json
```

Then check, recording the HTTP status and whether the body is usable:
- `/tracks/<id>/related` — answers **A3**
- `/mixed-selections` — answers **A4**
- `/charts?kind=trending&genre=soundcloud:genres:house` — answers **A5**

- [ ] **Step 6: Probe the authenticated path**

With a personal `oauth_token` taken from a signed-in browser session, call `/me` with the header `Authorization: OAuth <token>`. A 200 answers **A7**. Record whether the `client_id` query parameter is still required alongside the header.

Do not commit the token, and do not paste it into the findings document.

- [ ] **Step 7: Check the browser cookie helper**

```bash
cargo fetch
grep -rn "pub fn cookies" ~/.cargo/git/checkouts/ytmusic-rs-*/*/src/browser*
```

Determine whether `ytmusic::browser::cookies()` takes a host or is fixed to YouTube. This answers **A8** and decides whether `SignIn::Browser` ships in this plan or is deferred.

- [ ] **Step 8: Write the findings document**

One table, one row per assumption, columns: assumption, verdict, evidence, consequence. Then a short section recording the real field names wherever they differ from the spec's mapping table.

- [ ] **Step 9: Commit**

```bash
git add crates/music/src/soundcloud/fixtures docs/superpowers/specs/2026-09-04-soundcloud-provider-findings.md
git commit -m "docs: verify SoundCloud v2 API assumptions

Captured live fixtures for track, album, set, user and search, and
recorded a verdict for each of the eight assumptions the design rests on."
```

---

## Task 2: Provider skeleton and registration

Get a provider that compiles, registers, and signs in anonymously without doing anything useful yet. Everything after this is filling in methods behind a shape that already works.

**Files:**
- Create: `crates/music/src/soundcloud/mod.rs`
- Create: `crates/music/src/soundcloud/client.rs`
- Modify: `crates/music/src/lib.rs` (add `pub mod soundcloud;`)
- Modify: `crates/sonora/src/main.rs:62`

**Interfaces:**
- Consumes: `MusicProvider`, `MusicApi`, `ProviderSession`, `SignIn`, `UserProfile` from `crate::`.
- Produces:
  - `soundcloud::SoundCloudProvider::new() -> SoundCloudProvider`
  - `soundcloud::SoundCloudClient::new(http: Arc<Http>) -> SoundCloudClient`
  - `SoundCloudProvider::slug() -> "soundcloud"`, `name() -> "SoundCloud"`

- [ ] **Step 1: Write the failing test**

In `crates/music/src/soundcloud/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::SoundCloudProvider;
    use crate::MusicProvider;

    #[test]
    fn identifies_itself() {
        let provider = SoundCloudProvider::new();
        assert_eq!(provider.slug(), "soundcloud");
        assert_eq!(provider.name(), "SoundCloud");
    }

    #[test]
    fn offers_anonymous_and_secret_sign_in() {
        let provider = SoundCloudProvider::new();
        let options = provider.sign_in_options();
        assert!(options.contains(&crate::SignIn::Anonymous));
        assert!(options.contains(&crate::SignIn::Secret));
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p music soundcloud`
Expected: FAIL — `unresolved import` / `module soundcloud not found`.

- [ ] **Step 3: Write the skeleton**

`crates/music/src/soundcloud/mod.rs`:

```rust
mod client;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::{
    InputSource, MusicProvider, PromptSink, ProviderSession, SignIn, UserProfile,
};
pub use client::SoundCloudClient;

const GUEST_ID: &str = "soundcloud-guest";

pub struct SoundCloudProvider {
    client_id: PathBuf,
    token: PathBuf,
}

impl SoundCloudProvider {
    pub fn new() -> Self {
        let cache = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("sonora")
            .join("soundcloud");
        Self {
            client_id: cache.join("client_id.txt"),
            token: cache.join("token.txt"),
        }
    }
}

impl Default for SoundCloudProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for SoundCloudProvider {
    fn name(&self) -> &'static str {
        "SoundCloud"
    }

    fn slug(&self) -> &'static str {
        "soundcloud"
    }

    fn sign_in_options(&self) -> Vec<SignIn> {
        vec![SignIn::Anonymous, SignIn::Secret]
    }

    fn stored(&self) -> bool {
        self.token.exists() || self.client_id.exists()
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        Ok(None)
    }

    async fn sign_in(
        &self,
        _method: SignIn,
        _prompt: PromptSink,
        _input: InputSource,
    ) -> Result<ProviderSession> {
        anyhow::bail!("soundcloud sign-in is not implemented yet")
    }

    fn sign_out(&self) {
        for path in [&self.client_id, &self.token] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("soundcloud: cannot remove credential cache: {error}");
            }
        }
    }
}
```

`crates/music/src/soundcloud/client.rs` implements `MusicApi` with every required method returning `anyhow::bail!("not implemented yet")`, `share_url` returning `None`, and the trait's defaulted methods left alone. Consult `crates/music/src/lib.rs:67` for the full required list; the compiler will name any method you miss.

Add `pub mod soundcloud;` to `crates/music/src/lib.rs` in alphabetical order, between `mod spectrum;` and `pub mod spotify;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p music soundcloud`
Expected: PASS, both tests.

- [ ] **Step 5: Register the provider**

In `crates/sonora/src/main.rs:62`, extend the vector:

```rust
let providers: Vec<Arc<dyn music::MusicProvider>> = vec![
    Arc::new(music::spotify::SpotifyProvider::from_env()),
    Arc::new(music::youtube::YouTubeProvider::new()),
    Arc::new(music::soundcloud::SoundCloudProvider::new()),
];
```

- [ ] **Step 6: Verify the whole workspace still builds**

Run: `cargo check --workspace`
Expected: no errors. Warnings about unused fields are fine at this stage.

- [ ] **Step 7: Commit**

```bash
git add crates/music/src/lib.rs crates/music/src/soundcloud crates/sonora/src/main.rs
git commit -m "feat(soundcloud): add provider skeleton

Registers a SoundCloud provider that identifies itself and offers its
sign-in options. Every data method still refuses; the following commits
fill them in."
```

---

## Task 3: HTTP layer and `client_id` lifecycle

**Files:**
- Create: `crates/music/src/soundcloud/http.rs`
- Create: `crates/music/src/soundcloud/auth.rs`
- Modify: `crates/music/src/soundcloud/mod.rs`

**Interfaces:**
- Consumes: `SoundCloudProvider` from Task 2.
- Produces:
  - `auth::extract_client_id(bundle: &str) -> Option<String>`
  - `auth::bundle_urls(html: &str) -> Vec<String>`
  - `http::Http::anonymous(client_id: String) -> Http`
  - `http::Http::with_token(client_id: String, token: String) -> Http`
  - `http::Http::get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T>`
  - `http::Http::authenticated(&self) -> bool`

- [ ] **Step 1: Write the failing tests**

In `auth.rs`. These are pure string functions — no network, no fixtures needed. Adjust the literal in `finds_the_client_id` to match the real format recorded in Task 1.

```rust
#[cfg(test)]
mod tests {
    use super::{bundle_urls, extract_client_id};

    #[test]
    fn finds_the_client_id() {
        let bundle = r#"n.set({"client_id":"aBcD1234efGH5678ijKL9012mnOP3456"})"#;
        assert_eq!(
            extract_client_id(bundle).as_deref(),
            Some("aBcD1234efGH5678ijKL9012mnOP3456")
        );
    }

    #[test]
    fn ignores_a_short_candidate() {
        assert!(extract_client_id(r#"client_id:"abc""#).is_none());
    }

    #[test]
    fn returns_none_without_a_client_id() {
        assert!(extract_client_id("var x = 1;").is_none());
    }

    #[test]
    fn lists_bundles_in_document_order() {
        let html = r#"
            <script src="https://a-v2.sndcdn.com/assets/0-aaa.js"></script>
            <script src="https://a-v2.sndcdn.com/assets/9-zzz.js"></script>
        "#;
        assert_eq!(
            bundle_urls(html),
            vec![
                "https://a-v2.sndcdn.com/assets/0-aaa.js".to_string(),
                "https://a-v2.sndcdn.com/assets/9-zzz.js".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_other_hosts() {
        assert!(bundle_urls(r#"<script src="https://example.com/x.js">"#).is_empty());
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p music soundcloud::auth`
Expected: FAIL — `cannot find function extract_client_id`.

- [ ] **Step 3: Implement the extraction**

`crates/music/src/soundcloud/auth.rs`:

```rust
use anyhow::{Context as _, Result};

const MIN_ID: usize = 20;
const BUNDLE_HOST: &str = "https://a-v2.sndcdn.com/assets/";

/// Pulls the player's `client_id` out of a web bundle.
///
/// The bundle is minified and the identifier is written as a JSON key or an
/// object property, so both `"client_id":"…"` and `client_id:"…"` occur.
pub fn extract_client_id(bundle: &str) -> Option<String> {
    let mut rest = bundle;
    while let Some(at) = rest.find("client_id") {
        rest = &rest[at + "client_id".len()..];
        let value = rest
            .trim_start_matches(['"', ':', '=', ' ']);
        let id: String = value
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if id.len() >= MIN_ID {
            return Some(id);
        }
    }
    None
}

/// Lists the player bundle URLs a page references, in document order.
pub fn bundle_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(BUNDLE_HOST) {
        rest = &rest[at..];
        let end = rest.find(".js").map(|e| e + 3);
        let Some(end) = end else { break };
        urls.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    urls
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p music soundcloud::auth`
Expected: PASS, five tests.

- [ ] **Step 5: Add the network side of the harvest**

Append to `auth.rs`. This has no unit test — it is a network call, covered by the live tests in Task 14.

```rust
const DISCOVER: &str = "https://soundcloud.com/discover";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// Fetches the web player and reads the `client_id` it ships with.
pub async fn harvest_client_id() -> Result<String> {
    let agent = reqwest::Client::builder()
        .user_agent(UA)
        .build()
        .context("cannot build the http client")?;
    let page = agent
        .get(DISCOVER)
        .send()
        .await
        .context("cannot reach soundcloud")?
        .text()
        .await
        .context("cannot read the soundcloud page")?;
    for url in bundle_urls(&page).into_iter().rev() {
        let Ok(response) = agent.get(&url).send().await else {
            continue;
        };
        let Ok(bundle) = response.text().await else {
            continue;
        };
        if let Some(id) = extract_client_id(&bundle) {
            return Ok(id);
        }
    }
    anyhow::bail!("the soundcloud player carries no usable client id")
}
```

- [ ] **Step 6: Write the HTTP wrapper**

`crates/music/src/soundcloud/http.rs`:

```rust
use anyhow::{Context as _, Result};
use serde::de::DeserializeOwned;

const BASE: &str = "https://api-v2.soundcloud.com";

pub struct Http {
    agent: reqwest::Client,
    client_id: String,
    token: Option<String>,
}

impl Http {
    pub fn anonymous(client_id: String) -> Self {
        Self {
            agent: reqwest::Client::new(),
            client_id,
            token: None,
        }
    }

    pub fn with_token(client_id: String, token: String) -> Self {
        Self {
            agent: reqwest::Client::new(),
            client_id,
            token: Some(token),
        }
    }

    pub fn authenticated(&self) -> bool {
        self.token.is_some()
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let mut request = self
            .agent
            .get(format!("{BASE}{path}"))
            .query(&[("client_id", self.client_id.as_str())])
            .query(query);
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("OAuth {token}"));
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("cannot reach soundcloud for {path}"))?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("soundcloud refused {path} with {status}");
        }
        response
            .json()
            .await
            .with_context(|| format!("cannot read the soundcloud response for {path}"))
    }
}
```

- [ ] **Step 7: Wire sign-in**

In `mod.rs`, replace the `sign_in` stub. `SignIn::Anonymous` harvests a `client_id`, caches it, and returns a guest session. `SignIn::Secret` additionally prompts for the token via `SignInPrompt::Secret` and reads it from `input` — mirror `youtube/mod.rs:325`. `restore()` reads both cached files and rebuilds the matching session. Cache the `client_id` to `self.client_id` and the token to `self.token`, creating the parent directory first, exactly as `youtube/mod.rs:store_cookies` does.

The guest `UserProfile` is `{ id: GUEST_ID, display_name: "SoundCloud" }`.

- [ ] **Step 8: Add `SignIn::Browser`, but only if A8 was confirmed**

Skip this step entirely if Task 1 found `ytmusic::browser::cookies()` fixed to YouTube. In that case
`sign_in_options()` keeps returning just `Anonymous` and `Secret`, and browser sign-in becomes a
follow-up PR — say so in the commit body so the gap is a recorded decision rather than an oversight.

If the helper does take a host, mirror `crates/music/src/youtube/mod.rs:288`: list the Firefox-family
browsers, push a `SignIn::Browser(name)` per browser into `sign_in_options()`, and in `sign_in` read
the `oauth_token` cookie for `soundcloud.com` instead of prompting for a paste. Extend the Task 2
test `offers_anonymous_and_secret_sign_in` to assert the browser entries appear.

- [ ] **Step 9: Verify**

Run: `cargo test -p music soundcloud && cargo check --workspace`
Expected: PASS, no errors.

- [ ] **Step 10: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): harvest and cache the player client id

Anonymous sign-in now reads the client id out of the web player bundle
and caches it; token sign-in stores a pasted oauth_token alongside it."
```

---

## Task 4: Wire types and track conversion

The first task driven by Task 1's fixtures. Every field name below must be checked against `fixtures/track.json` and corrected if it differs.

**Files:**
- Create: `crates/music/src/soundcloud/wire.rs`

**Interfaces:**
- Consumes: `fixtures/track.json` from Task 1.
- Produces:
  - `wire::Track` (serde type) and `wire::User` (serde type)
  - `wire::track(raw: Track) -> crate::Track`
  - `wire::artwork(url: Option<&str>, size: &str) -> Option<String>`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::{artwork, track, Track as Raw};

    fn fixture() -> Raw {
        let json = include_str!("fixtures/track.json");
        serde_json::from_str(json).expect("the captured track fixture must parse")
    }

    #[test]
    fn parses_the_captured_track() {
        let raw = fixture();
        assert!(raw.id > 0);
        assert!(!raw.title.is_empty());
    }

    #[test]
    fn converts_the_captured_track() {
        let converted = track(fixture());
        assert!(converted.id.is_some());
        assert!(!converted.name.is_empty());
        assert!(!converted.artists.is_empty());
        assert!(converted.duration.as_millis() > 0);
        assert_eq!(converted.disc_number, 1);
        assert!(!converted.explicit);
        assert!(converted.album.is_empty());
        assert!(converted.album_id.is_none());
    }

    #[test]
    fn refuses_a_blocked_track() {
        let mut raw = fixture();
        raw.policy = "BLOCK".to_string();
        assert!(!track(raw).playable);
    }

    #[test]
    fn refuses_a_snipped_track() {
        let mut raw = fixture();
        raw.policy = "SNIP".to_string();
        assert!(!track(raw).playable);
    }

    #[test]
    fn refuses_an_unstreamable_track() {
        let mut raw = fixture();
        raw.streamable = false;
        assert!(!track(raw).playable);
    }

    #[test]
    fn upgrades_the_artwork_size() {
        let url = artwork(Some("https://i1.sndcdn.com/artworks-abc-large.jpg"), "t500x500");
        assert_eq!(
            url.as_deref(),
            Some("https://i1.sndcdn.com/artworks-abc-t500x500.jpg")
        );
    }

    #[test]
    fn leaves_an_unsized_artwork_alone() {
        let url = artwork(Some("https://i1.sndcdn.com/artworks-abc.jpg"), "t500x500");
        assert_eq!(url.as_deref(), Some("https://i1.sndcdn.com/artworks-abc.jpg"));
    }

    #[test]
    fn has_no_artwork_without_a_url() {
        assert!(artwork(None, "t500x500").is_none());
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p music soundcloud::wire`
Expected: FAIL — `cannot find type Track in this scope`.

- [ ] **Step 3: Implement**

```rust
use std::time::Duration;

use serde::Deserialize;

use crate::models::{ArtistRef, Track as Model};

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub streamable: bool,
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub playback_count: Option<u64>,
    #[serde(default)]
    pub likes_count: Option<u64>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    pub user: User,
}

/// Rewrites a SoundCloud artwork URL to a larger variant.
///
/// SoundCloud encodes the size in the filename (`…-large.jpg`), so asking for a
/// bigger cover is a substitution rather than a separate request.
pub fn artwork(url: Option<&str>, size: &str) -> Option<String> {
    let url = url?;
    Some(match url.rsplit_once("-large.") {
        Some((head, ext)) => format!("{head}-{size}.{ext}"),
        None => url.to_string(),
    })
}

pub fn track(raw: Track) -> Model {
    let artist = raw.user.username.clone();
    Model {
        id: Some(raw.id.to_string()),
        name: raw.title,
        playable: raw.streamable && raw.policy != "BLOCK" && raw.policy != "SNIP",
        artists: artist.clone(),
        artist_refs: vec![ArtistRef {
            name: artist,
            id: Some(raw.user.id.to_string()),
        }],
        album: String::new(),
        album_id: None,
        cover: artwork(raw.artwork_url.as_deref(), "t500x500"),
        duration: Duration::from_millis(raw.duration),
        added_at: None,
        added_by: None,
        playcount: raw.playback_count,
        popularity: 0,
        explicit: false,
        track_number: 0,
        disc_number: 1,
        tags: raw.genre.into_iter().collect(),
        languages: Vec::new(),
        credits: Vec::new(),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p music soundcloud::wire`
Expected: PASS, eight tests. If the fixture fails to parse, the field names differ from Task 1's capture — correct the struct, not the fixture.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/soundcloud/wire.rs
git commit -m "feat(soundcloud): convert v2 tracks to the shared model

Tracks under a BLOCK or SNIP policy are marked unplayable rather than
played as a truncated preview."
```

---

## Task 5: Playlists, sets, and the album mapping

SoundCloud has no separate album type. A playlist carrying a `set_type` of album, ep, single or compilation *is* an album; anything else is a playlist. This is what makes `saved_albums()` and `album()` real implementations instead of empty stubs.

**Files:**
- Modify: `crates/music/src/soundcloud/wire.rs`

**Interfaces:**
- Consumes: `wire::track`, `wire::artwork`, `wire::User` from Task 4; both playlist fixtures from Task 1.
- Produces:
  - `wire::Playlist` (serde type)
  - `wire::release_type(set_type: &str) -> Option<crate::ReleaseType>`
  - `wire::is_album(raw: &Playlist) -> bool`
  - `wire::playlist(raw: Playlist) -> crate::Playlist`
  - `wire::album(raw: Playlist) -> crate::Album`

- [ ] **Step 1: Write the failing tests**

Replace the literals with the values Task 1 actually observed.

```rust
#[test]
fn maps_the_known_set_types() {
    use crate::ReleaseType;
    assert_eq!(release_type("album"), Some(ReleaseType::Album));
    assert_eq!(release_type("ep"), Some(ReleaseType::Ep));
    assert_eq!(release_type("single"), Some(ReleaseType::Single));
    assert_eq!(release_type("compilation"), Some(ReleaseType::Compilation));
}

#[test]
fn treats_an_unknown_set_type_as_a_playlist() {
    assert_eq!(release_type(""), None);
    assert_eq!(release_type("playlist"), None);
}

#[test]
fn recognises_the_captured_album() {
    let raw: Playlist =
        serde_json::from_str(include_str!("fixtures/playlist_album.json")).unwrap();
    assert!(is_album(&raw));
    let converted = album(raw);
    assert!(!converted.name.is_empty());
    assert!(!converted.artists.is_empty());
}

#[test]
fn recognises_the_captured_set_as_a_playlist() {
    let raw: Playlist =
        serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
    assert!(!is_album(&raw));
    let converted = playlist(raw);
    assert!(!converted.name.is_empty());
    assert!(!converted.owner.is_empty());
}

#[test]
fn reads_visibility_from_sharing() {
    let mut raw: Playlist =
        serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
    raw.sharing = "private".to_string();
    assert!(!playlist(raw.clone()).public);
    raw.sharing = "public".to_string();
    assert!(playlist(raw).public);
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p music soundcloud::wire`
Expected: FAIL — `cannot find function release_type`.

- [ ] **Step 3: Implement**

```rust
use crate::models::{Album as AlbumModel, Playlist as PlaylistModel, ReleaseType};

#[derive(Clone, Debug, Deserialize)]
pub struct Playlist {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub set_type: String,
    #[serde(default)]
    pub sharing: String,
    #[serde(default)]
    pub track_count: u32,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    pub user: User,
}

/// Maps a SoundCloud `set_type` onto the shared release model.
///
/// A set with no recognised type is a plain playlist, not an album.
pub fn release_type(set_type: &str) -> Option<ReleaseType> {
    match set_type {
        "album" => Some(ReleaseType::Album),
        "ep" => Some(ReleaseType::Ep),
        "single" => Some(ReleaseType::Single),
        "compilation" => Some(ReleaseType::Compilation),
        _ => None,
    }
}

pub fn is_album(raw: &Playlist) -> bool {
    release_type(&raw.set_type).is_some()
}

pub fn playlist(raw: Playlist) -> PlaylistModel {
    PlaylistModel {
        id: raw.id.to_string(),
        name: raw.title,
        owner: raw.user.username,
        owner_id: raw.user.id.to_string(),
        owned: false,
        collaborative: false,
        blend: false,
        public: raw.sharing == "public",
        cover: artwork(raw.artwork_url.as_deref(), "t500x500"),
        track_count: raw.track_count,
        modified_at: None,
    }
}
```

`album(raw: Playlist) -> AlbumModel` follows the same shape: `name` from `title`, `artists`/`artist_refs` from `user`, `cover` from `artwork`, and the release type from `release_type(&raw.set_type)`. Read `crates/music/src/models.rs` for the exact `Album` fields — the compiler will list any you miss.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p music soundcloud::wire`
Expected: PASS, thirteen tests total in the module.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/soundcloud/wire.rs
git commit -m "feat(soundcloud): map typed sets onto albums

A set whose set_type is album, ep, single or compilation converts to an
Album; anything else stays a Playlist."
```

---

## Task 6: Search

**Files:**
- Create: `crates/music/src/soundcloud/search.rs`
- Modify: `crates/music/src/soundcloud/client.rs`

**Interfaces:**
- Consumes: `http::Http`, `wire::{track, playlist, Track, Playlist}`.
- Produces:
  - `search::tracks(http: &Http, query: &str) -> Result<Vec<crate::Track>>`
  - `search::playlists(http: &Http, query: &str) -> Result<Vec<crate::Playlist>>`
  - `search::users(http: &Http, query: &str) -> Result<Vec<wire::User>>`
  - `wire::Page<T>` — the `{ collection, next_href }` envelope every listing endpoint returns

- [ ] **Step 1: Write the failing test**

In `wire.rs`, against the search fixture:

```rust
#[test]
fn parses_a_search_page() {
    let page: Page<Track> =
        serde_json::from_str(include_str!("fixtures/search_tracks.json")).unwrap();
    assert!(!page.collection.is_empty());
    assert!(page.collection.iter().all(|t| t.id > 0));
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p music soundcloud::wire::tests::parses_a_search_page`
Expected: FAIL — `cannot find type Page`.

- [ ] **Step 3: Add the envelope**

In `wire.rs`:

```rust
/// The envelope every v2 listing endpoint returns.
#[derive(Clone, Debug, Deserialize)]
pub struct Page<T> {
    #[serde(default = "Vec::new")]
    pub collection: Vec<T>,
    #[serde(default)]
    pub next_href: Option<String>,
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p music soundcloud::wire`
Expected: PASS.

- [ ] **Step 5: Implement search**

`crates/music/src/soundcloud/search.rs`:

```rust
use anyhow::{Context as _, Result};

use crate::soundcloud::http::Http;
use crate::soundcloud::wire::{self, Page};

const LIMIT: &str = "50";

pub async fn tracks(http: &Http, query: &str) -> Result<Vec<crate::Track>> {
    let page: Page<wire::Track> = http
        .get_json("/search/tracks", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud tracks")?;
    Ok(page.collection.into_iter().map(wire::track).collect())
}

pub async fn playlists(http: &Http, query: &str) -> Result<Vec<crate::Playlist>> {
    let page: Page<wire::Playlist> = http
        .get_json("/search/playlists", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud playlists")?;
    Ok(page.collection.into_iter().map(wire::playlist).collect())
}

pub async fn users(http: &Http, query: &str) -> Result<Vec<wire::User>> {
    let page: Page<wire::User> = http
        .get_json("/search/users", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud users")?;
    Ok(page.collection)
}
```

- [ ] **Step 6: Wire it into the client**

In `client.rs`, replace the `search`, `search_playlists` and `search_albums` stubs. `search_albums` filters `search/playlists` results through `wire::is_album`, so it needs the raw page rather than converted playlists — add `search::albums` alongside the three functions above, filtering on `is_album` and mapping with `wire::album`.

- [ ] **Step 7: Verify**

Run: `cargo test -p music soundcloud && cargo check --workspace`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): search tracks, playlists and albums"
```

---

## Task 7: Library reads and writes

**Files:**
- Create: `crates/music/src/soundcloud/library.rs`
- Modify: `crates/music/src/soundcloud/client.rs`

**Interfaces:**
- Consumes: `http::Http`, `wire::*`.
- Produces:
  - `library::liked_tracks(http: &Http, limit: u32) -> Result<Vec<crate::Track>>`
  - `library::set_track_liked(http: &Http, id: &str, liked: bool) -> Result<()>`
  - `library::saved_sets(http: &Http, limit: u32) -> Result<Vec<wire::Playlist>>`
  - `library::followed(http: &Http, limit: u32) -> Result<Vec<crate::SavedArtist>>`
  - `library::set_followed(http: &Http, id: &str, followed: bool) -> Result<()>`
  - `library::set_saved(http: &Http, id: &str, saved: bool) -> Result<()>` — the shared
    `PUT`/`DELETE /me/library/albums_and_playlists/{id}` call behind both `set_album_saved` and
    `add_playlist_to_library`/`remove_playlist_from_library`
  - `http::Http::put_empty(&self, path: &str) -> Result<()>` and `http::Http::delete(&self, path: &str) -> Result<()>`

- [ ] **Step 1: Add the write verbs to `http.rs`**

Both mirror `get_json`: same query parameters, same `Authorization` header, but they return `()` and treat any 2xx as success. Add them before the library module uses them.

- [ ] **Step 2: Implement the reads**

`liked_tracks` calls `/me/likes/tracks` with `limit`; `saved_sets` calls `/me/library/albums_and_playlists`; `followed` calls `/me/followings`. Each parses `Page<T>` and converts through `wire`. Confirm the exact paths against Task 1's findings before writing them — a 404 here means the path is wrong, not that the feature is missing.

`saved_sets` returns the raw `wire::Playlist` so that `client.rs` can split it two ways: `saved_albums()` filters on `wire::is_album` and maps with `wire::album`, `playlists()` filters on the negation and maps with `wire::playlist`. This is the single request behind both trait methods.

- [ ] **Step 3: Implement the writes**

`set_track_liked` is `PUT /likes/tracks/{id}` when liking and `DELETE` on the same path when unliking. `set_followed` is `PUT`/`DELETE /me/followings/{id}`. Both require an authenticated `Http`; return `anyhow::bail!("signing in is required to change the library")` when `http.authenticated()` is false.

- [ ] **Step 4: Wire the client methods**

Replace the stubs for `saved_tracks`, `set_track_saved`, `playlists`, `saved_albums`, `set_album_saved`, `saved_artists` and `set_artist_saved`.

`set_album_saved` is the interesting one: SoundCloud has no album-save distinct from liking a set, so it maps to `PUT`/`DELETE /me/library/albums_and_playlists/{id}` — the same call `add_playlist_to_library` uses in Task 8. Implement it once in `library.rs` and call it from both.

- [ ] **Step 5: Verify**

Run: `cargo test -p music soundcloud && cargo check --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): read and write the library

Liked tracks, saved sets and followed artists, with one request behind
both saved_albums and playlists since SoundCloud stores them together."
```

---

## Task 8: Playlist editing

**Files:**
- Create: `crates/music/src/soundcloud/playlists.rs`
- Modify: `crates/music/src/soundcloud/client.rs`

**Interfaces:**
- Consumes: `http::Http`, `wire::*`, `library::set_saved` from Task 7.
- Produces:
  - `playlists::detail(http: &Http, id: &str) -> Result<crate::PlaylistDetail>`
  - `playlists::tracks(http: &Http, id: &str) -> Result<Vec<crate::Track>>`
  - `playlists::create(http: &Http, name: &str) -> Result<String>`
  - `playlists::rename(http: &Http, id: &str, name: &str) -> Result<()>`
  - `playlists::delete(http: &Http, id: &str) -> Result<()>`
  - `playlists::set_public(http: &Http, id: &str, public: bool) -> Result<()>`
  - `playlists::set_tracks(http: &Http, id: &str, ids: &[u64]) -> Result<()>`
  - `http::Http::put_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T>`
  - `http::Http::post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T>`

- [ ] **Step 1: Understand the shape of the edit API**

SoundCloud edits a set by sending its **whole** track list, not by patching one entry. So `add_track_to_playlist` and `remove_track_from_playlist` both read the current list, change it, and write it back. Write this as one private helper that takes a closure over the id list, so the read-modify-write happens in exactly one place:

```rust
async fn edit_tracks<F>(http: &Http, id: &str, change: F) -> Result<()>
where
    F: FnOnce(&mut Vec<u64>),
{
    let mut ids = track_ids(http, id).await?;
    change(&mut ids);
    set_tracks(http, id, &ids).await
}
```

This is the one place in the provider where a lost update is possible: two concurrent edits both read, both write, and the second wins. SoundCloud's own client has the same behaviour and the API offers no conditional write, so it is accepted rather than solved. Note it in the module doc comment so the next reader does not think it was missed.

- [ ] **Step 2: Write the failing test for the list edit**

The read-modify-write is pure once the list is in hand, so test the closure behaviour directly:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn appends_without_duplicating() {
        let mut ids = vec![1_u64, 2, 3];
        super::append(&mut ids, 4);
        assert_eq!(ids, vec![1, 2, 3, 4]);
        super::append(&mut ids, 2);
        assert_eq!(ids, vec![1, 2, 3, 4], "an existing track must not be added twice");
    }

    #[test]
    fn removes_every_occurrence() {
        let mut ids = vec![1_u64, 2, 3, 2];
        super::remove(&mut ids, 2);
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn removing_an_absent_track_changes_nothing() {
        let mut ids = vec![1_u64, 2];
        super::remove(&mut ids, 9);
        assert_eq!(ids, vec![1, 2]);
    }
}
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p music soundcloud::playlists`
Expected: FAIL — `cannot find function append`.

- [ ] **Step 4: Implement `append` and `remove`**

```rust
pub(crate) fn append(ids: &mut Vec<u64>, id: u64) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

pub(crate) fn remove(ids: &mut Vec<u64>, id: u64) {
    ids.retain(|kept| *kept != id);
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p music soundcloud::playlists`
Expected: PASS, three tests.

- [ ] **Step 6: Implement the network functions**

`detail` and `tracks` are `GET /playlists/{id}`. `create` is `POST /playlists` with `{"playlist": {"title": name, "sharing": "private", "tracks": []}}` and returns the new id. `rename` and `set_public` are `PUT /playlists/{id}` with the changed field. `delete` is `DELETE /playlists/{id}`. `set_tracks` is `PUT /playlists/{id}` with `{"playlist": {"tracks": [{"id": …}, …]}}`.

`album_tracks` reuses `tracks`, then fills each track's `album` and `album_id` from the set it just read — this is the asymmetry the spec calls out, and this is the only place it gets fixed.

`playlist_covers` calls `tracks` and passes the result to the existing `crate::distinct_covers` helper in `crates/music/src/lib.rs:45`. Do not write a second implementation.

- [ ] **Step 7: Wire the client methods**

Replace the stubs for `playlist`, `playlist_tracks`, `playlist_covers`, `album`, `album_tracks`, `create_playlist`, `rename_playlist`, `delete_playlist`, `set_playlist_public`, `add_track_to_playlist`, `remove_track_from_playlist`, `add_playlist_to_library` and `remove_playlist_from_library`.

- [ ] **Step 8: Verify**

Run: `cargo test -p music soundcloud && cargo check --workspace`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): create and edit sets

SoundCloud replaces a set's whole track list on every edit, so adding and
removing a track share one read-modify-write helper."
```

---

## Task 9: Artists, profiles and radio

**Files:**
- Create: `crates/music/src/soundcloud/users.rs`
- Modify: `crates/music/src/soundcloud/client.rs`, `crates/music/src/soundcloud/wire.rs`

**Interfaces:**
- Consumes: `http::Http`, `wire::{User, Track, Page}`.
- Produces:
  - `users::artist(http: &Http, id: &str) -> Result<crate::Artist>`
  - `users::profile(http: &Http, id: &str) -> Result<crate::ArtistProfile>`
  - `users::detail(http: &Http, id: &str) -> Result<crate::UserDetail>`
  - `users::top_tracks(http: &Http, id: &str) -> Result<Vec<crate::Track>>`
  - `users::images(http: &Http, ids: Vec<String>) -> Result<HashMap<String, String>>`
  - `users::related(http: &Http, track_id: &str) -> Result<Vec<crate::Track>>`

- [ ] **Step 1: Implement the reads**

`artist`, `profile` and `detail` all read `GET /users/{id}`; the difference is which model they fill. `top_tracks` is `GET /users/{id}/toptracks`, falling back to `/users/{id}/tracks` if Task 1 found the former missing. `related` is `GET /tracks/{id}/related` and is what `track_radio` returns — if Task 1 marked **A3** false, `track_radio` returns `Ok(Vec::new())` and this function is not written.

`images` fans out over the ids with a `tokio::task::JoinSet`, one `GET /users/{id}` each, collecting `id -> avatar_url`. This mirrors `spotify/profiles.rs`, which does the same thing for display names; read it before writing this.

- [ ] **Step 2: Implement `share_url`**

The one `MusicApi` method that needs no network. SoundCloud share links are permalinks, so the client caches the `permalink_url` it saw and returns it:

```rust
fn share_url(&self, _kind: MediaKind, id: &str) -> Option<String> {
    self.permalinks.lock().ok()?.get(id).cloned()
}
```

Populate the map in `wire` conversion. If that proves awkward, returning `None` is acceptable and the UI simply offers no share action — check what `views` does with `None` before deciding.

- [ ] **Step 3: Wire the client methods**

Replace the stubs for `artist`, `artist_profile`, `artist_images`, `user`, `track`, `track_playcount`, `track_radio` and `share_url`.

`profile()` on the trait returns the signed-in user: `GET /me` when authenticated, otherwise the guest profile from Task 2.

- [ ] **Step 4: Verify**

Run: `cargo test -p music soundcloud && cargo check --workspace`
Expected: PASS. At this point no `MusicApi` method should still say "not implemented yet" except the deliberate `bail!` ones for tag editing.

- [ ] **Step 5: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): artist pages, profiles and related-track radio"
```

---

## Task 10: Playback, progressive streams

**Files:**
- Create: `crates/music/src/soundcloud/playback.rs`
- Modify: `crates/music/src/soundcloud/mod.rs`

**Interfaces:**
- Consumes: `http::Http`, `wire::Track`, and `crate::audio::{Output, SmoothGain, Trimmed, Volume}`.
- Produces:
  - `playback::Factory::new(http: Arc<Http>) -> Factory` implementing `crate::PlaybackFactory`
  - `playback::pick(transcodings: &[wire::Transcoding]) -> Option<&wire::Transcoding>`
  - `wire::Transcoding` and `wire::Media` serde types

- [ ] **Step 1: Read the model first**

Open `crates/music/src/youtube/playback.rs` and read it end to end before writing anything. This task is that file with a different byte source. Copy its structure — the `Command` enum, the named thread, the `run` loop, the `Engine`/`Events` split — rather than inventing a second shape. `CLAUDE.md` is explicit that a view never drives a player; everything goes through `state::Playback`.

- [ ] **Step 2: Write the failing test for transcoding selection**

Selection is pure, so it is the testable part. Adjust the preset strings to what Task 1 observed.

```rust
#[cfg(test)]
mod tests {
    use super::pick;
    use crate::soundcloud::wire::{Format, Transcoding};

    fn transcoding(protocol: &str, preset: &str) -> Transcoding {
        Transcoding {
            url: format!("https://example.test/{preset}"),
            preset: preset.to_string(),
            format: Format { protocol: protocol.to_string() },
        }
    }

    #[test]
    fn prefers_progressive_over_hls() {
        let all = vec![
            transcoding("hls", "opus_0_0"),
            transcoding("progressive", "mp3_0_0"),
        ];
        assert_eq!(pick(&all).unwrap().format.protocol, "progressive");
    }

    #[test]
    fn falls_back_to_hls() {
        let all = vec![transcoding("hls", "opus_0_0")];
        assert_eq!(pick(&all).unwrap().format.protocol, "hls");
    }

    #[test]
    fn has_nothing_to_pick_from_an_empty_list() {
        assert!(pick(&[]).is_none());
    }
}
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p music soundcloud::playback`
Expected: FAIL — `cannot find function pick`.

- [ ] **Step 4: Implement selection and the wire types**

```rust
/// Chooses which transcoding to play.
///
/// Progressive is a single URL and decodes straight into rodio, so it wins
/// whenever it is offered. HLS needs the segment assembly in `stream.rs`.
pub fn pick(transcodings: &[Transcoding]) -> Option<&Transcoding> {
    transcodings
        .iter()
        .find(|t| t.format.protocol == "progressive")
        .or_else(|| transcodings.iter().find(|t| t.format.protocol == "hls"))
}
```

In `wire.rs`, add `Media { transcodings: Vec<Transcoding> }`, `Transcoding { url, preset, format }` and `Format { protocol }`, all `#[serde(default)]` on the optional parts, and add `#[serde(default)] pub media: Media` to `wire::Track`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p music soundcloud::playback`
Expected: PASS, three tests.

- [ ] **Step 6: Implement the engine**

Resolving a track to bytes is three steps: `GET /tracks/{id}` for the transcodings, `pick`, then `GET <transcoding.url>` which returns `{"url": "<cdn url>"}`, then the CDN URL for the audio itself. Read the body into a `Vec<u8>`, wrap it in `std::io::Cursor`, and hand it to rodio exactly as the YouTube engine does.

Gapless reuses `crate::youtube::trim` — the encoder padding it strips is not YouTube-specific. If the module is private, make it `pub(crate)` and move it to `crates/music/src/audio.rs`; do not copy it.

Normalisation honours `PlaybackConfig.gain` only. The v2 API carries no per-track loudness, so there is nothing to compute a per-track gain from. Say so in a comment at the point where the YouTube engine applies its own, so the difference reads as a decision rather than an omission.

- [ ] **Step 7: Wire the factory into the sessions**

In `mod.rs`, fill in `playback: Arc::new(Factory::new(http.clone()))` in both the guest and authenticated `ProviderSession`.

- [ ] **Step 8: Verify by hand**

Run: `cargo run --release`

Sign in to SoundCloud anonymously, search for a track, play it. Confirm audio comes out, the position advances, seek works, and pause and resume work.

- [ ] **Step 9: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): play progressive streams

Prefers a progressive transcoding and decodes it through the shared audio
stack. Per-track normalisation is not possible; the v2 API exposes no
loudness metadata."
```

---

## Task 11: HLS streams

Only needed if Task 1 found tracks that offer no progressive transcoding. If every captured track had one, skip this task and record why in the commit that closes the branch.

**Files:**
- Create: `crates/music/src/soundcloud/stream.rs`
- Modify: `crates/music/src/soundcloud/playback.rs`

**Interfaces:**
- Consumes: `wire::Transcoding` from Task 10.
- Produces: `stream::segments(playlist: &str) -> Vec<String>` and `stream::assemble(http: &reqwest::Client, url: &str) -> Result<Vec<u8>>`

- [ ] **Step 1: Write the failing test**

Parsing an m3u8 is pure string work:

```rust
#[cfg(test)]
mod tests {
    use super::segments;

    #[test]
    fn lists_segments_in_order() {
        let playlist = "#EXTM3U\n\
                        #EXT-X-VERSION:3\n\
                        #EXTINF:10.0,\n\
                        https://cf-hls.sndcdn.com/a/0.aac\n\
                        #EXTINF:10.0,\n\
                        https://cf-hls.sndcdn.com/a/1.aac\n\
                        #EXT-X-ENDLIST\n";
        assert_eq!(
            segments(playlist),
            vec![
                "https://cf-hls.sndcdn.com/a/0.aac".to_string(),
                "https://cf-hls.sndcdn.com/a/1.aac".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        assert!(segments("#EXTM3U\n\n#EXT-X-ENDLIST\n").is_empty());
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p music soundcloud::stream`
Expected: FAIL — `cannot find function segments`.

- [ ] **Step 3: Implement**

```rust
/// Reads the segment URLs out of a media playlist.
///
/// This is not an adaptive HLS client. SoundCloud hands out a single-variant
/// media playlist per transcoding, so every non-comment line is a segment and
/// the quality was already chosen by `playback::pick`.
pub fn segments(playlist: &str) -> Vec<String> {
    playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}
```

`assemble` fetches the playlist, calls `segments`, downloads each in order, and concatenates the bodies into one `Vec<u8>`. Sequential, not parallel: the segments must land in order and a track is a handful of them.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p music soundcloud::stream`
Expected: PASS, two tests.

- [ ] **Step 5: Use it from the engine**

In `playback.rs`, when the picked transcoding's protocol is `hls`, route through `stream::assemble` instead of a single GET. The rest of the path is unchanged — the result is still a `Vec<u8>` in a `Cursor`.

- [ ] **Step 6: Verify by hand**

Run: `cargo run --release` and play a track that Task 1 recorded as HLS-only.

- [ ] **Step 7: Commit**

```bash
git add crates/music/src/soundcloud
git commit -m "feat(soundcloud): play HLS-only tracks

Assembles the single-variant media playlist into one buffer. Not an
adaptive client; the quality is already fixed by transcoding selection."
```

---

## Task 12: Provider-neutral sign-in failures

`crates/music/src/lib.rs` hardcodes Spotify into three of the six `SignInFailure` messages. Two providers never exposed it because YouTube uses none of those variants. SoundCloud does, so a network failure currently renders as "Spotify could not be reached".

**Files:**
- Modify: `crates/music/src/lib.rs:230-240`

**Interfaces:**
- Consumes: nothing.
- Produces: no signature change. `SignInFailure` and `SignInProblem` keep their shape; only the rendered text changes.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod sign_in_failure_tests {
    use super::{SignInFailure, SignInProblem};

    #[test]
    fn names_no_provider() {
        let problems = [
            SignInProblem::Premium,
            SignInProblem::Region,
            SignInProblem::Credentials,
            SignInProblem::Network,
            SignInProblem::Cancelled,
            SignInProblem::Refused,
        ];
        for problem in problems {
            let message = SignInFailure(problem).to_string();
            assert!(
                !message.contains("Spotify"),
                "{problem:?} still names a provider: {message}"
            );
        }
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p music sign_in_failure`
Expected: FAIL on `Premium`, with the message naming Spotify.

- [ ] **Step 3: Neutralise the wording**

```rust
let reason = match self.0 {
    SignInProblem::Premium => "the account has no premium subscription",
    SignInProblem::Region => "the account is out of its home region",
    SignInProblem::Credentials => "the stored credentials are no longer valid",
    SignInProblem::Network => "the service could not be reached",
    SignInProblem::Cancelled => "authorization was cancelled in the browser",
    SignInProblem::Refused => "the service refused the session",
};
```

Threading a provider name through `SignInFailure` would read better still, but it widens the change into every construction site. Out of scope here; the spec records the decision.

- [ ] **Step 4: Run the test**

Run: `cargo test -p music sign_in_failure`
Expected: PASS.

- [ ] **Step 5: Check nothing asserted on the old text**

Run: `grep -rn "Spotify could not be reached\|has no Spotify Premium\|Spotify refused" crates/`
Expected: no hits outside the file just edited.

- [ ] **Step 6: Commit**

```bash
git add crates/music/src/lib.rs
git commit -m "fix(music): stop naming Spotify in shared sign-in errors

Three of the six SignInFailure messages hardcoded Spotify. A failure from
any other provider rendered as a Spotify failure."
```

---

## Task 13: Icon, translations and polish

**Files:**
- Create: `assets/icons/common/soundcloud.svg`
- Modify: `assets/icons/common/LICENSE`
- Modify: `crates/views/src/shared/mod.rs:56`
- Modify: `assets/i18n/en-US/main.ftl`

**Interfaces:**
- Consumes: `SoundCloudProvider::slug()` from Task 2.
- Produces: no code interface. This is the change that makes the provider look finished.

- [ ] **Step 1: Add the brand mark**

Take the SoundCloud mark from Simple Icons (CC0) and save it as `assets/icons/common/soundcloud.svg`. Match the shape of the existing `spotify.svg`: a bare `<svg>` with a single `<path>`, no fixed `fill`, so the theme colours it.

- [ ] **Step 2: Update the icon licence note**

`assets/icons/common/LICENSE` names the brand marks it covers. Extend the sentence to include soundcloud:

> The brand marks (firefoxbrowser, soundcloud, spotify, youtubemusic) come from Simple Icons

- [ ] **Step 3: Map the slug to the icon**

`crates/views/src/shared/mod.rs:56`:

```rust
pub(crate) fn provider_logo(slug: &str) -> &'static str {
    match slug {
        "soundcloud" => "icons/soundcloud.svg",
        "spotify" => "icons/spotify.svg",
        "youtube" => "icons/youtubemusic.svg",
        _ => "icons/music.svg",
    }
}
```

- [ ] **Step 4: Add the sign-in strings**

`assets/i18n/en-US/main.ftl` needs a string for the token paste prompt, in the style of the existing `login-secret` entries. Read the surrounding block before writing so the key naming matches. Leave the nine other languages untouched; the README coverage table regenerates from them.

- [ ] **Step 5: Verify by hand**

Run: `cargo run --release`

Confirm the SoundCloud logo appears next to the provider in the sign-in list and in the library sidebar, and that the paste prompt reads correctly in English.

- [ ] **Step 6: Commit**

```bash
git add assets crates/views/src/shared/mod.rs
git commit -m "feat(soundcloud): add the brand mark and sign-in strings"
```

---

## Task 14: Live tests

**Files:**
- Create: `crates/music/src/live_tests/soundcloud.rs`
- Modify: `crates/music/src/live_tests/mod.rs`

**Interfaces:**
- Consumes: the whole provider.
- Produces: nothing other tasks use.

- [ ] **Step 1: Read the three existing live tests**

`crates/music/src/live_tests/{add_album,follow_artist,playlist_privacy}.rs`. Match their shape: `#[cfg(test)]`, credentials from the environment, skipped when absent. Do not invent a second convention.

- [ ] **Step 2: Write the tests**

Four, each one round trip:
1. Anonymous sign-in harvests a `client_id` and a search returns tracks.
2. A track resolves to a playable transcoding URL.
3. With a token in the environment, liked tracks come back non-empty.
4. Creating a set, adding a track, removing it, and deleting the set all succeed.

Test 4 leaves nothing behind; delete the set in the same test even if an assertion fails, so a failure does not litter the account.

- [ ] **Step 3: Run them**

Run: `cargo test -p music live_tests::soundcloud -- --ignored --nocapture`
Expected: PASS with credentials present, skipped without.

- [ ] **Step 4: Commit**

```bash
git add crates/music/src/live_tests
git commit -m "test(soundcloud): cover search, streaming and set editing"
```

---

## Task 15: Ready the branch

- [ ] **Step 1: Full check**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must be clean. `rustfmt.toml` is in the repo root; do not override it.

- [ ] **Step 2: Walk the manual checklist**

The spec lists these; run every one against a release build and record the result.

```
[ ] sign in anonymously            [ ] sign in with a pasted token
[ ] sign in via browser            (skip if A8 was false)
[ ] play a public track            [ ] play a track from a set
[ ] like a track, then unlike it   [ ] create a set, then delete it
[ ] provider logo shows in the sign-in list and the sidebar
[ ] a forced network failure no longer names Spotify
```

The last one needs provoking: sign in with the machine offline, and read the error the UI shows.

- [ ] **Step 3: Take the design docs out of the PR**

`docs/superpowers/` is working material, not upstream content. Move both documents off the branch before opening the PR:

```bash
git rm -r --cached docs/superpowers
```

Keep them on a local branch or outside the repository. Confirm with `git diff --stat main...HEAD` that the diff contains only `crates/`, `assets/` and `Cargo` changes.

- [ ] **Step 4: Open the upstream issue first**

No SoundCloud issue exists on `nolight132/sonora`. Open one before the PR, covering: why SoundCloud, why the v2 API rather than the official one, and the two limitations (no per-track normalisation, Go+ tracks unplayable).

The repository's AI policy requires the reasoning and motivation in that issue and in the PR to be the contributor's own. This plan and its spec are input to that, not text to paste.

- [ ] **Step 5: Open the PR once the issue has an answer**

Title: `feat: add a SoundCloud provider`. Link the issue. Note the `SignInFailure` change explicitly — it touches shared code and a reviewer should see it called out rather than find it.
