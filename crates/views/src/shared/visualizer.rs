use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, EntityId, Window};
use music::Spectrum;
use ui::Levels;

/// How long the drawn levels take to close 63% of the gap to the analyzer's, until a caller
/// says otherwise. A time constant rather than a share per frame, so the motion is the same on
/// a 60 Hz and a 144 Hz display and a frame that came late moves further instead of falling
/// behind.
const SETTLE: Duration = Duration::from_millis(40);
/// How long a peak mark stays put before it starts to fall, and how fast it falls then, in
/// shares of the full height per second.
const HOLD: f32 = 0.35;
const FALL: f32 = 1.1;
/// The longest gap between two frames that still counts as one step: past it the levels adopt
/// the target outright, so coming back from a stalled window snaps rather than swoops.
const STALL: Duration = Duration::from_millis(250);

struct State {
    shown: Levels,
    armed: bool,
    visible: bool,
    beat: Option<Instant>,
    settle: Duration,
    /// Seconds each band's peak has left to hold, per channel.
    hold_left: Vec<f32>,
    hold_right: Vec<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            shown: Levels::default(),
            armed: false,
            visible: false,
            beat: None,
            settle: SETTLE,
            hold_left: Vec::new(),
            hold_right: Vec::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct VisualizerDrive {
    state: Rc<RefCell<State>>,
}

impl VisualizerDrive {
    pub fn levels(&self) -> Levels {
        self.state.borrow().shown.clone()
    }

    /// Keeps the drawn levels following `spectrum`, closing on it over `settle`.
    pub fn show(&self, watch: EntityId, spectrum: Spectrum, settle: Duration, window: &mut Window) {
        let mut state = self.state.borrow_mut();
        state.visible = true;
        state.settle = settle.max(Duration::from_millis(1));
        if state.armed {
            return;
        }
        state.armed = true;
        state.beat = None;
        drop(state);

        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }

    pub fn hide(&self) {
        self.state.borrow_mut().visible = false;
    }

    fn step(&self, watch: EntityId, spectrum: Spectrum, window: &mut Window, cx: &mut App) {
        {
            let mut state = self.state.borrow_mut();
            if !state.visible {
                state.armed = false;
                return;
            }

            let now = Instant::now();
            let settle = state.settle.as_secs_f32();
            let (rate, elapsed) = match state.beat.replace(now) {
                Some(beat) if now.duration_since(beat) < STALL => {
                    let elapsed = now.duration_since(beat).as_secs_f32();
                    (1. - (-elapsed / settle).exp(), elapsed)
                }
                _ => (1., 0.),
            };
            let State {
                shown,
                hold_left,
                hold_right,
                ..
            } = &mut *state;
            ease(&mut shown.left, spectrum.left(), rate);
            ease(&mut shown.right, spectrum.right(), rate);
            peak(&mut shown.peak_left, hold_left, &shown.left, elapsed);
            peak(&mut shown.peak_right, hold_right, &shown.right, elapsed);
        }

        cx.notify(watch);
        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }
}

/// Keeps each band's peak: it jumps to any level above it, holds there for `HOLD`, then falls
/// at `FALL` until the band catches it again.
fn peak(peaks: &mut Vec<f32>, holds: &mut Vec<f32>, levels: &[f32], elapsed: f32) {
    if peaks.len() != levels.len() {
        *peaks = levels.to_vec();
        *holds = vec![HOLD; levels.len()];
        return;
    }
    for ((peak, hold), level) in peaks.iter_mut().zip(holds.iter_mut()).zip(levels) {
        match *level >= *peak {
            true => {
                *peak = *level;
                *hold = HOLD;
            }
            false if *hold > 0. => *hold -= elapsed,
            false => *peak = (*peak - FALL * elapsed).max(*level),
        }
    }
}

/// Walks `shown` a share of the way towards `target`, adopting it outright when the band
/// count changes.
fn ease(shown: &mut Vec<f32>, target: Vec<f32>, rate: f32) {
    if shown.len() != target.len() {
        *shown = target;
        return;
    }
    for (shown, target) in shown.iter_mut().zip(&target) {
        *shown += (target - *shown) * rate;
    }
}
