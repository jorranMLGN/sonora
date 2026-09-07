use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

const BASE: &str = "https://api-v2.soundcloud.com";

/// How many share permalinks `Http` keeps before evicting the oldest one.
///
/// `share_url` is synchronous and cannot fetch, so a permalink can only be
/// answered if it was seen already; this caps that memory so a long-running
/// session scrolling a large library does not grow it forever.
const PERMALINK_CAPACITY: usize = 500;

#[derive(Default)]
struct Permalinks {
    order: VecDeque<String>,
    by_id: HashMap<String, String>,
}

impl Permalinks {
    fn remember(&mut self, id: String, url: String) {
        if self.by_id.insert(id.clone(), url).is_none() {
            self.order.push_back(id);
            if self.order.len() > PERMALINK_CAPACITY
                && let Some(oldest) = self.order.pop_front()
            {
                self.by_id.remove(&oldest);
            }
        }
    }
}

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

#[derive(Clone)]
pub struct Http {
    agent: reqwest::Client,
    client_id: String,
    token: Option<String>,
    permalinks: Arc<Mutex<Permalinks>>,
}

impl Http {
    pub fn anonymous(client_id: String) -> Self {
        Self {
            agent: reqwest::Client::new(),
            client_id,
            token: None,
            permalinks: Arc::new(Mutex::new(Permalinks::default())),
        }
    }

    pub fn with_token(client_id: String, token: String) -> Self {
        Self {
            agent: reqwest::Client::new(),
            client_id,
            token: Some(token),
            permalinks: Arc::new(Mutex::new(Permalinks::default())),
        }
    }

    pub fn authenticated(&self) -> bool {
        self.token.is_some()
    }

    /// Records a permalink `share_url` can later answer with, evicting the
    /// oldest entry once the cache is full. A missing `url` is a no-op.
    pub fn remember_permalink(&self, id: u64, url: Option<String>) {
        let Some(url) = url else { return };
        if let Ok(mut cache) = self.permalinks.lock() {
            cache.remember(id.to_string(), url);
        }
    }

    /// The permalink `share_url` answers with, if this id was ever seen.
    pub fn permalink(&self, id: &str) -> Option<String> {
        self.permalinks.lock().ok()?.by_id.get(id).cloned()
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

    pub async fn put_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let mut request = self
            .agent
            .put(format!("{BASE}{path}"))
            .query(&[("client_id", self.client_id.as_str())])
            .json(body);
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

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let mut request = self
            .agent
            .post(format!("{BASE}{path}"))
            .query(&[("client_id", self.client_id.as_str())])
            .json(body);
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

    pub async fn put_empty(&self, path: &str) -> Result<()> {
        let mut request = self
            .agent
            .put(format!("{BASE}{path}"))
            .query(&[("client_id", self.client_id.as_str())]);
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
        Ok(())
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let mut request = self
            .agent
            .delete(format!("{BASE}{path}"))
            .query(&[("client_id", self.client_id.as_str())]);
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
        Ok(())
    }
}
