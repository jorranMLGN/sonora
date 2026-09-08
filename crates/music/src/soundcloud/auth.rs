use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, bail};

const SCHEMES: [&str; 2] = ["oauth ", "bearer "];
const MIN_ID: usize = 20;
const BUNDLE_HOST: &str = "https://a-v2.sndcdn.com/assets/";
const DISCOVER: &str = "https://soundcloud.com/discover";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// Normalises whatever the user pasted into a bare bearer token.
///
/// The dialog tells them to copy the `Authorization` value, which reads
/// `OAuth 2-…`, so the scheme has to come off here rather than by hand. A
/// pasted multi-line request blob is tolerated the same way
/// `youtube::auth::header` tolerates one.
pub fn token(input: &str) -> Result<String> {
    let raw = input
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.get(..14)
                .filter(|head| head.eq_ignore_ascii_case("authorization:"))
                .map(|_| line[14..].trim())
        })
        .unwrap_or_else(|| input.trim());
    let token = SCHEMES
        .iter()
        .find_map(|scheme| {
            raw.get(..scheme.len())
                .filter(|head| head.eq_ignore_ascii_case(scheme))
                .map(|_| raw[scheme.len()..].trim())
        })
        .unwrap_or(raw);

    if token.is_empty() {
        bail!("the token is empty");
    }
    if token.contains(char::is_whitespace) {
        bail!(
            "that does not look like a token; copy the whole value of the Authorization request header on the me request, not the response or the request URL"
        );
    }

    Ok(token.to_owned())
}

/// Pulls the player's `client_id` out of a web bundle.
///
/// The bundle is minified and the identifier is written as a JSON key or an
/// object property, so both `"client_id":"…"` and `client_id:"…"` occur.
pub fn extract_client_id(bundle: &str) -> Option<String> {
    let mut rest = bundle;
    while let Some(at) = rest.find("client_id") {
        rest = &rest[at + "client_id".len()..];
        let value = rest.trim_start_matches(['"', ':', '=', ' ']);
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

/// The soundcloud web player's `client_id`, shared across every `Http`
/// cloned from the same `SoundCloudProvider`.
///
/// SoundCloud rotates this id from under a running session, so any request
/// can come back with `http::AuthRejected`. When that happens the caller
/// asks this type to [`refresh`](Self::refresh). Refreshing writes the new
/// id back to `cache_path` — an in-memory-only refresh would cost a fresh
/// harvest on every app start and give the cache file no purpose — and the
/// lock guarding the refresh doubles as a single-flight guard: if eight
/// callers (`users::images`'s `JoinSet`, say) hit a 401 together, the first
/// one to reach `refresh` harvests once, and the other seven find the id
/// already moved past the stale value they saw and return that instead of
/// harvesting a second time.
#[derive(Clone)]
pub struct ClientId {
    value: Arc<Mutex<String>>,
    cache_path: PathBuf,
    refreshing: Arc<tokio::sync::Mutex<()>>,
}

impl ClientId {
    pub fn new(value: String, cache_path: PathBuf) -> Self {
        Self {
            value: Arc::new(Mutex::new(value)),
            cache_path,
            refreshing: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// The id as it stands right now.
    pub fn get(&self) -> String {
        self.value.lock().unwrap().clone()
    }

    /// Re-harvests the client id and persists it to `cache_path`.
    ///
    /// `stale` is the id the caller saw rejected. If another caller already
    /// refreshed past it while this one was waiting for the lock, that
    /// fresher id is returned directly rather than harvesting a second time.
    pub async fn refresh(&self, stale: &str) -> Result<String> {
        let _guard = self.refreshing.lock().await;
        {
            let current = self.value.lock().unwrap();
            if current.as_str() != stale {
                return Ok(current.clone());
            }
        }
        let fresh = harvest_client_id()
            .await
            .context("cannot harvest the soundcloud client id")?;
        self.store(&fresh)?;
        *self.value.lock().unwrap() = fresh.clone();
        Ok(fresh)
    }

    fn store(&self, id: &str) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent).context("cannot create soundcloud cache dir")?;
        }
        std::fs::write(&self.cache_path, id).context("cannot store the soundcloud client id")
    }
}

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
