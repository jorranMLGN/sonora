//! A plain C ABI for the iOS spike: SoundCloud search and playback through the real `music`
//! crate, plus the version string and the test tone from the first milestone.
//!
//! Every call that answers with data hands back a NUL-terminated JSON string the caller must
//! release with `sonora_free`. A call that fails answers `{"error": "…"}` rather than null, so
//! Swift never has to tell a failure from an empty answer.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use music::soundcloud::SoundCloudProvider;
use music::{
    Equalizer, MusicApi, MusicProvider, PlaybackConfig, PlaybackEvent, Player, SignIn, Track,
};
use serde::Serialize;
use serde_json::json;
use tokio::runtime::Runtime;

const VERSION: &str = concat!("sonora-core ", env!("CARGO_PKG_VERSION"), "\0");
const FREQUENCY: f32 = 440.0;
const VOLUME: f32 = 0.2;
const RESULTS: usize = 30;

/// Everything `sonora_init` sets up: the runtime all network work runs on, the SoundCloud guest
/// session, its player and what the player last reported.
struct Core {
    runtime: Runtime,
    api: Arc<dyn MusicApi>,
    player: Box<dyn Player>,
    now: Arc<Mutex<Now>>,
    found: Mutex<HashMap<String, Track>>,
}

/// What `sonora_now_playing` answers: the track the player was last asked for, and whether the
/// engine says it is sounding.
#[derive(Clone, Default, Serialize)]
struct Now {
    id: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    artwork: Option<String>,
    playing: bool,
    loading: bool,
    position: f64,
    duration: f64,
    error: Option<String>,
}

/// One search result as Swift sees it.
#[derive(Serialize)]
struct Hit {
    id: String,
    title: String,
    artist: String,
    artwork: Option<String>,
    duration: f64,
    playable: bool,
}

static CORE: Mutex<Option<Arc<Core>>> = Mutex::new(None);

/// A static, NUL-terminated version string. The caller must never free it.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

/// Signs in to SoundCloud as a guest and starts the player, keeping the client id under
/// `data_dir`. Blocks on the network; call it off the main thread. Answers `{"ok": true}` or
/// `{"error": …}`, and a second call after a success does nothing.
///
/// # Safety
/// `data_dir` must be null or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sonora_init(data_dir: *const c_char) -> *mut c_char {
    let dir = unsafe { text(data_dir) };
    answer(|| {
        init(dir?)?;
        Ok(json!({ "ok": true }))
    })
}

/// Searches SoundCloud for tracks. Blocks on the network; call it off the main thread.
/// Answers a JSON array of `{id, title, artist, artwork, duration, playable}`.
///
/// # Safety
/// `query` must be null or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sonora_search(query: *const c_char) -> *mut c_char {
    let query = unsafe { text(query) };
    answer(|| {
        let query = query?;
        let core = core()?;
        let api = core.api.clone();
        let tracks = core
            .runtime
            .block_on(async move { api.search(&query).await })?;
        let mut found = core.found.lock().map_err(|_| anyhow!("poisoned"))?;
        let hits: Vec<Hit> = tracks
            .into_iter()
            .filter_map(|track| {
                let id = track.id.clone()?;
                let hit = Hit {
                    id: id.clone(),
                    title: track.name.clone(),
                    artist: track.artists.clone(),
                    artwork: track.cover.clone(),
                    duration: track.duration.as_secs_f64(),
                    playable: track.playable,
                };
                found.insert(id, track);
                Some(hit)
            })
            .take(RESULTS)
            .collect();
        Ok(serde_json::to_value(hits)?)
    })
}

/// Starts playing a track a search returned. Answers `{"ok": true}` once the player has the
/// request; the track itself arrives in the background.
///
/// # Safety
/// `track_id` must be null or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sonora_play(track_id: *const c_char) -> *mut c_char {
    let id = unsafe { text(track_id) };
    answer(|| {
        let id = id?;
        let core = core()?;
        let track = core
            .found
            .lock()
            .map_err(|_| anyhow!("poisoned"))?
            .get(&id)
            .cloned();
        {
            let mut now = core.now.lock().map_err(|_| anyhow!("poisoned"))?;
            *now = Now {
                id: Some(id.clone()),
                title: track.as_ref().map(|track| track.name.clone()),
                artist: track.as_ref().map(|track| track.artists.clone()),
                artwork: track.as_ref().and_then(|track| track.cover.clone()),
                duration: track.as_ref().map_or(0.0, |t| t.duration.as_secs_f64()),
                loading: true,
                ..Now::default()
            };
        }
        core.player.load(&id, Duration::ZERO, false)?;
        Ok(json!({ "ok": true }))
    })
}

/// Pauses a playing track or resumes a paused one. Answers `{"playing": bool}`, the state asked
/// for; the engine confirms it through `sonora_now_playing`.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_toggle() -> *mut c_char {
    answer(|| {
        let core = core()?;
        let playing = core.now.lock().map_err(|_| anyhow!("poisoned"))?.playing;
        match playing {
            true => core.player.pause(),
            false => core.player.play(),
        }
        Ok(json!({ "playing": !playing }))
    })
}

/// What is playing: `{id, title, artist, artwork, playing, loading, position, duration, error}`.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_now_playing() -> *mut c_char {
    answer(|| {
        let core = core()?;
        let now = core.now.lock().map_err(|_| anyhow!("poisoned"))?.clone();
        Ok(serde_json::to_value(now)?)
    })
}

/// Releases a string any `sonora_*` call answered with.
///
/// # Safety
/// `text` must be null or a pointer one of this library's calls returned, freed only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sonora_free(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Plays a 440 Hz sine for `seconds` on a thread of its own; 0 means the thread started.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_play_tone(seconds: u32) -> i32 {
    let spawned = std::thread::Builder::new()
        .name("sonora-tone".into())
        .spawn(move || {
            if let Err(error) = tone(seconds) {
                log::warn!("mobile: cannot play the tone: {error}");
            }
        });
    match spawned {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Builds the core once. A failed attempt leaves nothing behind, so the next call retries.
fn init(dir: String) -> Result<()> {
    let mut slot = CORE.lock().map_err(|_| anyhow!("poisoned"))?;
    if slot.is_some() {
        return Ok(());
    }
    let _ = env_logger::Builder::new()
        .parse_filters("warn,music=debug,mobile=debug")
        .try_init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("cannot build the runtime")?;
    let provider = SoundCloudProvider::at(PathBuf::from(dir).join("soundcloud"));
    let (_input, input) = tokio::sync::mpsc::unbounded_channel();
    let session = runtime.block_on(provider.sign_in(SignIn::Anonymous, Arc::new(|_| {}), input))?;
    let (player, mut events) = session.playback.start(
        PlaybackConfig {
            normalisation: false,
            gapless: false,
            position_interval: Duration::from_millis(500),
            gain: 1.0,
            equalizer: Equalizer::default(),
        },
        None,
    );
    let now = Arc::new(Mutex::new(Now::default()));
    let followed = now.clone();
    runtime.spawn(async move {
        while let Some(event) = events.next().await {
            let Ok(mut now) = followed.lock() else { break };
            follow(&mut now, event);
        }
        log::warn!("mobile: the player stopped reporting");
    });
    *slot = Some(Arc::new(Core {
        runtime,
        api: session.api,
        player,
        now,
        found: Mutex::new(HashMap::new()),
    }));
    Ok(())
}

/// Folds one engine event into what `sonora_now_playing` answers.
fn follow(now: &mut Now, event: PlaybackEvent) {
    log::debug!("mobile: {event:?}");
    match event {
        PlaybackEvent::Loading { .. } => now.loading = true,
        PlaybackEvent::Playing { at, .. } => {
            now.playing = true;
            now.loading = false;
            now.position = at.as_secs_f64();
        }
        PlaybackEvent::Paused { at, .. } => {
            now.playing = false;
            now.position = at.as_secs_f64();
        }
        PlaybackEvent::Position { at, .. } | PlaybackEvent::Seeked { at, .. } => {
            now.position = at.as_secs_f64();
        }
        PlaybackEvent::Length { duration, .. } => now.duration = duration.as_secs_f64(),
        PlaybackEvent::Ended { .. } => now.playing = false,
        PlaybackEvent::Unavailable { .. } => {
            now.playing = false;
            now.loading = false;
            now.error = Some("this track cannot be played".into());
        }
        _ => {}
    }
}

fn core() -> Result<Arc<Core>> {
    CORE.lock()
        .map_err(|_| anyhow!("poisoned"))?
        .clone()
        .context("sonora_init has not succeeded yet")
}

/// Reads a C string the caller owns.
///
/// # Safety
/// `raw` must be null or a valid NUL-terminated string.
unsafe fn text(raw: *const c_char) -> Result<String> {
    if raw.is_null() {
        anyhow::bail!("a null string was passed");
    }
    Ok(unsafe { CStr::from_ptr(raw) }.to_str()?.to_owned())
}

/// Runs `work` with panics caught and turns its outcome into an owned JSON string.
fn answer(work: impl FnOnce() -> Result<serde_json::Value>) -> *mut c_char {
    let value = match catch_unwind(AssertUnwindSafe(work)) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => json!({ "error": format!("{error:#}") }),
        Err(_) => json!({ "error": "the core panicked" }),
    };
    CString::new(value.to_string())
        .unwrap_or_default()
        .into_raw()
}

/// Opens the default output and writes the sine until `seconds` have passed.
fn tone(seconds: u32) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate as f32;
    let channels = config.channels as usize;
    let mut phase = 0.0_f32;
    let mut next = move || {
        let value = (phase * std::f32::consts::TAU).sin() * VOLUME;
        phase = (phase + FREQUENCY / rate) % 1.0;
        value
    };
    let error = |e| log::warn!("mobile: stream error: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                for frame in data.chunks_mut(channels) {
                    let value = next();
                    frame.fill(value);
                }
            },
            error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                for frame in data.chunks_mut(channels) {
                    let value = (next() * i16::MAX as f32) as i16;
                    frame.fill(value);
                }
            },
            error,
            None,
        ),
        other => return Err(format!("unsupported sample format {other}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs(seconds as u64));
    Ok(())
}
