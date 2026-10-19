//! A track's level over its length, the shape a seek bar draws as bars.
//!
//! A waveform comes from one of two places. A provider that publishes one answers
//! `MusicApi::waveform`, which is how SoundCloud's arrives. Every other track that plays through
//! `crate::engine` is measured from its own audio once the whole of it has arrived: a second
//! decoder runs over it on a thread of its own and the result lands in the engine's `Analysis`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How many values a waveform keeps. A seek bar draws a few hundred bars at most, and this is
/// enough to pool down to any of them without two bars sharing one value.
pub const BUCKETS: usize = 512;
/// How many windows a second of decoded audio is measured in before it is pooled to `BUCKETS`.
const WINDOWS_PER_SECOND: u32 = 20;
/// How many waveforms an `Analysis` holds, so going back a few tracks does not decode again.
const KEPT: usize = 8;
/// How many samples the measuring loop decodes between looks at whether it is still wanted.
const CHECK_EVERY: usize = 1 << 16;

/// A track's level over its length: `BUCKETS` values or fewer, from 0 to 1, with the loudest
/// at 1. Cloning shares the values.
#[derive(Clone, Debug, PartialEq)]
pub struct Waveform {
    levels: Arc<[f32]>,
}

impl Waveform {
    /// Pools `levels` down to `BUCKETS` and scales them so the loudest is 1. Negative or
    /// non-finite values count as silence. `None` for an empty list.
    pub fn new(levels: &[f32]) -> Option<Self> {
        if levels.is_empty() {
            return None;
        }
        let mut levels = match levels.len() > BUCKETS {
            true => pooled(levels, BUCKETS),
            false => levels.to_vec(),
        };
        for level in &mut levels {
            if !level.is_finite() || *level < 0. {
                *level = 0.;
            }
        }
        let loudest = levels.iter().copied().fold(0., f32::max);
        if loudest > 0. {
            for level in &mut levels {
                *level /= loudest;
            }
        }
        Some(Self {
            levels: levels.into(),
        })
    }

    /// The values, first to last, each from 0 to 1.
    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    /// The values as a shared slice, for a view that hands them to an element every frame.
    pub fn shared(&self) -> Arc<[f32]> {
        self.levels.clone()
    }
}

/// Resamples `levels` to `count` values. Shrinking keeps the loudest value of each span, so a
/// short peak never disappears between two bars; growing repeats the nearest one.
pub(crate) fn pooled(levels: &[f32], count: usize) -> Vec<f32> {
    if levels.is_empty() || count == 0 {
        return Vec::new();
    }
    let len = levels.len();
    (0..count)
        .map(|index| {
            let start = index * len / count;
            let end = ((index + 1) * len / count).max(start + 1).min(len);
            levels[start..end].iter().copied().fold(0., f32::max)
        })
        .collect()
}

/// Measures decoded audio as it is pushed in: the RMS of each short window across every
/// channel, pooled to a `Waveform` at the end.
pub(crate) struct Meter {
    /// Samples per window, all channels counted.
    window: usize,
    sum: f64,
    filled: usize,
    windows: Vec<f32>,
}

impl Meter {
    pub(crate) fn new(channels: u16, rate: u32) -> Self {
        let frames = (rate / WINDOWS_PER_SECOND).max(1) as usize;
        Self {
            window: frames * usize::from(channels.max(1)),
            sum: 0.,
            filled: 0,
            windows: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, sample: f32) {
        let sample = match sample.is_finite() {
            true => f64::from(sample),
            false => 0.,
        };
        self.sum += sample * sample;
        self.filled += 1;
        if self.filled == self.window {
            self.close();
        }
    }

    /// The waveform of everything pushed, a part-filled last window included. `None` when
    /// nothing was.
    pub(crate) fn finish(mut self) -> Option<Waveform> {
        if self.filled > 0 {
            self.close();
        }
        Waveform::new(&self.windows)
    }

    fn close(&mut self) {
        self.windows
            .push((self.sum / self.filled as f64).sqrt() as f32);
        self.sum = 0.;
        self.filled = 0;
    }
}

/// Decodes `source` to its end and measures it. `gone` is asked every so often and stops the
/// work with `None` once it answers true, which is how a skipped track stops being measured.
pub(crate) fn measure<S: rodio::Source>(source: S, gone: impl Fn() -> bool) -> Option<Waveform> {
    let mut meter = Meter::new(source.channels().get(), source.sample_rate().get());
    for (index, sample) in source.enumerate() {
        if index % CHECK_EVERY == 0 && gone() {
            return None;
        }
        meter.push(sample);
    }
    meter.finish()
}

/// Whether an engine may decode a finished track a second time to measure its waveform. Shared
/// rather than copied, like `Equalizer`: every engine holds a clone, so switching it off reaches
/// them all at once, and a track that finishes downloading afterwards is never decoded again.
#[derive(Clone, Debug)]
pub struct Measuring(Arc<AtomicBool>);

impl Measuring {
    pub fn new(on: bool) -> Self {
        Self(Arc::new(AtomicBool::new(on)))
    }

    pub fn on(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, on: bool) {
        self.0.store(on, Ordering::Relaxed);
    }
}

impl Default for Measuring {
    fn default() -> Self {
        Self::new(true)
    }
}

/// The waveforms an engine measured, by the provider's own track id (untagged), newest last.
/// The engine fills it; the state reads it whenever playback moves. Cloning shares it.
#[derive(Clone, Default)]
pub struct Analysis {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    kept: Mutex<VecDeque<(String, Waveform)>>,
    /// Moves on every load, so a measurement started for an earlier track knows to stop.
    epoch: AtomicU64,
}

impl Analysis {
    /// The waveform measured for `id`, if one has been.
    pub fn get(&self, id: &str) -> Option<Waveform> {
        let kept = self.shared.kept.lock().ok()?;
        kept.iter()
            .find(|(kept, _)| kept == id)
            .map(|(_, waveform)| waveform.clone())
    }

    pub(crate) fn has(&self, id: &str) -> bool {
        self.shared
            .kept
            .lock()
            .is_ok_and(|kept| kept.iter().any(|(kept, _)| kept == id))
    }

    /// Files a waveform, dropping the oldest once `KEPT` are held.
    pub(crate) fn keep(&self, id: String, waveform: Waveform) {
        let Ok(mut kept) = self.shared.kept.lock() else {
            return;
        };
        kept.retain(|(kept, _)| *kept != id);
        if kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back((id, waveform));
    }

    /// The current load's number; a measurement compares it with the one it started under.
    pub(crate) fn epoch(&self) -> u64 {
        self.shared.epoch.load(Ordering::Relaxed)
    }

    /// Marks a new load, so any measurement still running for the last one stops.
    pub(crate) fn bump(&self) {
        self.shared.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{Analysis, BUCKETS, Meter, Waveform, pooled};

    #[test]
    fn shrinking_keeps_the_loudest_of_each_span() {
        let levels = [0.1, 0.9, 0.2, 0.3, 0.8, 0.1];

        assert_eq!(pooled(&levels, 3), vec![0.9, 0.3, 0.8]);
    }

    #[test]
    fn a_short_peak_survives_heavy_pooling() {
        let mut levels = vec![0.1; 1000];
        levels[517] = 1.;

        let pooled = pooled(&levels, 7);

        assert_eq!(pooled.len(), 7);
        assert_eq!(pooled.iter().filter(|level| **level == 1.).count(), 1);
    }

    #[test]
    fn growing_repeats_the_nearest_value() {
        assert_eq!(pooled(&[0.2, 0.6], 4), vec![0.2, 0.2, 0.6, 0.6]);
    }

    #[test]
    fn pooling_nothing_gives_nothing() {
        assert!(pooled(&[], 10).is_empty());
        assert!(pooled(&[0.5], 0).is_empty());
    }

    #[test]
    fn a_waveform_is_scaled_so_the_loudest_is_one() {
        let waveform = Waveform::new(&[10., 20., 40.]).unwrap();

        assert_eq!(waveform.levels(), &[0.25, 0.5, 1.]);
    }

    #[test]
    fn a_long_waveform_is_pooled_to_the_bucket_count() {
        let levels: Vec<f32> = (0..BUCKETS * 3).map(|index| index as f32).collect();

        let waveform = Waveform::new(&levels).unwrap();

        assert_eq!(waveform.levels().len(), BUCKETS);
        assert_eq!(waveform.levels().last(), Some(&1.));
    }

    #[test]
    fn silence_and_garbage_read_as_zero() {
        let waveform = Waveform::new(&[0., f32::NAN, -3., 0.]).unwrap();

        assert_eq!(waveform.levels(), &[0., 0., 0., 0.]);
        assert!(Waveform::new(&[]).is_none());
    }

    #[test]
    fn the_meter_measures_each_window_across_channels() {
        // two channels at 40 Hz: a window is two frames, so four samples
        let mut meter = Meter::new(2, 40);
        for sample in [0.5, -0.5, 0.5, -0.5, 0.25, 0.25, -0.25, -0.25] {
            meter.push(sample);
        }

        let waveform = meter.finish().unwrap();

        assert_eq!(waveform.levels(), &[1., 0.5]);
    }

    #[test]
    fn a_part_filled_last_window_still_counts() {
        let mut meter = Meter::new(1, 40);
        for sample in [0.5, 0.5, 1.] {
            meter.push(sample);
        }

        let waveform = meter.finish().unwrap();

        assert_eq!(waveform.levels(), &[0.5, 1.]);
    }

    #[test]
    fn a_meter_fed_nothing_has_no_waveform() {
        assert!(Meter::new(2, 44_100).finish().is_none());
    }

    #[test]
    fn an_analysis_keeps_only_the_newest_few() {
        let analysis = Analysis::default();
        let waveform = Waveform::new(&[1.]).unwrap();
        for index in 0..20 {
            analysis.keep(index.to_string(), waveform.clone());
        }

        assert!(analysis.get("0").is_none());
        assert!(analysis.has("19"));
        assert_eq!(analysis.get("19"), Some(waveform));
    }

    #[test]
    fn keeping_a_track_again_replaces_it() {
        let analysis = Analysis::default();
        analysis.keep("a".into(), Waveform::new(&[1.]).unwrap());
        analysis.keep("a".into(), Waveform::new(&[1., 0.5]).unwrap());

        assert_eq!(analysis.get("a").unwrap().levels(), &[1., 0.5]);
    }
}
