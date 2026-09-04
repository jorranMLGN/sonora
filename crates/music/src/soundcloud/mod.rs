mod auth;
mod client;
mod http;
mod library;
mod playlists;
mod search;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    InputSource, MusicProvider, PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory,
    Player, PromptSink, ProviderSession, SignIn, SignInFailure, SignInProblem, SignInPrompt,
    UserProfile,
};
pub use client::SoundCloudClient;
use http::Http;

const GUEST_ID: &str = "soundcloud-guest";

#[derive(Deserialize)]
struct Me {
    id: i64,
    username: String,
}

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

    async fn client_id(&self) -> Result<String> {
        if let Ok(cached) = std::fs::read_to_string(&self.client_id) {
            let cached = cached.trim().to_string();
            if !cached.is_empty() {
                return Ok(cached);
            }
        }
        let harvested = auth::harvest_client_id()
            .await
            .context("cannot harvest the soundcloud client id")?;
        self.store_client_id(&harvested)?;
        Ok(harvested)
    }

    fn store_client_id(&self, id: &str) -> Result<()> {
        if let Some(parent) = self.client_id.parent() {
            std::fs::create_dir_all(parent).context("cannot create soundcloud cache dir")?;
        }
        std::fs::write(&self.client_id, id).context("cannot store the soundcloud client id")
    }

    fn store_token(&self, token: &str) -> Result<()> {
        if let Some(parent) = self.token.parent() {
            std::fs::create_dir_all(parent).context("cannot create soundcloud cache dir")?;
        }
        std::fs::write(&self.token, token).context("cannot store the soundcloud token")
    }

    async fn authenticate(&self, client_id: String, token: String) -> Result<(Http, UserProfile)> {
        let http = Http::with_token(client_id, token.clone());
        match fetch_profile(&http).await {
            Ok(profile) => Ok((http, profile)),
            Err(error) if error.downcast_ref::<http::AuthRejected>().is_some() => {
                log::debug!("soundcloud: client id was rejected, re-harvesting once");
                let fresh = auth::harvest_client_id()
                    .await
                    .map_err(|_| anyhow::Error::new(SignInFailure(SignInProblem::Network)))?;
                self.store_client_id(&fresh)?;
                let http = Http::with_token(fresh, token);
                fetch_profile(&http)
                    .await
                    .map(|profile| (http, profile))
                    .map_err(|error| classify(error, true))
            }
            Err(error) => Err(classify(error, false)),
        }
    }

    fn guest_session(&self, client_id: String) -> ProviderSession {
        let http = Arc::new(Http::anonymous(client_id));
        ProviderSession {
            profile: UserProfile {
                id: GUEST_ID.to_string(),
                display_name: "SoundCloud".to_string(),
            },
            api: Arc::new(SoundCloudClient::new(http)),
            playback: Arc::new(Factory),
            authenticated: false,
            playcounts: false,
        }
    }

    fn authenticated_session(&self, http: Http, profile: UserProfile) -> ProviderSession {
        let client = SoundCloudClient::new(Arc::new(http)).as_user(profile.id.clone());
        ProviderSession {
            profile,
            api: Arc::new(client),
            playback: Arc::new(Factory),
            authenticated: true,
            playcounts: false,
        }
    }
}

/// Maps a `/me` failure to a `SignInFailure`, or passes it through unchanged.
///
/// `rejected_is_credentials` distinguishes the first attempt, where an
/// `AuthRejected` still might just mean a stale `client_id` and triggers a
/// retry, from the retry itself, where the client id was just re-harvested
/// and a repeat rejection can only mean a bad token.
fn classify(error: anyhow::Error, rejected_is_credentials: bool) -> anyhow::Error {
    if rejected_is_credentials && error.downcast_ref::<http::AuthRejected>().is_some() {
        return anyhow::Error::new(SignInFailure(SignInProblem::Credentials));
    }
    if error.downcast_ref::<http::Unreachable>().is_some() {
        return anyhow::Error::new(SignInFailure(SignInProblem::Network));
    }
    error
}

async fn fetch_profile(http: &Http) -> Result<UserProfile> {
    let me: Me = http.get_json("/me", &[]).await?;
    Ok(UserProfile {
        id: me.id.to_string(),
        display_name: me.username,
    })
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
        let Ok(client_id) = std::fs::read_to_string(&self.client_id) else {
            return Ok(None);
        };
        let client_id = client_id.trim().to_string();
        if client_id.is_empty() {
            return Ok(None);
        }
        if let Ok(token) = std::fs::read_to_string(&self.token) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                match self.authenticate(client_id.clone(), token).await {
                    Ok((http, profile)) => {
                        log::debug!("soundcloud: restored the authenticated session");
                        return Ok(Some(self.authenticated_session(http, profile)));
                    }
                    Err(error) => {
                        if let Some(SignInFailure(SignInProblem::Credentials)) =
                            error.downcast_ref::<SignInFailure>()
                        {
                            log::debug!("soundcloud: dropping the rejected token");
                            let _ = std::fs::remove_file(&self.token);
                        }
                        log::warn!("soundcloud: cannot restore the session: {error:#}");
                    }
                }
            }
        }
        log::debug!("soundcloud: restoring guest session");
        Ok(Some(self.guest_session(client_id)))
    }

    async fn sign_in(
        &self,
        method: SignIn,
        prompt: PromptSink,
        mut input: InputSource,
    ) -> Result<ProviderSession> {
        match method {
            SignIn::Anonymous | SignIn::Default => {
                let client_id = self
                    .client_id()
                    .await
                    .map_err(|_| anyhow::Error::new(SignInFailure(SignInProblem::Network)))?;
                log::debug!("soundcloud: guest sign-in succeeded");
                Ok(self.guest_session(client_id))
            }
            SignIn::Secret => {
                prompt(SignInPrompt::Secret);
                let token = input.recv().await.context("sign-in was cancelled")?;
                let token = token.trim().to_string();
                let client_id = self
                    .client_id()
                    .await
                    .map_err(|_| anyhow::Error::new(SignInFailure(SignInProblem::Network)))?;
                let (http, profile) = self.authenticate(client_id, token.clone()).await?;
                self.store_token(&token)?;
                log::debug!("soundcloud: token sign-in succeeded");
                Ok(self.authenticated_session(http, profile))
            }
            SignIn::Browser(_) | SignIn::Path(_) => Err(anyhow::anyhow!(
                "soundcloud does not support this sign-in method"
            )),
        }
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

struct Factory;

impl PlaybackFactory for Factory {
    fn start(&self, _config: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        (Box::new(NoPlayer), Box::new(NoEvents))
    }
}

struct NoPlayer;

impl Player for NoPlayer {
    fn load(&self, _track_id: &str, _seamless: bool) -> Result<()> {
        anyhow::bail!("soundcloud playback is not implemented yet")
    }

    fn preload(&self, _track_id: &str) -> Result<()> {
        anyhow::bail!("soundcloud playback is not implemented yet")
    }

    fn play(&self) {}
    fn pause(&self) {}
    fn seek(&self, _position: Duration) {}
    fn set_gain(&self, _gain: f32) {}
}

struct NoEvents;

#[async_trait]
impl PlaybackEvents for NoEvents {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{SoundCloudProvider, classify, http};
    use crate::{MusicProvider, SignInFailure, SignInProblem};

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

    #[test]
    fn a_first_rejection_is_not_yet_a_credentials_problem() {
        let error = classify(anyhow::Error::new(http::AuthRejected), false);
        assert!(error.downcast_ref::<SignInFailure>().is_none());
        assert!(error.downcast_ref::<http::AuthRejected>().is_some());
    }

    #[test]
    fn a_rejection_after_the_retry_is_a_credentials_problem() {
        let error = classify(anyhow::Error::new(http::AuthRejected), true);
        assert_eq!(
            error.downcast_ref::<SignInFailure>(),
            Some(&SignInFailure(SignInProblem::Credentials))
        );
    }

    #[test]
    fn an_unreachable_service_is_always_a_network_problem() {
        for rejected_is_credentials in [false, true] {
            let error = classify(
                anyhow::Error::new(http::Unreachable),
                rejected_is_credentials,
            );
            assert_eq!(
                error.downcast_ref::<SignInFailure>(),
                Some(&SignInFailure(SignInProblem::Network))
            );
        }
    }

    #[test]
    fn any_other_error_passes_through_unchanged() {
        let error = classify(anyhow::anyhow!("cannot read the soundcloud response"), true);
        assert!(error.downcast_ref::<SignInFailure>().is_none());
        assert_eq!(error.to_string(), "cannot read the soundcloud response");
    }
}
