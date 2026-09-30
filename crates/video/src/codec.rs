//! Where Cisco's OpenH264 binary lives. Sonora never ships it: Cisco's H.264 patent licence
//! covers only the binary a user fetches from Cisco, so `install` downloads it with consent,
//! and `api` loads it through `from_blob_path`, which refuses any file whose sha256 is not one
//! `openh264` knows.

use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context as _, Result};
use openh264::OpenH264API;

/// The release every hash in `openh264-sys2`'s `blobs/hashes.txt` belongs to.
pub const VERSION: &str = "2.6.0";
const MIRROR: &str = "https://ciscobinary.openh264.org";
/// Overrides the store, for a package that carries a module of its own.
const OVERRIDE: &str = "SONORA_OPENH264";

static FOUND: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Cisco's file name for this platform, or `None` where Cisco publishes no binary.
fn blob() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("libopenh264-2.6.0-linux64.8.so"),
        ("linux", "aarch64") => Some("libopenh264-2.6.0-linux-arm64.8.so"),
        ("macos", "aarch64") => Some("libopenh264-2.6.0-mac-arm64.dylib"),
        ("macos", "x86_64") => Some("libopenh264-2.6.0-mac-x64.dylib"),
        ("windows", "x86_64") => Some("openh264-2.6.0-win64.dll"),
        ("windows", "aarch64") => Some("openh264-2.6.0-win-arm64.dll"),
        _ => None,
    }
}

/// Whether Cisco publishes a binary for this platform at all.
pub fn supported() -> bool {
    blob().is_some()
}

/// `$XDG_CACHE_HOME/sonora/openh264/<version>/`.
fn store() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("sonora")
        .join("openh264")
        .join(VERSION)
}

/// The binary to load: `SONORA_OPENH264` first, then Sonora's own store. Only a successful
/// search is remembered, so a binary installed mid-run is found without a restart.
pub fn find() -> Option<PathBuf> {
    let mut found = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(path) = found.as_ref() {
        return Some(path.clone());
    }
    let path = std::env::var_os(OVERRIDE)
        .map(PathBuf::from)
        .or_else(|| blob().map(|name| store().join(name)))
        .filter(|path| path.is_file())?;
    *found = Some(path.clone());
    Some(path)
}

/// Loads the binary `find` answers. Fails when there is none or its hash is not a known one.
pub fn api() -> Result<OpenH264API> {
    let path = find().context("no openh264 binary is installed")?;
    OpenH264API::from_blob_path(&path).with_context(|| format!("cannot load {}", path.display()))
}

/// Downloads Cisco's binary for this platform, unpacks it, proves it loads, and only then moves
/// it into the store. Blocking file work runs inside; call it on `Io`.
pub async fn install() -> Result<PathBuf> {
    let name = blob().context("cisco publishes no openh264 binary for this platform")?;
    let url = format!("{MIRROR}/{name}.bz2");
    let packed = crate::download(&url)
        .await
        .context("cannot download openh264 from cisco")?;
    let mut unpacked = Vec::new();
    bzip2::read::BzDecoder::new(packed.as_slice())
        .read_to_end(&mut unpacked)
        .context("cannot unpack openh264")?;

    let dir = store();
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let part = dir.join(format!("{name}.part"));
    std::fs::write(&part, &unpacked).context("cannot write openh264")?;
    if let Err(error) = OpenH264API::from_blob_path(&part) {
        let _ = std::fs::remove_file(&part);
        anyhow::bail!("the downloaded openh264 is not one sonora can trust: {error}");
    }
    let path = dir.join(name);
    std::fs::rename(&part, &path).context("cannot install openh264")?;
    *FOUND.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(path.clone());
    Ok(path)
}

/// Removes the store and forgets the cached path. A decoder already open stays open until the
/// process ends.
pub fn uninstall() -> Result<()> {
    let root = store();
    if root.exists() {
        std::fs::remove_dir_all(&root)
            .with_context(|| format!("cannot remove {}", root.display()))?;
    }
    *FOUND.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
    Ok(())
}
