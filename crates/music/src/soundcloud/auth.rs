use anyhow::{Context as _, Result};

const MIN_ID: usize = 20;
const BUNDLE_HOST: &str = "https://a-v2.sndcdn.com/assets/";
const DISCOVER: &str = "https://soundcloud.com/discover";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

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
