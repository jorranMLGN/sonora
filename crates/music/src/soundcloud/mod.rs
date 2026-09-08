mod auth;
mod client;
mod http;
mod library;
mod playback;
mod playlists;
mod search;
mod stream;
mod users;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    InputSource, MusicProvider, PromptSink, ProviderSession, SignIn, SignInFailure, SignInProblem,
    SignInPrompt, UserProfile,
};
use auth::ClientId;
pub use client::SoundCloudClient;
use http::Http;
#[cfg(test)]
pub(crate) use playback::resolve_playable_url;

pub(crate) const GUEST_ID: &str = "soundcloud-guest";

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

    async fn client_id(&self) -> Result<ClientId> {
        if let Ok(cached) = std::fs::read_to_string(&self.client_id) {
            let cached = cached.trim().to_string();
            if !cached.is_empty() {
                return Ok(ClientId::new(cached, self.client_id.clone()));
            }
        }
        let harvested = auth::harvest_client_id()
            .await
            .context("cannot harvest the soundcloud client id")?;
        self.store_client_id(&harvested)?;
        Ok(ClientId::new(harvested, self.client_id.clone()))
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

    /// `Http` itself refreshes `client_id` once and retries once on an
    /// `AuthRejected` (see `http::Http::execute`), so any `AuthRejected`
    /// that reaches this point already survived that retry and can only
    /// mean the token is bad.
    async fn authenticate(
        &self,
        client_id: ClientId,
        token: String,
        pasted: bool,
    ) -> Result<(Http, UserProfile)> {
        let http = Http::with_token(client_id, token);
        fetch_profile(&http)
            .await
            .map(|profile| (http, profile))
            .map_err(|error| classify(error, pasted))
    }

    fn guest_session(&self, client_id: ClientId, expired: bool) -> ProviderSession {
        let http = Arc::new(Http::anonymous(client_id));
        ProviderSession {
            profile: UserProfile {
                id: GUEST_ID.to_string(),
                display_name: "SoundCloud".to_string(),
            },
            playback: Arc::new(playback::Factory::new(http.clone())),
            api: Arc::new(SoundCloudClient::new(http)),
            authenticated: false,
            playcounts: false,
            expired,
        }
    }

    fn authenticated_session(&self, http: Http, profile: UserProfile) -> ProviderSession {
        let http = Arc::new(http);
        let client = SoundCloudClient::new(http.clone()).as_user(profile.id.clone());
        ProviderSession {
            profile,
            api: Arc::new(client),
            playback: Arc::new(playback::Factory::new(http)),
            authenticated: true,
            playcounts: false,
            expired: false,
        }
    }
}

/// Maps a `/me` failure to a `SignInFailure`, or passes it through unchanged.
///
/// By the time an `AuthRejected` reaches here, `Http` has already refreshed
/// the client id once and retried once, so a repeat rejection can only mean
/// the token itself is bad. Which advice that deserves depends on where the
/// token came from: a token the user just pasted was mistyped, while a stored
/// one has expired.
fn classify(error: anyhow::Error, pasted: bool) -> anyhow::Error {
    if error.downcast_ref::<http::AuthRejected>().is_some() {
        let problem = match pasted {
            true => SignInProblem::Secret,
            false => SignInProblem::Credentials,
        };
        return anyhow::Error::new(SignInFailure(problem));
    }
    if error.downcast_ref::<http::Unreachable>().is_some() {
        return anyhow::Error::new(SignInFailure(SignInProblem::Network));
    }
    error
}

pub(crate) async fn fetch_profile(http: &Http) -> Result<UserProfile> {
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
        let client_id = ClientId::new(client_id, self.client_id.clone());
        let mut expired = false;
        if let Ok(token) = std::fs::read_to_string(&self.token) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                match self.authenticate(client_id.clone(), token, false).await {
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
                            expired = true;
                        }
                        log::warn!("soundcloud: cannot restore the session: {error:#}");
                    }
                }
            }
        }
        log::debug!("soundcloud: restoring guest session");
        Ok(Some(self.guest_session(client_id, expired)))
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
                Ok(self.guest_session(client_id, false))
            }
            SignIn::Secret => {
                prompt(SignInPrompt::Secret);
                let pasted = input.recv().await.context("sign-in was cancelled")?;
                let token = auth::token(&pasted)?;
                let client_id = self
                    .client_id()
                    .await
                    .map_err(|_| anyhow::Error::new(SignInFailure(SignInProblem::Network)))?;
                let (http, profile) = self.authenticate(client_id, token.clone(), true).await?;
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
    fn a_rejected_stored_token_is_a_credentials_problem() {
        // `Http` already refreshed the client id and retried once before an
        // `AuthRejected` ever reaches `classify`, so it can only mean the
        // token itself is bad.
        let error = classify(anyhow::Error::new(http::AuthRejected), false);
        assert_eq!(
            error.downcast_ref::<SignInFailure>(),
            Some(&SignInFailure(SignInProblem::Credentials))
        );
    }

    #[test]
    fn a_rejected_paste_blames_the_paste() {
        let error = classify(anyhow::Error::new(http::AuthRejected), true);
        assert_eq!(
            error.downcast_ref::<SignInFailure>(),
            Some(&SignInFailure(SignInProblem::Secret))
        );
    }

    #[test]
    fn an_unreachable_service_is_a_network_problem() {
        let error = classify(anyhow::Error::new(http::Unreachable), false);
        assert_eq!(
            error.downcast_ref::<SignInFailure>(),
            Some(&SignInFailure(SignInProblem::Network))
        );
    }

    #[test]
    fn any_other_error_passes_through_unchanged() {
        let error = classify(anyhow::anyhow!("cannot read the soundcloud response"), true);
        assert!(error.downcast_ref::<SignInFailure>().is_none());
        assert_eq!(error.to_string(), "cannot read the soundcloud response");
    }
}
