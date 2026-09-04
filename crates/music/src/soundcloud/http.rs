use anyhow::{Context as _, Result};
use serde::de::DeserializeOwned;

const BASE: &str = "https://api-v2.soundcloud.com";

#[derive(Debug)]
pub struct AuthRejected;

impl std::fmt::Display for AuthRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "soundcloud rejected the client id")
    }
}

impl std::error::Error for AuthRejected {}

#[derive(Debug)]
pub struct Unreachable;

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "soundcloud could not be reached")
    }
}

impl std::error::Error for Unreachable {}

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
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return Err(anyhow::Error::new(error)
                    .context(Unreachable)
                    .context(format!("cannot reach soundcloud for {path}")));
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(anyhow::Error::new(AuthRejected)
                .context(format!("soundcloud refused {path} with {status}")));
        }
        if !status.is_success() {
            anyhow::bail!("soundcloud refused {path} with {status}");
        }
        response
            .json()
            .await
            .with_context(|| format!("cannot read the soundcloud response for {path}"))
    }
}
