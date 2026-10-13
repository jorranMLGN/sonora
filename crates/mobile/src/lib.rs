//! A plain C ABI for the iOS spike: a version string and a test tone through cpal.

use std::ffi::c_char;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const VERSION: &str = concat!("sonora-core ", env!("CARGO_PKG_VERSION"), "\0");
const FREQUENCY: f32 = 440.0;
const VOLUME: f32 = 0.2;

/// A static, NUL-terminated version string. The caller must never free it.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

/// Plays a 440 Hz sine for `seconds` on a thread of its own; 0 means the thread started.
#[unsafe(no_mangle)]
pub extern "C" fn sonora_play_tone(seconds: u32) -> i32 {
    let spawned = std::thread::Builder::new()
        .name("sonora-tone".into())
        .spawn(move || {
            if let Err(error) = tone(seconds) {
                eprintln!("mobile: cannot play the tone: {error}");
            }
        });
    match spawned {
        Ok(_) => 0,
        Err(_) => -1,
    }
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
    let error = |e| eprintln!("mobile: stream error: {e}");
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
