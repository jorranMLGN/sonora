mod client;

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::{InputSource, MusicProvider, PromptSink, ProviderSession, SignIn, UserProfile};
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
