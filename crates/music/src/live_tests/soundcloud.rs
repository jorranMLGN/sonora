//! Live round trips against the real SoundCloud v2 API.
//!
//! Every other soundcloud test in this crate runs against a captured fixture;
//! nothing here has ever spoken to the live service, and in particular no
//! write endpoint on this API has ever been exercised against a real
//! account — verifying one modifies that account, which nobody had
//! authorised until now.
//!
//! A GET probe (see `task-14-brief.md`) found all three write paths this
//! branch currently calls answering 404, where a route that merely needs
//! authentication answers 401:
//!
//! | path                                      | GET status |
//! | ------------------------------------------ | ---------- |
//! | `/likes/tracks/{id}`                        | 404        |
//! | `/me/followings/{id}`                       | 404        |
//! | `/me/library/albums_and_playlists/{id}`     | 404        |
//!
//! So these tests are expected to fail on their first run, and that failure
//! is them doing their job: every assertion here names the method, the path
//! and the status it got back, so a failure hands over the answer instead of
//! costing someone an afternoon. **Do not chase a fix by trying candidate
//! paths against the real account** — report exactly what was sent and
//! received, and stop.
//!
//! `crates/music/src/soundcloud/*` keeps its modules private to that
//! provider, the same as `spotify` and `youtube` do, so — like the other
//! three files in this module — these tests drive the provider only through
//! `MusicProvider`/`MusicApi`. The one exception is the anonymous-vs-token
//! comparison in the third test: that claim (a public library is readable
//! without a token) has no public accessor for "somebody else's session with
//! no token", so that one test reads the `client_id`/token cache files
//! `SoundCloudProvider` itself writes to `dirs::cache_dir()/sonora/soundcloud`
//! (documented in `global-constraints.md`) and issues the comparison request
//! directly. Nothing here reads a browser profile, cookie store or keychain,
//! and no credential is ever written to a file, a fixture or a report.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use tokio::sync::mpsc::unbounded_channel;

use crate::soundcloud::SoundCloudProvider;
use crate::{InputSource, MusicApi, MusicProvider, PromptSink, ProviderSession, SignIn};

/// Mirrors `crates/music/src/soundcloud/http.rs`'s own `BASE`, which is
/// private to that module. Duplicated here rather than exposed, since these
/// tests otherwise drive the provider only through its public surface — see
/// the module doc comment.
const BASE: &str = "https://api-v2.soundcloud.com";

/// Track 293, "Flickermood" by forss — the fixture track this whole branch
/// was built and measured against (`crates/music/src/soundcloud/fixtures/track.json`,
/// `task-10-brief.md`'s measured transcoding table). Public, long-lived, and
/// already the anchor id used throughout this provider's planning docs.
const TRACK_ID: &str = "293";
const SEARCH_QUERY: &str = "forss flickermood";

const VERIFY_ATTEMPTS: usize = 30;

fn no_prompt() -> PromptSink {
    Arc::new(|_| {})
}

fn no_input() -> InputSource {
    let (_tx, rx) = unbounded_channel();
    rx
}

async fn connected(provider: &dyn MusicProvider) -> Result<ProviderSession> {
    let session = provider
        .restore()
        .await?
        .with_context(|| format!("{} has no stored Sonora session", provider.name()))?;
    if !session.authenticated {
        bail!(
            "{} restored a guest session, not an account",
            provider.name()
        );
    }
    Ok(session)
}

/// Reads a file `SoundCloudProvider` caches under
/// `dirs::cache_dir()/sonora/soundcloud`, trimmed.
fn read_cache(name: &str) -> Result<String> {
    let path = dirs::cache_dir()
        .context("no cache dir on this platform")?
        .join("sonora")
        .join("soundcloud")
        .join(name);
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let trimmed = contents.trim().to_string();
    if trimmed.is_empty() {
        bail!("{} is empty", path.display());
    }
    Ok(trimmed)
}

#[tokio::test]
#[ignore = "makes live network requests to the soundcloud api"]
async fn anonymous_sign_in_harvests_a_client_id_and_a_search_returns_tracks() -> Result<()> {
    let provider = SoundCloudProvider::new();
    let session = provider
        .sign_in(SignIn::Anonymous, no_prompt(), no_input())
        .await
        .context("GET client_id harvest failed")?;
    assert!(
        !session.authenticated,
        "an anonymous sign-in must not report an authenticated session"
    );

    let tracks = session
        .api
        .search(SEARCH_QUERY)
        .await
        .context(format!("GET {BASE}/search/tracks failed"))?;
    assert!(
        !tracks.is_empty(),
        "GET {BASE}/search/tracks for {SEARCH_QUERY:?} returned no tracks"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "makes live network requests to the soundcloud api"]
async fn a_track_resolves_to_a_playable_transcoding_url() -> Result<()> {
    let provider = SoundCloudProvider::new();
    provider
        .sign_in(SignIn::Anonymous, no_prompt(), no_input())
        .await
        .context("anonymous sign-in failed while harvesting a client_id")?;
    let client_id = read_cache("client_id.txt")?;

    let agent = reqwest::Client::new();

    #[derive(serde::Deserialize)]
    struct Track {
        media: Media,
    }
    #[derive(serde::Deserialize)]
    struct Media {
        transcodings: Vec<Transcoding>,
    }
    #[derive(serde::Deserialize, Clone)]
    struct Transcoding {
        url: String,
        format: Format,
    }
    #[derive(serde::Deserialize, Clone)]
    struct Format {
        protocol: String,
    }
    #[derive(serde::Deserialize)]
    struct Resolved {
        url: String,
    }

    let track_path = format!("/tracks/{TRACK_ID}");
    let response = agent
        .get(format!("{BASE}{track_path}"))
        .query(&[("client_id", client_id.as_str())])
        .send()
        .await
        .with_context(|| format!("GET {track_path} could not reach soundcloud"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("GET {track_path} -> {status}");
    }
    let track: Track = response
        .json()
        .await
        .with_context(|| format!("GET {track_path} -> {status}, but the body did not parse"))?;

    // Prefer hls, the way `soundcloud::playback::pick` does: it is the only
    // protocol behind the better-quality presets.
    let chosen = track
        .media
        .transcodings
        .iter()
        .find(|t| t.format.protocol == "hls")
        .or_else(|| track.media.transcodings.first())
        .with_context(|| format!("GET {track_path} -> {status}, but it offers no transcodings"))?
        .clone();

    let response = agent
        .get(&chosen.url)
        .query(&[("client_id", client_id.as_str())])
        .send()
        .await
        .with_context(|| format!("GET {} could not reach soundcloud", chosen.url))?;
    let status = response.status();
    if !status.is_success() {
        bail!("GET {} -> {status}", chosen.url);
    }
    let resolved: Resolved = response.json().await.with_context(|| {
        format!(
            "GET {} -> {status}, but the body was not {{\"url\": …}}",
            chosen.url
        )
    })?;

    let response = agent
        .get(&resolved.url)
        .send()
        .await
        .context("could not reach the resolved cdn url")?;
    let status = response.status();
    assert!(
        status.is_success(),
        "GET {} (the resolved transcoding url) -> {status}",
        resolved.url
    );
    Ok(())
}

#[tokio::test]
#[ignore = "reads the connected soundcloud account's likes, once with a token and once without"]
async fn a_users_track_likes_are_readable_with_and_without_a_token() -> Result<()> {
    let provider = SoundCloudProvider::new();
    let session = connected(&provider).await?;
    let user_id = session.profile.id.clone();

    let client_id = read_cache("client_id.txt")?;
    let token = read_cache("token.txt")?;

    let path = format!("/users/{user_id}/track_likes");
    let agent = reqwest::Client::new();

    let anonymous = agent
        .get(format!("{BASE}{path}"))
        .query(&[("client_id", client_id.as_str())])
        .send()
        .await
        .with_context(|| format!("GET {path} (anonymous) could not reach soundcloud"))?;
    let anonymous_status = anonymous.status();
    assert!(
        anonymous_status.is_success(),
        "GET {path} without a token -> {anonymous_status}; a public library read should not require one"
    );

    let with_token = agent
        .get(format!("{BASE}{path}"))
        .query(&[("client_id", client_id.as_str())])
        .header("Authorization", format!("OAuth {token}"))
        .send()
        .await
        .with_context(|| format!("GET {path} (with a token) could not reach soundcloud"))?;
    let with_token_status = with_token.status();
    assert!(
        with_token_status.is_success(),
        "GET {path} with a token -> {with_token_status}"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "creates a set, adds and removes a track, and deletes the set on the connected soundcloud account"]
async fn a_set_can_be_created_edited_and_deleted() -> Result<()> {
    let provider = SoundCloudProvider::new();
    let session = connected(&provider).await?;
    let api = session.api.as_ref();

    let playlist_id = api
        .create_playlist("Sonora live test — safe to delete")
        .await
        .context("POST /playlists")?;

    let exercise = async {
        api.add_track_to_playlist(&playlist_id, TRACK_ID)
            .await
            .with_context(|| format!("PUT /playlists/{playlist_id} (adding track {TRACK_ID})"))?;
        wait_until_playlist_has_track(api, &playlist_id, true).await?;

        api.remove_track_from_playlist(&playlist_id, TRACK_ID)
            .await
            .with_context(|| format!("PUT /playlists/{playlist_id} (removing track {TRACK_ID})"))?;
        wait_until_playlist_has_track(api, &playlist_id, false).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    let cleanup = api
        .delete_playlist(&playlist_id)
        .await
        .with_context(|| format!("DELETE /playlists/{playlist_id}"));

    match (exercise, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error.context("set edit cycle failed; the set was deleted")),
        (Ok(()), Err(error)) => {
            Err(error.context("set edit cycle passed but deleting the set afterwards failed"))
        }
        (Err(exercise), Err(cleanup)) => Err(anyhow::anyhow!(
            "set edit cycle failed: {exercise:#}; deleting the set also failed: {cleanup:#}"
        )),
    }
}

async fn wait_until_playlist_has_track(
    api: &dyn MusicApi,
    playlist_id: &str,
    expected: bool,
) -> Result<()> {
    for _ in 0..VERIFY_ATTEMPTS {
        let tracks = api
            .playlist_tracks(playlist_id)
            .await
            .with_context(|| format!("GET /playlists/{playlist_id}"))?;
        let has_track = tracks
            .iter()
            .any(|track| track.id.as_deref() == Some(TRACK_ID));
        if has_track == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    bail!(
        "GET /playlists/{playlist_id} never reported track {TRACK_ID} as {}",
        if expected { "present" } else { "removed" }
    )
}
