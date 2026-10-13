//! YouTube's proof-of-origin tokens, minted by a hidden browser page in Swift.
//!
//! The YouTube client records the binding it needs a token for in `music::potoken`; Swift polls
//! `po_token_wanted`, loads the page this hands it with the desktop's own `potoken.js` injected,
//! reads the answer out of the cookie the script leaves, and files it with `po_token`. The
//! script is the desktop's file, compiled in from the repo, so the two never drift apart.

/// Where the minting page is loaded from, as on the desktop.
const PAGE: &str = "https://www.youtube.com/";
const DOMAIN: &str = "youtube.com";
/// The cookie the page leaves its answer in.
const COOKIE: &str = "SONORA_POT";
/// What the page prefixes a failure with. A token is base64url and never starts with one.
const FAILED: char = '!';
const SCRIPT: &str = include_str!("../../state/src/potoken.js");

/// One token to mint: load `url` with `script` injected at document start into every page, and
/// wait for the cookie `cookie` to appear on `domain`. Its value goes back through `po_token`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PoTokenAsk {
    pub binding: String,
    pub url: String,
    pub domain: String,
    pub cookie: String,
    pub script: String,
}

/// What the YouTube client is waiting on a token for, if anything. Each ask is handed out once.
#[uniffi::export]
pub fn po_token_wanted() -> Option<PoTokenAsk> {
    let binding = music::potoken::wanted()?;
    log::debug!("potoken: minting for {binding}");
    Some(PoTokenAsk {
        url: format!("{PAGE}?binding={}", music::escape::component(&binding)),
        binding,
        domain: DOMAIN.to_owned(),
        cookie: COOKIE.to_owned(),
        script: SCRIPT.to_owned(),
    })
}

/// Files what minting for `binding` came to: the cookie's value, or nothing when the page gave
/// up or never answered. A value the script marked as a failure counts as nothing.
#[uniffi::export]
pub fn po_token(binding: String, token: Option<String>) {
    match token.filter(|token| !token.is_empty()) {
        Some(token) if token.starts_with(FAILED) => {
            log::warn!("potoken: the page could not mint a token: {}", &token[1..]);
            music::potoken::failed(&binding);
        }
        Some(token) => {
            log::debug!("potoken: minted a token for {binding}");
            music::potoken::give(&binding, token);
        }
        None => {
            log::warn!("potoken: no token for {binding}");
            music::potoken::failed(&binding);
        }
    }
}
