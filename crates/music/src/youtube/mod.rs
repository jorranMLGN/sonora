mod accounts;
mod auth;
mod browser;
mod client;
mod genres;
mod lyrics;
mod playback;
mod radio;
mod segments;
mod site;
mod subscriptions;
mod wire;

use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ytmusic::YtMusic;

use self::browser::{Browser, Family, detect};
use crate::youtube::playback::Factory;
use crate::youtube::site::MUSIC;

use crate::{
    Capabilities, InputSource, MusicProvider, PromptSink, ProviderSession, Shape, SignIn,
    SignInPrompt, UserProfile, WebSignIn, credentials,
};
pub use client::YouTubeClient;
pub use lyrics::YouTubeLyrics;
pub(crate) use site::Site;
pub(crate) use wire::profile as user_profile;

const GUEST_ID: &str = "youtube-guest";
const COOKIE_DOMAIN: &str = "youtube.com";
/// How often a signed-in session asks Google for fresh cookies, the cadence of an open YouTube tab.
const ROTATION: Duration = Duration::from_secs(10 * 60);
/// How often the rotation loop looks at the wall clock. A machine that slept past a rotation
/// catches up within this, since a monotonic timer stops while the system is suspended.
const ROTATION_CHECK: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Saved {
    Cookies {
        cookies: String,
        authuser: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page_id: Option<String>,
    },
    Guest,
}

/// Asks the app for the proof-of-origin token the client needs. Every answer comes out of
/// `music::potoken`, so the client never waits on the browser window that fills it.
struct Minter;

#[async_trait]
impl ytmusic::Minter for Minter {
    async fn mint(&self, binding: &str) -> Option<String> {
        crate::potoken::token(binding)
    }
}

pub struct YouTubeProvider {
    site: &'static Site,
    credentials: PathBuf,
    /// The cookie store ytmusic writes back to as Google rotates the session. The pasted
    /// header in the credential file seeds it when it is missing.
    cookies: PathBuf,
    resolved: PathBuf,
    player: PathBuf,
}

impl YouTubeProvider {
    pub fn new() -> Self {
        Self::for_site(&MUSIC, credentials::dir(MUSIC.slug))
    }

    /// A provider that keeps its credentials, cookies and player caches in `cache`, for a
    /// platform whose cache directory is handed in rather than found, such as an iOS app's
    /// sandbox.
    pub fn at(cache: PathBuf) -> Self {
        Self::for_site(&MUSIC, cache)
    }

    /// A provider for `site` that keeps its credentials, cookies and player caches in `cache`.
    pub(crate) fn for_site(site: &'static Site, cache: PathBuf) -> Self {
        Self {
            site,
            credentials: cache.join(credentials::FILE),
            cookies: cache.join("cookies.json"),
            resolved: cache.join("resolved.json"),
            player: cache.join("player.json"),
        }
    }

    fn save(&self, saved: &Saved) -> Result<()> {
        save(&self.credentials, saved)
    }

    fn saved(&self) -> Option<Saved> {
        let body = std::fs::read(&self.credentials).ok()?;
        match serde_json::from_slice(&body) {
            Ok(saved) => Some(saved),
            Err(error) => {
                log::warn!("youtube: cannot read the stored credentials: {error}");
                None
            }
        }
    }

    fn cookie_client(&self, cookies: &str, authuser: usize, page_id: Option<&str>) -> Arc<YtMusic> {
        let api = YtMusic::with_cookies(cookies).as_user(authuser);
        let api = match page_id {
            Some(page) => api.as_page(page),
            None => api,
        };
        Arc::new(
            api.persist_cookies(self.cookies.clone())
                .cache_resolutions(self.resolved.clone())
                .cache_player(self.player.clone())
                .mint_po_tokens(Arc::new(Minter)),
        )
    }

    fn guest_client(&self) -> Arc<YtMusic> {
        Arc::new(
            YtMusic::anonymous()
                .cache_player(self.player.clone())
                .mint_po_tokens(Arc::new(Minter)),
        )
    }

    /// A signed-in session over `api`. It starts the loop that keeps the cookies rotated, so it
    /// must be called on the tokio runtime.
    fn authenticated_session(&self, api: Arc<YtMusic>, profile: UserProfile) -> ProviderSession {
        let since = self.rotated_at().unwrap_or_else(SystemTime::now);
        keep_fresh(Arc::downgrade(&api), since);
        let client = (self.site.client)(api.clone(), Some(profile.display_name.clone()));
        ProviderSession {
            profile,
            api: client,
            playback: Arc::new(Factory::new(api)),
            shape: Shape::Saved,
            authenticated: true,
            capabilities: Capabilities {
                playcounts: false,
                ..Capabilities::ALL
            },
            expired: false,
        }
    }

    fn guest_session(&self, api: Arc<YtMusic>) -> ProviderSession {
        ProviderSession {
            profile: UserProfile {
                id: GUEST_ID.to_string(),
                display_name: self.site.guest_name.to_string(),
                avatar: None,
            },
            api: (self.site.client)(api.clone(), None),
            playback: Arc::new(Factory::new(api)),
            shape: Shape::Saved,
            authenticated: false,
            capabilities: Capabilities {
                playcounts: false,
                ..Capabilities::ALL
            },
            expired: false,
        }
    }

    async fn connect(
        &self,
        cookies: &str,
        prompt: &PromptSink,
        input: &mut InputSource,
    ) -> Result<ProviderSession> {
        let cookies = auth::header(cookies)?;

        let found = accounts::list(&cookies).await;
        let account = match found.len() {
            0 => anyhow::bail!("cookies were not accepted; sign in to the browser first"),
            1 => &found[0],
            _ => pick(&found, prompt, input).await?,
        };

        let page_id = account.identity.page_id.clone();
        let profile = wire::profile(account.identity.profile.clone());
        credentials::remove(&self.cookies);
        let api = self.cookie_client(&cookies, account.index, page_id.as_deref());
        self.store_cookies(&cookies, account.index, page_id.clone())?;
        log::debug!(
            "youtube: cookie sign-in succeeded for authuser {} page {page_id:?}",
            account.index
        );
        Ok(self.authenticated_session(api, profile))
    }

    fn store_cookies(&self, cookies: &str, authuser: usize, page_id: Option<String>) -> Result<()> {
        self.save(&Saved::Cookies {
            cookies: cookies.to_owned(),
            authuser,
            page_id,
        })
        .context("cannot store youtube cookies")
    }

    /// The session the stored cookies still hold, or nothing when YouTube refused them. A
    /// failure that only says the host was unreachable is handed back instead, so a run without
    /// a network never reads as an account that went bad.
    async fn restore_cookies(
        &self,
        cookies: &str,
        authuser: usize,
        page_id: Option<&str>,
    ) -> Result<Option<ProviderSession>> {
        let api = self.cookie_client(cookies, authuser, page_id);
        let fresh = self
            .rotated_at()
            .is_some_and(|at| at.elapsed().is_ok_and(|elapsed| elapsed < ROTATION));
        if !fresh && let Err(error) = api.rotate_cookies().await {
            log::warn!("youtube: cannot rotate the stored cookies: {error:#}");
        }
        match api.profile().await {
            Ok(profile) => {
                log::debug!(
                    "youtube: restored the session for authuser {authuser} page {page_id:?}"
                );
                Ok(Some(
                    self.authenticated_session(api, wire::profile(profile)),
                ))
            }
            Err(error) if crate::trouble::offline(&format!("{error:#}")) => Err(error),
            Err(error) => {
                log::warn!("youtube: the cached cookies are no longer usable: {error:#}");
                Ok(None)
            }
        }
    }

    /// When the cookie store was last written, which is when Google last rotated the session.
    /// Nothing when there is no store yet, as after a fresh sign-in or on an older install.
    fn rotated_at(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.cookies)
            .and_then(|meta| meta.modified())
            .ok()
    }

    /// Records that the user chose to listen as a guest, so the next launch restores that
    /// instead of asking again. The rotating cookie store goes with it, since a guest client
    /// never reads one.
    fn store_guest(&self) {
        credentials::remove(&self.cookies);
        if let Err(error) = self.save(&Saved::Guest) {
            log::warn!("youtube: cannot remember the guest session: {error:#}");
        }
    }
}

/// Rotates the session cookies every `ROTATION` of wall-clock time, counted from `since`, for as
/// long as the client lives, the way an open YouTube tab does. The loop ends once the session
/// drops the client.
fn keep_fresh(api: Weak<YtMusic>, since: SystemTime) {
    tokio::spawn(async move {
        let mut rotated = since;
        loop {
            tokio::time::sleep(ROTATION_CHECK).await;
            let Some(api) = api.upgrade() else {
                return;
            };
            if rotated.elapsed().is_ok_and(|elapsed| elapsed < ROTATION) {
                continue;
            }
            match api.rotate_cookies().await {
                Ok(()) => log::debug!("youtube: rotated the session cookies"),
                Err(error) => log::warn!("youtube: cannot rotate the session cookies: {error:#}"),
            }
            rotated = SystemTime::now();
        }
    });
}

fn save(file: &std::path::Path, saved: &Saved) -> Result<()> {
    let body = serde_json::to_vec_pretty(saved).context("cannot encode youtube credentials")?;
    credentials::write(file, &body)
}

async fn pick<'a>(
    found: &'a [accounts::Account],
    prompt: &PromptSink,
    input: &mut InputSource,
) -> Result<&'a accounts::Account> {
    prompt(SignInPrompt::Accounts(
        found.iter().map(accounts::Account::choice).collect(),
    ));
    let picked = input.recv().await.context("sign-in was cancelled")?;
    found
        .iter()
        .find(|account| account.id() == picked.trim())
        .context("that account is no longer signed in")
}

/// The firefox-based browsers whose cookie store can be read without a keyring.
fn browsers() -> Vec<Browser> {
    let mut browsers = detect();
    browsers.retain(|browser| browser.family == Family::Firefox);
    #[cfg(target_os = "windows")]
    detect_windows_browsers(&mut browsers);
    browsers.sort_by_key(|browser| browser.name);
    browsers
}

#[cfg(target_os = "windows")]
fn detect_windows_browsers(found: &mut Vec<Browser>) {
    const FIREFOX: &[(&str, &str)] = &[
        ("Firefox", "Mozilla/Firefox/Profiles"),
        ("Zen", "zen/Profiles"),
        ("LibreWolf", "librewolf/Profiles"),
        ("Floorp", "Floorp/Profiles"),
        ("Waterfox", "Waterfox/Profiles"),
        ("Mullvad Browser", "Mullvad/MullvadBrowser/Profiles"),
        ("Pale Moon", "Moonchild Productions/Pale Moon/Profiles"),
        ("SeaMonkey", "Mozilla/SeaMonkey/Profiles"),
    ];
    let home = dirs::home_dir();
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.as_ref().map(|home| home.join("AppData/Roaming")));

    if let Some(root) = roaming {
        for &(name, relative) in FIREFOX {
            let root = root.join(relative);
            if has_firefox_cookies(&root) {
                push_browser(found, name, Family::Firefox, root);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn push_browser(found: &mut Vec<Browser>, name: &'static str, family: Family, root: PathBuf) {
    if !found.iter().any(|browser| browser.name == name) {
        found.push(Browser { name, family, root });
    }
}

#[cfg(target_os = "windows")]
fn has_firefox_cookies(root: &std::path::Path) -> bool {
    std::fs::read_dir(root).is_ok_and(|profiles| {
        profiles
            .flatten()
            .any(|profile| profile.path().join("cookies.sqlite").exists())
    })
}

impl Default for YouTubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for YouTubeProvider {
    fn name(&self) -> &'static str {
        self.site.name
    }

    fn slug(&self) -> &'static str {
        self.site.slug
    }

    fn reach(&self) -> Option<String> {
        Some(self.site.reach.to_owned())
    }

    fn public_art(&self) -> bool {
        true
    }

    fn sign_in_options(&self) -> Vec<SignIn> {
        let mut options = vec![SignIn::Anonymous, SignIn::Secret];
        options.extend(
            browsers()
                .into_iter()
                .map(|browser| SignIn::Browser(browser.name.to_string())),
        );
        options
    }

    fn stored(&self) -> bool {
        self.credentials.exists()
    }

    fn stored_guest(&self) -> bool {
        matches!(self.saved(), Some(Saved::Guest))
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        match self.saved() {
            Some(Saved::Cookies {
                cookies,
                authuser,
                page_id,
            }) => {
                self.restore_cookies(&cookies, authuser, page_id.as_deref())
                    .await
            }
            Some(Saved::Guest) => {
                log::debug!("youtube: restoring guest session");
                Ok(Some(self.guest_session(self.guest_client())))
            }
            None => Ok(None),
        }
    }

    async fn sign_in(
        &self,
        method: SignIn,
        prompt: crate::PromptSink,
        mut input: InputSource,
    ) -> Result<ProviderSession> {
        match method {
            SignIn::Anonymous | SignIn::Default => {
                self.store_guest();
                Ok(self.guest_session(self.guest_client()))
            }
            SignIn::Browser(name) => {
                let browser = browsers()
                    .into_iter()
                    .find(|browser| browser.name == name)
                    .with_context(|| format!("{name} is no longer installed"))?;
                let cookies = auth::cookies(&browser)?;
                self.connect(&cookies, &prompt, &mut input).await
            }
            SignIn::Secret => {
                prompt(SignInPrompt::Secret);
                let cookies = input.recv().await.context("sign-in was cancelled")?;
                self.connect(&cookies, &prompt, &mut input).await
            }
            SignIn::Path(_) => Err(anyhow::anyhow!(
                "youtube does not sign in with a folder path"
            )),
            SignIn::Credentials { .. } => Err(anyhow::anyhow!(
                "youtube does not sign in with a server address"
            )),
        }
    }

    fn sign_out(&self) {
        credentials::remove(&self.credentials);
        credentials::remove(&self.cookies);
    }

    fn web_sign_in(&self) -> Option<WebSignIn> {
        Some(WebSignIn {
            url: self.site.sign_in_url,
            landing: self.site.landing,
            domain: COOKIE_DOMAIN,
            proof: auth::PROOF,
            agent: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MusicApi;

    fn video_client(api: Arc<YtMusic>, owner: Option<String>) -> Arc<dyn MusicApi> {
        let client = YouTubeClient::new(api);
        Arc::new(match owner {
            Some(owner) => client.owned_by(owner),
            None => client,
        })
    }

    static VIDEO: Site = Site {
        name: "YouTube",
        slug: "youtube-video",
        reach: "www.youtube.com",
        sign_in_url: "https://accounts.google.com/ServiceLogin?service=youtube",
        landing: "www.youtube.com",
        guest_name: "YouTube",
        client: video_client,
    };

    #[test]
    fn each_site_keeps_its_own_credentials() {
        let tmp = tempfile::tempdir().unwrap();
        let music = YouTubeProvider::for_site(&MUSIC, tmp.path().join("a"));
        let video = YouTubeProvider::for_site(&VIDEO, tmp.path().join("b"));
        std::fs::create_dir_all(tmp.path().join("a")).unwrap();
        std::fs::create_dir_all(tmp.path().join("b")).unwrap();

        music.store_guest();

        assert!(music.stored());
        assert!(!video.stored());
        assert_eq!(music.slug(), "youtube");
        assert_eq!(video.slug(), "youtube-video");
        assert_eq!(video.name(), "YouTube");
        assert_eq!(video.reach().as_deref(), Some("www.youtube.com"));
    }
}
