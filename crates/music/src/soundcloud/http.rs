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

/// Cloning `Http` shares state, it does not reset it: the clone reuses the
/// same underlying `reqwest::Client` (already internally reference-counted,
/// so this is cheap) and the same permalink cache, deliberately — spawned
/// tasks (`users::images`'s `JoinSet`) need their own owned handle to make
/// requests concurrently, and they must see permalinks the original `Http`
/// already remembered rather than starting a cache of their own. A field
/// added here that should NOT be shared across clones needs its own answer,
/// not a silent `derive(Clone)`.
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

#[cfg(test)]
mod tests {
    use super::{Http, PERMALINK_CAPACITY, Permalinks};

    #[test]
    fn evicts_the_oldest_id_once_the_cap_is_exceeded() {
        let mut cache = Permalinks::default();
        for n in 0..=PERMALINK_CAPACITY {
            cache.remember(n.to_string(), format!("url-{n}"));
        }

        assert_eq!(cache.by_id.len(), PERMALINK_CAPACITY);
        assert!(
            !cache.by_id.contains_key("0"),
            "the first id inserted must be evicted once the cache is full"
        );
        assert!(
            cache.by_id.contains_key(&PERMALINK_CAPACITY.to_string()),
            "the most recently inserted id must still be present"
        );
    }

    #[test]
    fn remembering_a_known_id_again_does_not_grow_the_queue() {
        let mut cache = Permalinks::default();
        cache.remember("1".to_string(), "url-1".to_string());
        cache.remember("1".to_string(), "url-1-again".to_string());

        assert_eq!(
            cache.order.len(),
            1,
            "re-seeing an id already cached must not push a second entry, \
             or a hot id would silently evict unrelated ones"
        );
        assert_eq!(
            cache.by_id.get("1").map(String::as_str),
            Some("url-1-again")
        );
    }

    #[test]
    fn an_id_never_inserted_is_not_found() {
        let cache = Permalinks::default();
        assert_eq!(cache.by_id.get("missing"), None);
    }

    #[test]
    fn remembering_a_missing_url_is_a_no_op() {
        let http = Http::anonymous("client".to_string());
        http.remember_permalink(1, None);
        assert_eq!(http.permalink("1"), None);
    }
}
