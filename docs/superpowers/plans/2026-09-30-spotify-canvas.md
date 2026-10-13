# Spotify Canvas — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Play a song's Spotify Canvas (a short looping video) in place of its cover in the fullscreen view and at the top of the right sidebar's Queue tab.

**Architecture:** A new leaf crate `video` owns the decoder store (Cisco's `libopenh264`, downloaded with consent and loaded through `openh264`'s `libloading` backend), the MP4 → Annex-B → YUV → BGRA pipeline and a looping frame player on its own thread. `music` gains `MusicApi::canvas(track_id) -> Option<url>`, answered only by Spotify through extended metadata (`ExtensionKind::CANVAZ`), and re-exports the crate as `music::video`. `state` gains two entities modelled on existing ones: `Codec` (like `Drm`: find, ask, install, remove) and `Canvas` (like `Cover`: follows `Playback`, loads per track). `views` gains one `CanvasArt` view that pumps frames with `window.on_next_frame` the way `VisualizerDrive` does, a consent modal like `WidevinePrompt`, two Settings rows, and the two placements.

**Tech Stack:** Rust 2024, GPUI, tokio via `state::Io`, `openh264` 0.9.8 (`libloading`), `mp4` 0.14, `bzip2` 0.6, `sha2`, `reqwest`, librespot `spclient` extended metadata, Fluent.

**Spec:** `docs/superpowers/specs/2026-09-30-spotify-canvas-design.md`

## Global Constraints

Copied from `CLAUDE.md`, the user's standing rules and the spec. Every task's requirements implicitly include this section.

- **Never run `git commit` or `git push`.** A task's last step prints the commit it proposes and stops; the user approves each one separately. Subagents get the same restriction.
- **No new tests.** `CLAUDE.md`: "Do not add tests unless asked." Each task ends with manual checks instead; offering a test is fine, writing one is not.
- **Never call the Spotify Web API.** Canvas data comes from `session.spclient()` through `collection::extended`. No `api.spotify.com`, no hand-rolled protobuf: `librespot_protocol::canvaz` has the generated type.
- **The decoder is never shipped.** `openh264` with `default-features = false, features = ["libloading"]` only. No `source` feature anywhere, no binary in a release, package or Flatpak, no `about.toml` entry for it. The binary comes from `https://ciscobinary.openh264.org/` and is loaded only through `OpenH264API::from_blob_path`, which checks its sha256.
- **Dependency direction** gains exactly one edge, `music → video`, mirroring `music → widevine`. `video` depends on no workspace crate. `state` and `views` reach it only as `music::video`. `music` still never depends on `gpui`.
- **GPUI images are BGRA.** `RenderImage` frames hold B, G, R, A in that order (`ui::artwork::bgra`, `crates/ui/src/artwork.rs:199-219`).
- **Every user-facing string comes from Fluent.** Add to `assets/i18n/en-US/main.ftl`; `ru`, `uk`, `pl` may lag. Never `t!` in a constructor. Developer text (`log::warn!`, `.context("cannot …")`) stays English, lowercase, prefixed by subsystem (`"canvas: …"`, `"codec: …"`).
- **Never hardcode a colour, radius or size** in GPUI code: `cx.theme()` and `theme.metrics`. A non-scaling local detail is a `const` at module top.
- **Network work runs on `Io`** (`io.spawn` / `io.spawn_blocking`), never on GPUI's executor. Every load's `Task` is stored in a field, never `.detach()`ed.
- **Doc comments** on every type and every function whose behaviour is not obvious from its name, one or two plain sentences.
- **Dependencies** go in the root `[workspace.dependencies]`, then `dep.workspace = true`.
- **After every task** all of these are clean:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets` (no new warning)
  - `cargo test --workspace`
- **Commit messages**: Conventional Commits, `type(scope): description`, imperative, lowercase, no trailing period, no body, never a `Co-Authored-By` trailer.

## Review Focus

The inputs the spec implies but no test exercises. Each line has a manual check in the task that owns the code.

1. **Skipping tracks quickly** while a canvas is still downloading must never show the old song's canvas on the new song (Task 5, check 5.b).
2. **A song without a canvas, or any non-Spotify song,** shows the cover exactly as today, with no request storm and no decoder prompt (Task 5, check 5.c).
3. **The Cisco download failing** (offline, 404, hash mismatch) leaves the cover in place, logs one warning, and Settings can retry (Task 4, check 4.c).
4. **Nothing on screen draws the canvas** (fullscreen closed, Queue tab not shown, window hidden, playback paused): the decoder thread must idle near 0% CPU (Task 6, check 6.c).
5. **Odd frame sizes** (the verified 718 × 1280 file) must convert without a panic or a skewed image (Task 2, check 2.b).

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `Cargo.toml` | Workspace deps `openh264`, `mp4`, `bzip2`, `video`. | 1 |
| `crates/video/Cargo.toml` | **New.** Leaf crate manifest. | 1 |
| `crates/video/src/lib.rs` | **New.** Crate doc, re-exports, `download(url)`. | 1 |
| `crates/video/src/codec.rs` | **New.** Where the Cisco binary lives, `find`, `install`, `uninstall`, `api`. | 1 |
| `crates/video/src/annexb.rs` | **New.** MP4 samples → Annex-B with SPS/PPS (adapted from `openh264`'s own example). | 2 |
| `crates/video/src/convert.rs` | **New.** BT.709 limited-range YUV 4:2:0 → BGRA, downscaled in the same pass. | 2 |
| `crates/video/src/player.rs` | **New.** `Player`: parse once, decode in a loop on its own thread, pace to the clip's fps, idle when unwatched. | 2 |
| `crates/music/Cargo.toml`, `crates/music/src/video.rs` | `video` dependency; the `music::video` door. | 3 |
| `crates/music/src/lib.rs` | `MusicApi::canvas` with an `Ok(None)` default; `pub mod video`. | 3 |
| `crates/music/src/spotify/canvas.rs`, `client.rs`, `mod.rs` | Spotify's answer through `ExtensionKind::CANVAZ`. | 3 |
| `crates/music/src/tagged/mod.rs` | Forwards `canvas`. | 3 |
| `crates/state/src/codec.rs` | **New.** `Codec` entity: `CodecState`, look / want / install / dismiss / uninstall. | 4 |
| `crates/state/src/settings.rs` | `appearance.canvas: bool` (default `true`), `canvas()`, `set_canvas()`. | 4 |
| `crates/state/src/canvas.rs` | **New.** `Canvas` entity: follows `Playback`, resolves the url, downloads, owns the `Player`. | 5 |
| `crates/state/src/lib.rs` | Registers both entities on `Sonora`; re-exports. | 4, 5 |
| `crates/views/src/shared/canvas.rs` | **New.** `CanvasArt`: the frame pump and the image. | 6 |
| `crates/views/src/shared/codec.rs` | **New.** `CodecPrompt`: the consent modal. | 4 |
| `crates/views/src/shells/workspace.rs` | Mounts `CodecPrompt` beside `WidevinePrompt`. | 4 |
| `crates/views/src/screens/settings.rs` | `Slot::Canvas` (switch) and `Slot::CanvasCodec` (download / remove). | 4 |
| `crates/views/src/shells/fullscreen.rs` | Canvas over the cover in `artwork()`. | 7 |
| `crates/views/src/chrome/aside.rs` | Canvas block at the top of the Queue tab while one plays. | 7 |
| `assets/i18n/en-US/main.ftl` | Every new string. | 4, 7 |
| `CLAUDE.md`, `CHANGELOG.md`, `THIRD-PARTY.md` | Crate layout and dependency edge; the user-facing entry; regenerated notices. | 8 |

---

### Task 1: The `video` crate and the decoder store

**Files:**
- Modify: `Cargo.toml` (root `[workspace.dependencies]`)
- Create: `crates/video/Cargo.toml`
- Create: `crates/video/src/lib.rs`
- Create: `crates/video/src/codec.rs`

**Interfaces:**
- Consumes: nothing from the workspace.
- Produces:
  - `video::codec::VERSION: &str`
  - `video::codec::supported() -> bool`
  - `video::codec::find() -> Option<PathBuf>`
  - `video::codec::api() -> anyhow::Result<openh264::OpenH264API>`
  - `video::codec::install() -> anyhow::Result<PathBuf>` (async)
  - `video::codec::uninstall() -> anyhow::Result<()>`
  - `video::download(url: &str) -> anyhow::Result<Vec<u8>>` (async)

- [ ] **Step 1: Add the workspace dependencies**

In the root `Cargo.toml`, under `[workspace.dependencies]`, next to `widevine = { path = "crates/widevine" }` (line 23):

```toml
video = { path = "crates/video" }
```

And in the external dependency list, alphabetically:

```toml
bzip2 = "0.6"
mp4 = "0.14"
openh264 = { version = "0.9.8", default-features = false, features = ["libloading"] }
```

`members = ["crates/*"]` already picks the crate up.

- [ ] **Step 2: Write the manifest**

`crates/video/Cargo.toml`:

```toml
[package]
name = "video"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
bzip2.workspace = true
dirs.workspace = true
log.workspace = true
mp4.workspace = true
openh264.workspace = true
reqwest.workspace = true
```

- [ ] **Step 3: Write `codec.rs`**

```rust
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
    OpenH264API::from_blob_path(&path)
        .with_context(|| format!("cannot load {}", path.display()))
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
```

- [ ] **Step 4: Write `lib.rs`**

```rust
//! A short, silent H.264 clip played as frames: the decoder store, the download and the
//! looping player. Knows nothing about music or gpui; `music::video` is the one door into it.

pub mod codec;

use anyhow::{Context as _, Result};

/// Fetches a file whole. Canvases are a few MB and Cisco's binary under one, so nothing streams.
pub async fn download(url: &str) -> Result<Vec<u8>> {
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("cannot reach {url}"))?
        .error_for_status()
        .with_context(|| format!("{url} refused the download"))?;
    Ok(response.bytes().await.context("cannot read the download")?.to_vec())
}
```

- [ ] **Step 5: Check the crate builds and is a leaf**

```sh
cargo check -p video
cargo tree -p video --depth 1 | grep -E "music|state|views|gpui" || echo "leaf: ok"
cargo tree -p video -e features -i openh264-sys2 | grep -q '"source"' && echo "SOURCE FEATURE ON — WRONG" || echo "libloading only: ok"
```

Expected: `cargo check` clean, `leaf: ok`, `libloading only: ok`.

- [ ] **Step 6: Manual check 1.a — the install works end to end**

Add nothing to the repo. From a scratch binary, or temporarily in `crates/video/examples/` (delete it again afterwards), call `video::codec::install().await` and then `video::codec::api()`. Expected: a file at `~/.cache/sonora/openh264/2.6.0/libopenh264-2.6.0-linux64.8.so` and `api()` answering `Ok`. Delete the example and the store afterwards: `rm -r ~/.cache/sonora/openh264`.

- [ ] **Step 7: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

Proposed, for the user to approve:

```
git commit -m "feat(music): add a video crate that fetches cisco's openh264 on request" -- Cargo.toml Cargo.lock crates/video
```

---

### Task 2: The frame pipeline and the looping player

**Files:**
- Create: `crates/video/src/annexb.rs`
- Create: `crates/video/src/convert.rs`
- Create: `crates/video/src/player.rs`
- Modify: `crates/video/src/lib.rs`

**Interfaces:**
- Consumes: `codec::api()` (Task 1).
- Produces:
  - `video::Frame { pub pixels: Vec<u8>, pub width: u32, pub height: u32, pub index: u64 }` (BGRA)
  - `video::Player::start(bytes: Vec<u8>) -> anyhow::Result<Player>`
  - `Player::frame(&self) -> Option<Arc<Frame>>`
  - `Player::watch(&self, width: u32)`
  - `Player::set_running(&self, running: bool)`
  - Dropping a `Player` stops its thread.

- [ ] **Step 1: Copy the Annex-B converter**

Copy `~/.cargo/registry/src/index.crates.io-*/openh264-0.9.8/examples/mp4/mp4_bitstream_converter.rs` to `crates/video/src/annexb.rs` unchanged apart from:
- a first line `//! MP4 keeps H.264 as length-prefixed NAL units and its SPS/PPS in the avcC box; openh264 wants Annex-B start codes. Adapted from openh264-rs's own example (BSD-2-Clause).`
- `pub struct Mp4BitstreamConverter` → `pub(crate) struct Mp4BitstreamConverter`, and the same for its `for_mp4_track` and `convert_packet`.
- `anyhow::anyhow!` stays; the file already uses `anyhow`.

- [ ] **Step 2: Write `convert.rs`**

```rust
//! YUV 4:2:0 to BGRA, BT.709 limited range (what every verified canvas is tagged), sampled
//! straight to the output size so a 720-wide clip drawn 300 wide costs a 300-wide pass.

use openh264::formats::YUVSource;

/// Converts `yuv` to BGRA at `width` (never wider than the source), keeping the aspect ratio.
/// Odd source sizes are fine: chroma is read at `x / 2`, `y / 2` and clamped to its plane.
pub(crate) fn bgra(yuv: &impl YUVSource, width: u32) -> (Vec<u8>, u32, u32) {
    let (source_w, source_h) = yuv.dimensions();
    let out_w = (width as usize).clamp(1, source_w);
    let out_h = ((source_h * out_w) as f32 / source_w as f32).round().max(1.0) as usize;
    let (y_stride, u_stride, v_stride) = yuv.strides();
    let (plane_y, plane_u, plane_v) = (yuv.y(), yuv.u(), yuv.v());
    let chroma_w = source_w.div_ceil(2);
    let chroma_h = source_h.div_ceil(2);

    let mut out = vec![0u8; out_w * out_h * 4];
    for row in 0..out_h {
        let sy = (row * source_h / out_h).min(source_h - 1);
        let cy = (sy / 2).min(chroma_h - 1);
        for col in 0..out_w {
            let sx = (col * source_w / out_w).min(source_w - 1);
            let cx = (sx / 2).min(chroma_w - 1);
            let y = plane_y[sy * y_stride + sx] as f32;
            let u = plane_u[cy * u_stride + cx] as f32 - 128.0;
            let v = plane_v[cy * v_stride + cx] as f32 - 128.0;
            let luma = 1.164 * (y - 16.0);
            let r = luma + 1.793 * v;
            let g = luma - 0.213 * u - 0.533 * v;
            let b = luma + 2.112 * u;
            let at = (row * out_w + col) * 4;
            out[at] = b.clamp(0.0, 255.0) as u8;
            out[at + 1] = g.clamp(0.0, 255.0) as u8;
            out[at + 2] = r.clamp(0.0, 255.0) as u8;
            out[at + 3] = 255;
        }
    }
    (out, out_w as u32, out_h as u32)
}
```

- [ ] **Step 3: Write `player.rs`**

```rust
//! Plays one clip in a loop on a thread of its own. The clip is parsed once; the thread decodes
//! a frame at the clip's own pace, and sleeps whenever it is paused or nothing has drawn a frame
//! for `UNWATCHED`, so a canvas nobody sees costs nothing.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use openh264::decoder::{Decoder, DecoderConfig, Flush};

use crate::annexb::Mp4BitstreamConverter;
use crate::convert;

/// How long after the last `watch` the thread keeps decoding.
const UNWATCHED: Duration = Duration::from_millis(500);
/// How often an idle thread looks again.
const IDLE: Duration = Duration::from_millis(50);
/// The width a player starts at before anything has asked for one.
const FIRST_WIDTH: u32 = 360;

/// One decoded picture, BGRA, the way gpui's `RenderImage` wants it. `index` only grows, so a
/// view can tell a new frame from the one it already holds.
pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub index: u64,
}

struct Shared {
    latest: Mutex<Option<Arc<Frame>>>,
    width: AtomicU32,
    running: AtomicBool,
    stop: AtomicBool,
    /// Milliseconds since `born` at the last `watch`.
    seen: AtomicU64,
    born: Instant,
}

/// A looping clip. Dropping it stops the thread.
pub struct Player {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

struct Clip {
    packets: Vec<Vec<u8>>,
    frame: Duration,
}

impl Player {
    /// Parses `bytes` as an MP4 with one H.264 track and starts the thread paused and unwatched.
    /// Fails when the file is no such MP4 or no decoder can be loaded.
    pub fn start(bytes: Vec<u8>) -> Result<Self> {
        let clip = parse(&bytes)?;
        let api = crate::codec::api()?;
        let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
        let decoder = Decoder::with_api_config(api, config).context("cannot open the decoder")?;
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            width: AtomicU32::new(FIRST_WIDTH),
            running: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            seen: AtomicU64::new(0),
            born: Instant::now(),
        });
        let thread = std::thread::Builder::new()
            .name("canvas".into())
            .spawn({
                let shared = shared.clone();
                move || run(clip, decoder, &shared)
            })
            .context("cannot start the canvas thread")?;
        Ok(Self { shared, thread: Some(thread) })
    }

    /// The newest frame, if one has been decoded yet.
    pub fn frame(&self) -> Option<Arc<Frame>> {
        self.shared.latest.lock().ok()?.clone()
    }

    /// Says a surface is drawing this clip at `width` pixels. Call it every frame the clip is on
    /// screen; the widest ask of the last moment wins.
    pub fn watch(&self, width: u32) {
        let now = self.shared.born.elapsed().as_millis() as u64;
        let before = self.shared.seen.swap(now, Ordering::Relaxed);
        let stale = now.saturating_sub(before) > UNWATCHED.as_millis() as u64;
        match stale {
            true => self.shared.width.store(width.max(1), Ordering::Relaxed),
            false => {
                self.shared.width.fetch_max(width.max(1), Ordering::Relaxed);
            }
        }
    }

    /// Pauses or resumes the loop, following playback.
    pub fn set_running(&self, running: bool) {
        self.shared.running.store(running, Ordering::Relaxed);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Clip> {
    let mut mp4 = mp4::Mp4Reader::read_header(Cursor::new(bytes), bytes.len() as u64)
        .context("cannot read the canvas")?;
    let track = mp4
        .tracks()
        .values()
        .find(|track| track.media_type().ok() == Some(mp4::MediaType::H264))
        .context("the canvas has no h264 track")?;
    let (id, count) = (track.track_id(), track.sample_count());
    let fps = track.frame_rate();
    let frame = Duration::from_secs_f64(1.0 / if fps > 1.0 { fps } else { 25.0 });
    let mut convert = Mp4BitstreamConverter::for_mp4_track(track)?;
    let mut packets = Vec::with_capacity(count as usize);
    for index in 1..=count {
        let Some(sample) = mp4.read_sample(id, index)? else { continue };
        let mut packet = Vec::new();
        convert.convert_packet(&sample.bytes, &mut packet);
        packets.push(packet);
    }
    anyhow::ensure!(!packets.is_empty(), "the canvas has no frames");
    Ok(Clip { packets, frame })
}

/// The loop. The first packet carries SPS/PPS and an IDR, so feeding it again restarts the
/// decoder cleanly at every lap. `Flush::NoFlush` holds two frames back, so the last two of a
/// lap are skipped; at 25–30 fps that is invisible.
fn run(clip: Clip, mut decoder: Decoder, shared: &Shared) {
    let mut next = 0usize;
    let mut index = 0u64;
    let mut due = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let now = shared.born.elapsed().as_millis() as u64;
        let watched = now.saturating_sub(shared.seen.load(Ordering::Relaxed))
            <= UNWATCHED.as_millis() as u64;
        if !shared.running.load(Ordering::Relaxed) || !watched {
            std::thread::sleep(IDLE);
            due = Instant::now();
            continue;
        }
        match decoder.decode(&clip.packets[next]) {
            Ok(Some(image)) => {
                let width = shared.width.load(Ordering::Relaxed);
                let (pixels, width, height) = convert::bgra(&image, width);
                index += 1;
                if let Ok(mut latest) = shared.latest.lock() {
                    *latest = Some(Arc::new(Frame { pixels, width, height, index }));
                }
            }
            Ok(None) => {}
            Err(error) => log::debug!("canvas: frame {next} did not decode: {error}"),
        }
        next = (next + 1) % clip.packets.len();
        due += clip.frame;
        let wait = due.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            due = Instant::now();
        }
        std::thread::sleep(wait);
    }
}
```

> Note for the implementer: `mp4::Mp4Track::frame_rate()` exists in `mp4` 0.14 and returns `f64`. If `cargo check` says otherwise, compute it as `sample_count / duration.as_secs_f64()` from `track.duration()`.

- [ ] **Step 4: Export from `lib.rs`**

Add under `pub mod codec;`:

```rust
mod annexb;
mod convert;
mod player;

pub use player::{Frame, Player};
```

- [ ] **Step 5: Build**

```sh
cargo check -p video && cargo clippy -p video --all-targets
```

Expected: clean.

- [ ] **Step 6: Manual checks 2.a and 2.b — both verified canvases decode, the odd width included**

With the decoder from Task 1 installed, a throwaway example (not committed) that downloads
`https://canvaz.scdn.co/upload/artist/0C0XlULifJtAgn6ZNCW2eu/video/b3f0621714af4ed58ab1cabee59b3f1c.cnvs.mp4` (720 × 1280) and
`https://canvaz.scdn.co/upload/artist/2lpFs8QJyIeVDb2Sq4vZYi/video/63fc45a458704376af4af47d79bdf96e.cnvs.mp4` (718 × 1280),
starts a `Player` on each, calls `set_running(true)` and `watch(300)` every 100 ms for 3 s, and prints `frame().map(|f| (f.index, f.width, f.height))`.
Expected: the index climbs by ~25–30 a second, the size is `300 × 533` for both, no panic. Write one frame to a `.ppm` (swap B and R back) and check it looks like the ffmpeg frame, colours included.

- [ ] **Step 7: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(music): decode a looping h264 clip to bgra frames" -- crates/video
```

---

### Task 3: `MusicApi::canvas` and Spotify's answer

**Files:**
- Modify: `crates/music/Cargo.toml`
- Create: `crates/music/src/video.rs`
- Modify: `crates/music/src/lib.rs` (the `MusicApi` trait at line 83; module list)
- Create: `crates/music/src/spotify/canvas.rs`
- Modify: `crates/music/src/spotify/mod.rs` (module list)
- Modify: `crates/music/src/spotify/client.rs` (`impl MusicApi for LibrespotClient`, line 50)
- Modify: `crates/music/src/tagged/mod.rs`

**Interfaces:**
- Consumes: `collection::extended(session, uris, kind)` (`crates/music/src/spotify/collection.rs:65`).
- Produces:
  - `MusicApi::canvas(&self, track_id: &str) -> Result<Option<String>>`, default `Ok(None)`
  - `music::video` re-exporting `video::{Frame, Player, codec, download}`

- [ ] **Step 1: The dependency and the door**

`crates/music/Cargo.toml` `[dependencies]`: `video.workspace = true`.

`crates/music/src/video.rs`:

```rust
//! The one door `state` and `views` use into the `video` crate, as `drm` is for Widevine.

pub use video::{Frame, Player, codec, download};
```

In `crates/music/src/lib.rs`, next to `pub mod drm;`: `pub mod video;`.

- [ ] **Step 2: The trait method**

In `MusicApi` (`crates/music/src/lib.rs`), next to `track_tags`:

```rust
    /// The url of the short looping video an artist attached to this track, or `None` when it
    /// has none. Only Spotify answers; every other provider has no such thing.
    async fn canvas(&self, _track_id: &str) -> Result<Option<String>> {
        Ok(None)
    }
```

- [ ] **Step 3: Spotify's answer**

`crates/music/src/spotify/canvas.rs`:

```rust
//! A track's Canvas through extended metadata, the same batched route `TRACK_V4` takes.

use anyhow::{Context as _, Result};
use librespot_core::Session;
use librespot_protocol::canvaz::Type;
use librespot_protocol::canvaz::entity_canvaz_response::Canvaz;
use librespot_protocol::extension_kind::ExtensionKind;
use protobuf::Message as _;

use super::collection;

/// The video url for `track_id`, or `None` when the track has no canvas or only a still one.
/// An empty extension is how Spotify says "none".
pub(crate) async fn canvas(session: &Session, track_id: &str) -> Result<Option<String>> {
    let uri = format!("spotify:track:{track_id}");
    let entities = collection::extended(session, std::slice::from_ref(&uri), ExtensionKind::CANVAZ)
        .await
        .context("cannot read the canvas")?;
    let Some(entity) = entities.into_iter().find(|entity| entity.entity_uri == uri) else {
        return Ok(None);
    };
    if entity.extension_data.value.is_empty() {
        return Ok(None);
    }
    let canvas = Canvaz::parse_from_bytes(&entity.extension_data.value)
        .context("cannot parse the canvas")?;
    let moving = matches!(
        canvas.type_.enum_value(),
        Ok(Type::VIDEO | Type::VIDEO_LOOPING | Type::VIDEO_LOOPING_RANDOM)
    );
    Ok((moving && !canvas.url.is_empty()).then_some(canvas.url))
}
```

Register it in `crates/music/src/spotify/mod.rs` beside the other `mod` lines: `mod canvas;`.

In `impl MusicApi for LibrespotClient` (`client.rs`), beside `track`:

```rust
    async fn canvas(&self, track_id: &str) -> Result<Option<String>> {
        canvas::canvas(&self.session, track_id).await
    }
```

(with `use super::canvas;` at the top if the module's imports are explicit).

- [ ] **Step 4: Forward it through `Tagged`**

`crates/music/src/tagged/mod.rs` forwards every method explicitly; a missing one silently falls back to the default. Beside `track_tags`:

```rust
    async fn canvas(&self, track_id: &str) -> Result<Option<String>> {
        self.inner.canvas(tag::untag(track_id)).await
    }
```

- [ ] **Step 5: Manual check 3.a — the live answer**

A throwaway `#[ignore]`d test or example (not committed), like the one this plan was verified with: restore the session with `auth::restore(&AuthConfig::from_env())` (install `rustls::crypto::ring::default_provider()` first, as `crates/music/examples/library-probe.rs` does) and call `canvas::canvas` for `3n3Ppam7vgaVa1iaRUc9Lp` and `0VjIjW4GlUZAMYd2vXMi3b`. Expected: `Some("https://canvaz.scdn.co/…cnvs.mp4")` and `None`. Remove it afterwards.

- [ ] **Step 6: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(music): read a spotify track's canvas from extended metadata" -- crates/music
```

---

### Task 4: Consent, the decoder entity and its Settings rows

**Files:**
- Create: `crates/state/src/codec.rs`
- Modify: `crates/state/src/lib.rs` (module, `pub use`, `Sonora` field near `pub drm` at line 160, construction near line 233)
- Modify: `crates/state/src/settings.rs` (`struct Appearance` at line 349, its `Default`, getter and setter beside `check_updates` at lines 949 and 1376)
- Create: `crates/views/src/shared/codec.rs`
- Modify: `crates/views/src/shared/mod.rs` (module list)
- Modify: `crates/views/src/shells/workspace.rs` (lines 20, 52, 83, 374, beside `WidevinePrompt`)
- Modify: `crates/views/src/screens/settings.rs` (`Slot` enum ~line 164, `tab_slots` ~580-593, `slot_meta` ~663, `slot_element` ~889, a `canvas_row` and `codec_row` beside `widevine_row` at 2429)
- Modify: `assets/i18n/en-US/main.ftl`

**Interfaces:**
- Consumes: `music::video::codec::{supported, find, install, uninstall}` (Tasks 1, 3).
- Produces:
  - `state::CodecState { Looking, Ready, Missing, Wanted, Installing, Declined, Failed }`
  - `state::Codec::{state, ready, want, install, dismiss, uninstall}`
  - `Sonora::global(cx).codec: Entity<Codec>`
  - `AppSettings::canvas() -> bool`, `AppSettings::set_canvas(bool, cx)`

- [ ] **Step 1: The setting**

In `struct Appearance` (`settings.rs:349`), after `ambient_motion`:

```rust
    /// Plays a track's Canvas in place of its cover where one exists.
    canvas: bool,
```

In `Appearance`'s `Default`: `canvas: true,`. Then, beside `check_updates` and `set_check_updates`, following their exact shape:

```rust
    pub fn canvas(&self) -> bool {
        self.values.appearance.canvas
    }

    pub fn set_canvas(&mut self, canvas: bool, cx: &mut Context<Self>) {
        self.values.appearance.canvas = canvas;
        self.schedule_save(cx);
    }
```

(Match how `ambient` is read: if the field path is not `values.appearance`, copy the `ambient()` accessor's path.)

- [ ] **Step 2: The `Codec` entity**

`crates/state/src/codec.rs`:

```rust
//! Whether Cisco's OpenH264 is here, and the one question that fetches it. Modelled on `Drm`:
//! nothing is looked for until a canvas actually wants playing, and "not now" lasts the run.

use gpui::{Context, Task};
use music::video::codec;

use crate::{Io, join};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecState {
    /// Not looked for yet.
    Looking,
    Ready,
    /// Looked for, not found, not asked about.
    Missing,
    /// The prompt is up.
    Wanted,
    Installing,
    /// The user said not now; asked again only from Settings.
    Declined,
    /// The download or the check failed; Settings can retry.
    Failed,
}

pub struct Codec {
    state: CodecState,
    io: Io,
    task: Option<Task<()>>,
}

impl Codec {
    pub fn new(io: Io) -> Self {
        Self { state: CodecState::Looking, io, task: None }
    }

    pub fn state(&self) -> CodecState {
        self.state
    }

    /// Whether Cisco publishes a binary for this platform, which is when Settings shows a row.
    pub fn supported(&self) -> bool {
        codec::supported()
    }

    /// Looks once, lazily, and answers whether a decoder can be loaded now.
    pub fn ready(&mut self) -> bool {
        if self.state == CodecState::Looking {
            self.state = match codec::find() {
                Some(_) => CodecState::Ready,
                None => CodecState::Missing,
            };
        }
        self.state == CodecState::Ready
    }

    /// A canvas wants playing and there is no decoder: raise the prompt, once a run.
    pub fn want(&mut self, cx: &mut Context<Self>) {
        if !self.ready() && self.state == CodecState::Missing && codec::supported() {
            self.state = CodecState::Wanted;
            cx.notify();
        }
    }

    /// Downloads and installs the decoder, from the prompt or from Settings.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        self.state = CodecState::Installing;
        cx.notify();
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = join(io.spawn(codec::install())).await;
            this.update(cx, |this, cx| {
                this.task = None;
                this.state = match result {
                    Ok(_) => CodecState::Ready,
                    Err(error) => {
                        log::warn!("codec: cannot install openh264: {error:#}");
                        CodecState::Failed
                    }
                };
                cx.notify();
            })
            .ok();
        }));
    }

    /// "Not now" on the prompt.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.state = CodecState::Declined;
        cx.notify();
    }

    /// Removes the decoder from Settings. A canvas already playing keeps its open decoder.
    pub fn uninstall(&mut self, cx: &mut Context<Self>) {
        match codec::uninstall() {
            Ok(()) => self.state = CodecState::Missing,
            Err(error) => log::warn!("codec: cannot remove openh264: {error:#}"),
        }
        cx.notify();
    }
}
```

In `crates/state/src/lib.rs`: `mod codec;`, `pub use codec::{Codec, CodecState};`, a `pub codec: Entity<Codec>` field on `Sonora` after `pub drm`, and `let codec = cx.new(|_| Codec::new(io.clone()));` next to the `Drm` construction (line 233), passed into the `Sonora { … }` literal.

- [ ] **Step 3: The strings**

`assets/i18n/en-US/main.ftl`, beside the `widevine-prompt-*` keys:

```ftl
canvas-prompt-title = Play this song's canvas?
canvas-prompt-detail = A canvas is the short video an artist made for a song. Playing it needs the OpenH264 decoder, which Sonora downloads from Cisco once.
canvas-prompt-download = Download
canvas-prompt-later = Not now
canvas-prompt-installing = Downloading the decoder…
```

Beside the other `settings-*` appearance keys:

```ftl
settings-canvas = Canvas
settings-canvas-detail = Plays the short video an artist made for a song in place of its cover
settings-canvas-codec = Video decoder
settings-canvas-codec-ready = OpenH264 Video Codec provided by Cisco Systems, Inc.
settings-canvas-codec-missing = Needed to play a canvas, downloaded from Cisco
settings-canvas-codec-failed = The download did not work, try again
settings-canvas-codec-installing = Downloading…
settings-canvas-codec-fetch = Download
settings-canvas-codec-remove = Remove
```

Then run `python3 scripts/i18n-coverage.py`.

- [ ] **Step 4: The prompt**

Read `crates/views/src/shared/widevine.rs` whole first; `CodecPrompt` is the same shape, minus the licence step. `crates/views/src/shared/codec.rs`:
- `pub(crate) struct CodecPrompt { codec: Entity<state::Codec>, focus: FocusHandle, restore: Option<FocusHandle>, shown: bool }`, with `new(window, cx)` reading `Sonora::global(cx).codec` and `cx.observe(&codec, |_, _, cx| cx.notify()).detach()`, exactly as `WidevinePrompt::new` does with `drm`.
- Open while `state` is `Wanted | Installing`.
- One `Modal::new("canvas-prompt", t!("canvas-prompt-title"))` with the detail `t!("canvas-prompt-detail")`; in `Wanted` the buttons `t!("canvas-prompt-download")` → `codec.install(cx)` and `t!("canvas-prompt-later")` → `codec.dismiss(cx)`; in `Installing` the proceed button disabled and the note `t!("canvas-prompt-installing")`. Dismissing the modal calls `dismiss`.
- Copy `WidevinePrompt`'s focus save/restore and `shown` handling verbatim.

Register the module in `crates/views/src/shared/mod.rs`, then mount it in `crates/views/src/shells/workspace.rs` the way `widevine` is (import line 20, field line 52, `cx.new(CodecPrompt::new)` line 83, `.child(self.codec.clone())` line 374).

- [ ] **Step 5: The Settings rows**

In `crates/views/src/screens/settings.rs`:
- `Slot::Canvas` and `Slot::CanvasCodec` in the `Slot` enum beside `Slot::Widevine`.
- In `tab_slots`, in the Appearance tab's `Slot::Title("settings-group-fullscreen")` group after `Slot::Ambient`: `Slot::Canvas`, then `Slot::CanvasCodec` only when `self.codec.read(cx).supported()` (hold `codec: Entity<state::Codec>` on the view like `drm`).
- `slot_meta`: `Slot::Canvas => (t!("settings-canvas"), t!("settings-canvas-detail"))`, `Slot::CanvasCodec => (t!("settings-canvas-codec"), t!("settings-canvas-codec-missing"))`.
- `slot_element`: `Slot::Canvas => self.canvas_row(cx).element`, `Slot::CanvasCodec => self.codec_row(cx).element`.
- `canvas_row` copies `updates_row` (line 2481): `Switch::new("canvas", on)` toggling `set_canvas(!on, cx)`.
- `codec_row` copies `widevine_row` (lines 2429–2479): the detail key follows the state (`Ready` → `settings-canvas-codec-ready`, `Failed` → `settings-canvas-codec-failed`, `Installing` → `settings-canvas-codec-installing`, anything else → `settings-canvas-codec-missing`); the button is `settings-canvas-codec-fetch` → `codec.install` in `Missing | Declined | Failed`, disabled in `Installing`, and `settings-canvas-codec-remove` → `codec.uninstall` in `Ready`. Call `codec.ready()` through `update` once when the row is built, so the row shows the truth before any canvas has played.

- [ ] **Step 6: Manual checks 4.a–4.c**

`cargo run --package sonora`, then:
- **4.a** Settings → Appearance shows *Canvas* (on) and *Video decoder* with *Download*. Download: the button disables, then the detail reads Cisco's attribution line and the button becomes *Remove*. `~/.cache/sonora/openh264/2.6.0/` holds the file.
- **4.b** *Remove*: the store is gone and the row offers *Download* again.
- **4.c** Offline (or `SONORA_OPENH264` unset and the network cut): *Download* ends in the failed detail, one `codec: cannot install openh264` warning in `~/.local/state/sonora/sonora.log`, and *Download* works again once online.

- [ ] **Step 7: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(settings): offer cisco's openh264 decoder for canvases" -- crates/state crates/views assets/i18n README.md
```

---

### Task 5: The `Canvas` entity

**Files:**
- Create: `crates/state/src/canvas.rs`
- Modify: `crates/state/src/lib.rs`

**Interfaces:**
- Consumes: `MusicApi::canvas` (Task 3), `music::video::{download, Player}` (Tasks 1–3), `Codec::{ready, want}` (Task 4), `AppSettings::canvas` (Task 4), `Playback::{track, state}`, `Session::{slug_for, client_for_slug}`, `crate::join`.
- Produces:
  - `state::Canvas::player(&self) -> Option<&music::video::Player>` — `Some` only for the current track, with the setting on and a decoder loaded
  - `Sonora::global(cx).canvas: Entity<Canvas>`

- [ ] **Step 1: Write the entity**

`crates/state/src/canvas.rs`, modelled line for line on `state/src/cover.rs`:

```rust
//! The Canvas of the playing track: its url from the provider, its bytes from the CDN, and the
//! `Player` that loops it. Only the current track ever has a player, so a late answer for a
//! track already skipped is dropped rather than shown.

use std::collections::HashMap;

use gpui::{Context, Entity, Task};
use music::video::{Player, download};

use crate::{AppSettings, Codec, Io, Playback, PlaybackState, Session, SessionEvent, join};

pub struct Canvas {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    codec: Entity<Codec>,
    /// The tagged id of the track this entity is following.
    track: Option<String>,
    /// What each track answered, `None` meaning it has no canvas, so a replay asks nothing.
    urls: HashMap<String, Option<String>>,
    player: Option<Player>,
    io: Io,
    task: Option<Task<()>>,
}

impl Canvas {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        codec: Entity<Codec>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&playback, |this, _, cx| this.follow(cx)).detach();
        cx.observe(&settings, |this, _, cx| this.follow(cx)).detach();
        cx.observe(&codec, |this, _, cx| this.retry(cx)).detach();
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedOut(slug) => this.forget(slug, cx),
            SessionEvent::SignedIn(_) | SessionEvent::Reconnected(_) | SessionEvent::LocalChanged => {}
        })
        .detach();
        Self {
            session,
            playback,
            settings,
            codec,
            track: None,
            urls: HashMap::new(),
            player: None,
            io,
            task: None,
        }
    }

    /// The player for the current track, while the setting allows one and a decoder loaded.
    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// Runs on every playback and settings change, so the common case (same track, nothing to
    /// do) has to return before any work.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let playback = self.playback.read(cx);
        let playing = playback.state() == PlaybackState::Playing;
        let track = playback.track().and_then(|track| track.id.clone());
        let wanted = self.settings.read(cx).canvas();
        if let Some(player) = &self.player {
            player.set_running(playing);
        }
        if track != self.track {
            self.task = None;
            self.player = None;
            self.track = track.clone();
            cx.notify();
        } else if !wanted && self.player.is_some() {
            self.task = None;
            self.player = None;
            cx.notify();
            return;
        }
        let Some(id) = track else { return };
        if wanted && self.player.is_none() && self.task.is_none() {
            self.load(id, cx);
        }
    }

    /// The decoder arrived: try the current track again.
    fn retry(&mut self, cx: &mut Context<Self>) {
        let ready = self.codec.update(cx, |codec, _| codec.ready());
        if ready
            && self.player.is_none()
            && self.task.is_none()
            && self.settings.read(cx).canvas()
            && let Some(id) = self.track.clone()
        {
            self.load(id, cx);
        }
    }

    fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.urls.retain(|id, _| music::tag::slug_of(id) != Some(slug));
        if self.track.as_deref().and_then(music::tag::slug_of) == Some(slug) {
            self.task = None;
            self.player = None;
            cx.notify();
        }
    }

    /// Resolves, downloads and starts. Returns at once when the track is known to have no
    /// canvas, or has one but no decoder is loaded yet (then it only raises the prompt, and
    /// `retry` comes back once the decoder arrives).
    fn load(&mut self, id: String, cx: &mut Context<Self>) {
        match self.urls.get(&id) {
            Some(None) => return,
            Some(Some(_)) if !self.codec.update(cx, |codec, _| codec.ready()) => {
                self.codec.update(cx, |codec, cx| codec.want(cx));
                return;
            }
            _ => {}
        }
        let session = self.session.read(cx);
        let Some(client) = session.slug_for(&id).and_then(|slug| session.client_for_slug(slug))
        else {
            return;
        };
        let known = self.urls.get(&id).cloned().flatten();
        let io = self.io.clone();
        let wanted = id.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let url = match known {
                Some(url) => Ok(Some(url)),
                None => join(io.spawn(async move { client.canvas(&wanted).await })).await,
            };
            let url = match url {
                Ok(url) => url,
                Err(error) => {
                    log::warn!("canvas: cannot look up {id}: {error:#}");
                    return;
                }
            };
            let current = this
                .update(cx, |this, cx| {
                    this.urls.insert(id.clone(), url.clone());
                    let ready = this.codec.update(cx, |codec, cx| {
                        let ready = codec.ready();
                        if !ready && url.is_some() {
                            codec.want(cx);
                        }
                        ready
                    });
                    this.track.as_deref() == Some(id.as_str()) && ready
                })
                .unwrap_or(false);
            let (Some(url), true) = (url, current) else { return };
            let fetched = join(io.spawn(async move { download(&url).await })).await;
            let started = fetched.and_then(Player::start);
            this.update(cx, |this, cx| {
                this.task = None;
                if this.track.as_deref() != Some(id.as_str()) {
                    return;
                }
                match started {
                    Ok(player) => {
                        let playing = this.playback.read(cx).state() == PlaybackState::Playing;
                        player.set_running(playing);
                        this.player = Some(player);
                        cx.notify();
                    }
                    Err(error) => log::warn!("canvas: cannot play {id}: {error:#}"),
                }
            })
            .ok();
        }));
    }
}
```

> Note for the implementer: `Player::start` parses the clip and loads the decoder, a few milliseconds; if a profile shows it on the foreground, move it into `io.spawn_blocking(move || Player::start(bytes))`. `PlaybackState::Playing` is the variant name from `state/src/playback.rs`; confirm it before building.

- [ ] **Step 2: Register it**

`crates/state/src/lib.rs`: `mod canvas;`, `pub use canvas::Canvas;`, a `pub canvas: Entity<Canvas>` field on `Sonora`, and after `playback`, `settings` and `codec` exist:

```rust
let canvas = cx.new(|cx| Canvas::new(session.clone(), playback.clone(), settings.clone(), codec.clone(), io.clone(), cx));
```

- [ ] **Step 3: Manual checks 5.a–5.c**

Nothing draws the canvas yet, so check through the log. Add a temporary `log::debug!("canvas: playing {id}")` beside `this.player = Some(player)` and remove it again before the commit. With the decoder installed, `cargo run --package sonora`:
- **5.a** Play `spotify:track:3n3Ppam7vgaVa1iaRUc9Lp`: one `canvas: playing …` line.
- **5.b** Skip through five Spotify tracks in two seconds, ending on one with a canvas: exactly one `canvas: playing` line, for the last track.
- **5.c** Play a YouTube, SoundCloud or local track, and `0VjIjW4GlUZAMYd2vXMi3b`: no line, no warning, no prompt. Replay the same Spotify track: no second lookup (the url cache answers).

- [ ] **Step 4: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(state): follow the playing track's canvas" -- crates/state
```

---

### Task 6: `CanvasArt`, the frame pump

**Files:**
- Create: `crates/views/src/shared/canvas.rs`
- Modify: `crates/views/src/shared/mod.rs`

**Interfaces:**
- Consumes: `Canvas::player()` (Task 5), `Player::{frame, watch}` (Task 2), `ui::motion::animates(cx)`, `shared::effects()`.
- Produces:
  - `pub(crate) struct CanvasArt` (a `Render` entity)
  - `CanvasArt::new(cx: &mut Context<Self>) -> Self`
  - `CanvasArt::set_side(&mut self, width: Pixels, height: Pixels, radius: Pixels)`
  - `pub(crate) fn canvas_playing(cx: &App) -> bool`

- [ ] **Step 1: Write the view**

Read `crates/views/src/shared/visualizer.rs` (`VisualizerDrive::show`, `step`, `hide`) and `crates/ui/src/grain.rs:19-41` first; this copies the first's `on_next_frame` arming and the second's `img(Arc<RenderImage>)`.

```rust
//! Draws the playing track's Canvas. Each frame it asks the player for its newest picture and
//! swaps the image only when that picture is new, dropping the old one from the atlas, so a
//! 25 fps clip uploads 25 textures a second and never piles them up.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{App, Context, ObjectFit, Pixels, RenderImage, Window, img, px};
use image::{Frame, RgbaImage};
use state::Sonora;

/// Whether the current track has a canvas to draw, under every motion rule. It does not wait
/// for a first frame: the player only decodes once something `watch`es it, so the owner mounts
/// `CanvasArt` on this answer and `CanvasArt` draws nothing until a frame exists. The cover
/// stays underneath for that first frame's few milliseconds.
pub(crate) fn canvas_playing(cx: &App) -> bool {
    ui::motion::animates(cx)
        && super::effects()
        && Sonora::global(cx).canvas.read(cx).player().is_some()
}

pub(crate) struct CanvasArt {
    image: Option<Arc<RenderImage>>,
    index: u64,
    width: Pixels,
    height: Pixels,
    radius: Pixels,
    armed: bool,
}

impl CanvasArt {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let canvas = Sonora::global(cx).canvas.clone();
        cx.observe(&canvas, |_, _, cx| cx.notify()).detach();
        Self { image: None, index: 0, width: px(0.), height: px(0.), radius: px(0.), armed: false }
    }

    /// The box the owner lays the canvas out in; the clip covers it, cropped like a cover.
    pub(crate) fn set_side(&mut self, width: Pixels, height: Pixels, radius: Pixels) {
        (self.width, self.height, self.radius) = (width, height, radius);
    }

    /// Takes the player's newest frame when it is new, and keeps the loop armed while one plays.
    fn pull(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wide = (f32::from(self.width.max(self.height * (9. / 16.))) * window.scale_factor()).ceil();
        let entity = Sonora::global(cx).canvas.clone();
        // the read borrow ends here, before the image work needs `cx` mutably
        let newest = {
            let canvas = entity.read(cx);
            let Some(player) = canvas.player() else {
                self.release(window, cx);
                return;
            };
            player.watch(wide as u32);
            player.frame()
        };
        if let Some(frame) = newest
            && frame.index != self.index
            && let Some(pixels) = RgbaImage::from_raw(frame.width, frame.height, frame.pixels.clone())
        {
            let fresh = Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
            if let Some(old) = self.image.replace(fresh) {
                cx.drop_image(old, Some(window));
            }
            self.index = frame.index;
        }
        if !self.armed {
            self.armed = true;
            let view = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.armed = false;
                    cx.notify();
                })
                .ok();
            });
        }
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(old) = self.image.take() {
            cx.drop_image(old, Some(window));
        }
        self.index = 0;
    }
}

impl Render for CanvasArt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match canvas_playing(cx) {
            true => self.pull(window, cx),
            false => self.release(window, cx),
        }
        let Some(image) = self.image.clone() else {
            return gpui::div().into_any_element();
        };
        img(image)
            .w(self.width)
            .h(self.height)
            .object_fit(ObjectFit::Cover)
            .rounded(self.radius)
            .into_any_element()
    }
}
```

> Note for the implementer: `cx.drop_image(image, Some(window))` is the call `ui::artwork` already makes (`artwork.rs:321, 360`); match its exact signature there. `image::Frame::new(RgbaImage)` with BGRA bytes inside is what `ui::grain` and `ui::artwork::bgra` do; the name says RGBA, the bytes are BGRA. The `on_next_frame` loop dies on its own the first render `canvas_playing` answers `false`, the way `VisualizerDrive::hide` lets its loop die. `views` depends on `image` already through `ui`; if it is not a direct dependency of `views`, add `image.workspace = true` to `crates/views/Cargo.toml`.

Register it in `crates/views/src/shared/mod.rs`: `pub(crate) mod canvas;`.

- [ ] **Step 2: Build**

```sh
cargo check -p views && cargo clippy -p views --all-targets
```

- [ ] **Step 3: Manual check 6.c is done in Task 7**, once `CanvasArt` is on screen.

- [ ] **Step 4: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(views): draw a canvas from the player's newest frame" -- crates/views
```

---

### Task 7: The two placements

**Files:**
- Modify: `crates/views/src/shells/fullscreen.rs` (`artwork()` at line 330, struct fields and `new` around lines 106–122)
- Modify: `crates/views/src/chrome/aside.rs` (struct, `new`, and the Queue tab body around lines 864 and 1832–1850)
- Modify: `assets/i18n/en-US/main.ftl` (only if the block gets a label; the plan's default needs none)

**Interfaces:**
- Consumes: `CanvasArt::{new, set_side}`, `canvas_playing` (Task 6).
- Produces: nothing later tasks use.

- [ ] **Step 1: Fullscreen**

In `FullscreenView`, add `canvas: Entity<CanvasArt>` built in `new` with `cx.new(CanvasArt::new)`. In `artwork()`, after the `.when_some(large, …)` child (the block ending at line 405), add the canvas as the topmost layer, fading in with the same `Motion::Slow` the large cover uses:

```rust
                    .when_some(canvas, |this, canvas| {
                        this.child(
                            div()
                                .absolute()
                                .top(pad)
                                .left(pad)
                                .child(canvas)
                                .motion(("cover-canvas", revision), Motion::Slow, |art, t| art.opacity(t)),
                        )
                    })
```

with `canvas` worked out at the top of `artwork()`, before the element tree borrows anything:

```rust
        let canvas = crate::shared::canvas::canvas_playing(cx).then(|| {
            self.canvas.update(cx, |art, _| art.set_side(raster_side, raster_side, radius));
            self.canvas.clone()
        });
```

`artwork` takes `&mut self` already, so the `update` is allowed. The cover stays underneath, so the canvas's first frame never flashes an empty square.

- [ ] **Step 2: Right sidebar**

In `Aside`, add `canvas: Entity<CanvasArt>` built in `new`. At the top of the Queue tab's scrolling body (where `self.tab == SideTab::Queue && !empty` begins, around line 1850), and only when `canvas_playing(cx)`, put one block before the rows:

Work the block out before the element tree, so no closure borrows `self` mutably:

```rust
        let theme = *cx.theme();
        let canvas = (self.tab == SideTab::Queue && crate::shared::canvas::canvas_playing(cx)).then(|| {
            let width = self.width - theme.metrics.pad * 2.;
            let height = (width * (16. / 9.)).min(window.viewport_size().height * CANVAS_SHARE);
            self.canvas.update(cx, |art, _| art.set_side(width, height, theme.radius * 2.));
            self.canvas.clone()
        });
```

and in the tree:

```rust
    .when_some(canvas, |this, canvas| {
        this.child(div().px(theme.metrics.pad).pb(theme.metrics.pad).child(canvas))
    })
```

with a module-top `const CANVAS_SHARE: f32 = 0.55;` (the tallest the block may be, as a share of the window). `self.width` is whatever field `Aside` keeps its panel width in; use it, or `Chrome`'s published right width, never `window.viewport_size().width`. The viewport height here is the panel's own sizing decision, which `CLAUDE.md` allows.

The Queue list's virtualised rows sit in a `Scroller`; if the block has to live outside it to keep row maths exact (`pin` and the anchor logic around lines 1644–1660 measure row offsets), put it above the scroller rather than inside, and let the list take the remaining height.

- [ ] **Step 3: Manual checks 6.c and 7.a–7.d**

`cargo run --package sonora` with the decoder installed and a Canvas track playing (`3n3Ppam7vgaVa1iaRUc9Lp`):
- **7.a** Fullscreen: the canvas fades in over the cover, fills the square cropped, loops, and its colours match the Spotify app.
- **7.b** Right sidebar → Queue: the block sits on top, 9:16 or capped at 55% of the window height, rounded with the theme radius. With a track without a canvas the Queue tab looks exactly as before.
- **7.c** Pause: both stop on their frame. Play: they continue.
- **7.d** Settings → *Canvas* off, reduced motion on (Settings or the system), or `SONORA_BLUR=0`: the cover only, everywhere.
- **6.c** Close fullscreen and switch the sidebar to Lyrics (or hide it): within a second `top -H -p $(pgrep -f target/debug/sonora)` shows the `canvas` thread at ~0% CPU. Back to Queue: it resumes.
- Run with `GPUI_SHOW_REPAINTS=1 cargo prof` once: only the canvas view and its ancestors repaint at the clip's rate.

- [ ] **Step 4: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "feat(views): play the canvas in fullscreen and above the queue" -- crates/views assets/i18n
```

---

### Task 8: Documentation, notices and the changelog

**Files:**
- Modify: `CLAUDE.md`
- Modify: `CHANGELOG.md`
- Modify: `THIRD-PARTY.md` (generated)

- [ ] **Step 1: `CLAUDE.md`**

- In the crate layout block, after the `widevine/` line: `  video/      the OpenH264 store, fetched from Cisco on request, and a looping H.264 clip player`.
- In the dependency-direction block: add `music → video` beside `music → widevine`.
- After the `widevine` bullet in *Crate layout*, one paragraph: `video` is a leaf that knows nothing about music or gpui. It never ships a decoder: `openh264` is built with `libloading` only, and Cisco's own `libopenh264` 2.6.0 is downloaded to `$XDG_CACHE_HOME/sonora/openh264/<version>/` when the user agrees, then loaded through `OpenH264API::from_blob_path`, which checks its sha256. `SONORA_OPENH264` points at a module a package brings. `state::Codec` asks, `state::Canvas` follows the playing track, `views::shared::canvas::CanvasArt` draws the newest BGRA frame. No release artefact, package or Flatpak carries the binary and `about.toml` never gains an entry for it.
- In *Runtime environment*, a row: `| OpenH264 | `$XDG_CACHE_HOME/sonora/openh264/<version>/`, Cisco's binary, fetched on request |`.

- [ ] **Step 2: `CHANGELOG.md`**

Under `## [Unreleased]` → `### Added`:

```markdown
- Spotify songs with a Canvas now play its looping video in fullscreen and above the queue.
```

- [ ] **Step 3: Notices**

```sh
python3 scripts/generate-notices.py
git diff --stat THIRD-PARTY.md
```

Expected: `openh264`, `openh264-sys2`, `mp4` and `bzip2` (and their dependencies) appear. Cisco's binary does not, since nothing ships it.

- [ ] **Step 4: Run the checks and propose the commit**

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

```
git commit -m "docs: describe the canvas and the openh264 store" -- CLAUDE.md CHANGELOG.md THIRD-PARTY.md
```
