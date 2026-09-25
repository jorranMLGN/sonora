use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

const WANTED: &[&str] = &[
    "SID",
    "__Secure-1PSID",
    "__Secure-3PSID",
    "__Secure-1PSIDTS",
    "__Secure-3PSIDTS",
    "__Secure-1PSIDCC",
    "__Secure-3PSIDCC",
    "HSID",
    "SSID",
    "APISID",
    "SAPISID",
    "__Secure-1PAPISID",
    "__Secure-3PAPISID",
    "LOGIN_INFO",
    "SIDCC",
    "PREF",
    "VISITOR_INFO1_LIVE",
    "VISITOR_PRIVACY_METADATA",
    "__Secure-YNID",
    "__Secure-ROLLOUT_TOKEN",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Firefox,
    Chromium,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Browser {
    pub name: &'static str,
    pub family: Family,
    pub root: PathBuf,
}

#[cfg(target_os = "linux")]
const FIREFOX: &[(&str, &str)] = &[
    ("Firefox", ".mozilla/firefox"),
    ("Firefox", ".var/app/org.mozilla.firefox/.mozilla/firefox"),
    ("Firefox", "snap/firefox/common/.mozilla/firefox"),
    ("Firefox Developer Edition", ".mozilla/firefox-dev"),
    ("Zen", ".zen"),
    ("Zen", ".config/zen"),
    ("Zen", ".var/app/app.zen_browser.zen/.zen"),
    ("LibreWolf", ".librewolf"),
    (
        "LibreWolf",
        ".var/app/io.gitlab.librewolf-community/.librewolf",
    ),
    ("Floorp", ".floorp"),
    ("Floorp", ".var/app/one.ablaze.floorp/.floorp"),
    ("Waterfox", ".waterfox"),
    ("Mullvad Browser", ".mullvad-browser"),
    ("Tor Browser", ".tor-browser"),
    ("Pale Moon", ".moonchild productions/pale moon"),
    ("Basilisk", ".moonchild productions/basilisk"),
    ("SeaMonkey", ".mozilla/seamonkey"),
    ("Cachy Browser", ".cachy-browser"),
];

#[cfg(target_os = "linux")]
const CHROMIUM: &[(&str, &str)] = &[
    ("Chrome", "google-chrome"),
    ("Chrome", ".var/app/com.google.Chrome/config/google-chrome"),
    ("Chrome Beta", "google-chrome-beta"),
    ("Chrome Dev", "google-chrome-unstable"),
    ("Chromium", "chromium"),
    ("Chromium", ".var/app/org.chromium.Chromium/config/chromium"),
    ("Chromium", "snap/chromium/common/chromium"),
    ("Brave", "BraveSoftware/Brave-Browser"),
    (
        "Brave",
        ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
    ),
    ("Edge", "microsoft-edge"),
    ("Edge", ".var/app/com.microsoft.Edge/config/microsoft-edge"),
    ("Vivaldi", "vivaldi"),
    ("Vivaldi", ".var/app/com.vivaldi.Vivaldi/config/vivaldi"),
    ("Opera", "opera"),
    ("Opera", ".var/app/com.opera.Opera/config/opera"),
    ("Opera GX", "opera-gx"),
    ("Yandex", "yandex-browser"),
    ("Arc", "arc"),
    ("Thorium", "Thorium"),
    ("Ungoogled Chromium", "chromium-browser"),
    ("Helium", "net.imput.helium"),
    (
        "Helium",
        ".var/app/net.imput.helium/config/net.imput.helium",
    ),
];

#[cfg(target_os = "macos")]
const FIREFOX: &[(&str, &str)] = &[
    ("Firefox", "Library/Application Support/Firefox/Profiles"),
    ("Zen", "Library/Application Support/zen/Profiles"),
    (
        "LibreWolf",
        "Library/Application Support/librewolf/Profiles",
    ),
    ("Floorp", "Library/Application Support/Floorp/Profiles"),
    ("Waterfox", "Library/Application Support/Waterfox/Profiles"),
    (
        "Mullvad Browser",
        "Library/Application Support/MullvadBrowser/Profiles",
    ),
    (
        "Tor Browser",
        "Library/Application Support/TorBrowser-Data/Browser",
    ),
    (
        "SeaMonkey",
        "Library/Application Support/SeaMonkey/Profiles",
    ),
];

#[cfg(target_os = "macos")]
const CHROMIUM: &[(&str, &str)] = &[
    ("Chrome", "Google/Chrome"),
    ("Chrome Beta", "Google/Chrome Beta"),
    ("Chrome Canary", "Google/Chrome Canary"),
    ("Chromium", "Chromium"),
    ("Brave", "BraveSoftware/Brave-Browser"),
    ("Edge", "Microsoft Edge"),
    ("Vivaldi", "Vivaldi"),
    ("Opera", "com.operasoftware.Opera"),
    ("Opera GX", "com.operasoftware.OperaGX"),
    ("Yandex", "Yandex/YandexBrowser"),
    ("Arc", "Arc/User Data"),
    ("Helium", "net.imput.helium"),
];

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
const CHROMIUM: &[(&str, &str)] = &[
    ("Chrome", "Google/Chrome/User Data"),
    ("Chrome Beta", "Google/Chrome Beta/User Data"),
    ("Chrome Canary", "Google/Chrome SxS/User Data"),
    ("Chromium", "Chromium/User Data"),
    ("Brave", "BraveSoftware/Brave-Browser/User Data"),
    ("Edge", "Microsoft/Edge/User Data"),
    ("Vivaldi", "Vivaldi/User Data"),
    ("Opera", "Opera Software/Opera Stable"),
    ("Opera GX", "Opera Software/Opera GX Stable"),
    ("Yandex", "Yandex/YandexBrowser/User Data"),
    (
        "Arc",
        "Packages/TheBrowserCompany.Arc/LocalCache/Local/Arc/User Data",
    ),
    ("Thorium", "Thorium/User Data"),
    ("Helium", "net.imput.helium/User Data"),
];

pub fn detect() -> Vec<Browser> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let mut found: Vec<Browser> = Vec::new();
    let push = |name: &'static str, family: Family, root: PathBuf, found: &mut Vec<Browser>| {
        if found.iter().any(|browser| browser.name == name) {
            return;
        }
        found.push(Browser { name, family, root });
    };
    let firefox = [firefox_root(&home), home.clone()];
    let chromium = [chromium_root(&home), home.clone()];
    for (name, rel) in FIREFOX {
        for base in firefox.iter().map(|root| root.join(rel)) {
            if firefox_profile(&base).is_some() {
                push(name, Family::Firefox, base, &mut found);
                break;
            }
        }
    }
    for (name, rel) in CHROMIUM {
        for base in chromium.iter().map(|root| root.join(rel)) {
            if chromium_store(&base).is_some() {
                push(name, Family::Chromium, base, &mut found);
                break;
            }
        }
    }
    found.sort_by_key(|browser| browser.name);
    found
}

#[cfg(target_os = "linux")]
fn firefox_root(home: &Path) -> PathBuf {
    home.to_path_buf()
}

#[cfg(not(target_os = "linux"))]
fn firefox_root(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return roaming_dir(home);
    }
    #[cfg(not(target_os = "windows"))]
    home.to_path_buf()
}

#[cfg(target_os = "linux")]
fn chromium_root(home: &Path) -> PathBuf {
    config_dir(home)
}

#[cfg(target_os = "macos")]
fn chromium_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support")
}

#[cfg(target_os = "windows")]
fn chromium_root(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join("AppData/Local"))
}

#[cfg(target_os = "windows")]
fn roaming_dir(home: &Path) -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join("AppData/Roaming"))
}

fn chromium_store(root: &Path) -> Option<PathBuf> {
    [
        "Default/Network/Cookies",
        "Default/Cookies",
        "Network/Cookies",
        "Cookies",
    ]
    .iter()
    .map(|rel| root.join(rel))
    .find(|path| path.exists())
}

pub fn cookies(browser: &Browser) -> Result<String> {
    match browser.family {
        Family::Firefox => firefox_cookies(&browser.root),
        Family::Chromium => bail!(
            "{} keeps its cookies behind the system keyring",
            browser.name
        ),
    }
}

fn firefox_cookies(root: &Path) -> Result<String> {
    let profile = firefox_profile(root).context("no firefox profile with cookies")?;
    let db = profile.join("cookies.sqlite");
    let temp = copy_locked(&db)?;
    let result = read_firefox(&temp);
    let _ = std::fs::remove_file(&temp);
    assemble(result?)
}

fn read_firefox(db: &Path) -> Result<Vec<(String, String)>> {
    let connection =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .context("cannot open cookies.sqlite")?;
    let mut statement = connection
        .prepare("SELECT name, value FROM moz_cookies WHERE host LIKE '%youtube.com'")
        .context("cannot query moz_cookies")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("cannot read cookie rows")?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn firefox_profile(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("cookies.sqlite").exists())
        .max_by_key(|path| {
            std::fs::metadata(path.join("cookies.sqlite"))
                .and_then(|meta| meta.modified())
                .ok()
        })
}

fn assemble(pairs: Vec<(String, String)>) -> Result<String> {
    let mut cookie = String::new();
    for name in WANTED {
        if let Some((_, value)) = pairs.iter().find(|(candidate, _)| candidate == name) {
            if !cookie.is_empty() {
                cookie.push_str("; ");
            }
            cookie.push_str(name);
            cookie.push('=');
            cookie.push_str(value);
        }
    }
    if !cookie.contains("SAPISID=") && !cookie.contains("__Secure-3PAPISID=") {
        bail!("no youtube login cookies found; sign in to the browser first");
    }
    Ok(cookie)
}

fn copy_locked(db: &Path) -> Result<PathBuf> {
    let stamp = db
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let temp = std::env::temp_dir().join(format!("ytmusic-cookies-{stamp}.sqlite"));
    std::fs::copy(db, &temp).with_context(|| format!("cannot copy {}", db.display()))?;
    Ok(temp)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn config_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
}
