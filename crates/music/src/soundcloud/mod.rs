mod auth;
mod client;
mod http;

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
                let profile = fetch_profile(&http)
                    .await
                    .map_err(|_| anyhow::Error::new(SignInFailure(SignInProblem::Network)))?;
                Ok((http, profile))
            }
            Err(_) => Err(anyhow::Error::new(SignInFailure(SignInProblem::Network))),
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
