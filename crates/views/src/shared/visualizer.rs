use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, EntityId, Task, Window};
use music::{Listening, Spectrum};
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
/// How often levels at rest are redrawn anyway. A redraw is what tells the drive the view is
/// still on screen, so this bounds how long it keeps listening after the view goes away.
const HEARTBEAT: Duration = Duration::from_millis(500);
/// How close a drawn level has to come to its target to be put on it, so a paused track comes
/// to rest instead of easing towards zero forever.
const REST: f32 = 1e-3;

/// Holds the eased levels and the next timed update without keeping its view alive.
struct State {
    shown: Levels,
    spectrum: Spectrum,
    listening: Option<Listening>,
    armed: bool,
    /// The last render that included this visualizer.
    seen: Option<Instant>,
    visible: bool,
    beat: Option<Instant>,
    settle: Duration,
    /// Seconds each band's peak has left to hold, per channel.
    hold_left: Vec<f32>,
    hold_right: Vec<f32>,
    /// When the view was last asked to redraw.
    drawn: Option<Instant>,
    /// Owns the next update without retaining a hidden or dropped visualizer.
    pacing: Option<Task<()>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            shown: Levels::default(),
            spectrum: Spectrum::default(),
            listening: None,
            armed: false,
            seen: None,
            visible: false,
            beat: None,
            settle: SETTLE,
            hold_left: Vec::new(),
            hold_right: Vec::new(),
            drawn: None,
            pacing: None,
        }
    }
}

/// Eases the analyzer levels at most 120 times a second and redraws only when they move.
/// Its timer keeps the update rate independent of frames that GPUI chooses not to draw.
#[derive(Clone, Default)]
pub struct VisualizerDrive {
    state: Rc<RefCell<State>>,
}

impl VisualizerDrive {
    pub fn levels(&self) -> Levels {
        self.state.borrow().shown.clone()
    }

    /// Marks the visualizer as on screen in `watch`, closing on `spectrum` over `settle`, and
    /// starts stepping it if it is not already. Call it from every render that draws it.
    pub fn show(&self, watch: EntityId, spectrum: Spectrum, settle: Duration, window: &mut Window) {
        let mut state = self.state.borrow_mut();
        state.visible = true;
        state.settle = settle.max(Duration::from_millis(1));
        state.seen = Some(Instant::now());
        state.listening = Some(spectrum.listen());
        state.spectrum = spectrum;
        if state.armed {
            return;
        }
        state.armed = true;
        state.beat = None;
        state.drawn = None;
        drop(state);

        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, window, cx));
    }

    pub fn hide(&self) {
        let mut state = self.state.borrow_mut();
        state.visible = false;
        state.armed = false;
        state.listening = None;
        state.pacing = None;
    }

    /// Samples the analyzer while visible and schedules the next update independently of drawing.
    fn step(&self, watch: EntityId, window: &mut Window, cx: &mut App) {
        let now = Instant::now();
        let redraw = {
            let mut state = self.state.borrow_mut();
            let gone = state
                .seen
                .is_none_or(|seen| now.duration_since(seen) > HEARTBEAT + STALL);
            if !state.visible || gone {
                state.armed = false;
                state.listening = None;
                state.pacing = None;
                return;
            }
            let spectrum = state.spectrum.clone();
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
                drawn,
                ..
            } = &mut *state;
            let left = ease(&mut shown.left, spectrum.left(), rate);
            let right = ease(&mut shown.right, spectrum.right(), rate);
            let peaks_left = peak(&mut shown.peak_left, hold_left, &shown.left, elapsed);
            let peaks_right = peak(&mut shown.peak_right, hold_right, &shown.right, elapsed);
            let redraw = left
                || right
                || peaks_left
                || peaks_right
                || drawn.is_none_or(|drawn| now.duration_since(drawn) >= HEARTBEAT);
            if redraw {
                *drawn = Some(now);
            }
            redraw
        };

        if redraw {
            cx.notify(watch);
            // Arm the platform frame source as well as dirtying the view.
            window.on_next_frame(|_, _| {});
        }
        let weak = Rc::downgrade(&self.state);
        let handle = window.window_handle();
        let timer = super::animation::next(1, cx);
        self.state.borrow_mut().pacing = Some(cx.spawn(async move |cx| {
            timer.await;
            let Some(state) = weak.upgrade() else {
                return;
            };
            handle
                .update(cx, |_, window, cx| Self { state }.step(watch, window, cx))
                .ok();
        }));
    }
}

/// Keeps each band's peak: it jumps to any level above it, holds there for `HOLD`, then falls
/// at `FALL` until the band catches it again. Returns whether any peak moved.
fn peak(peaks: &mut Vec<f32>, holds: &mut Vec<f32>, levels: &[f32], elapsed: f32) -> bool {
    if peaks.len() != levels.len() {
        *peaks = levels.to_vec();
        *holds = vec![HOLD; levels.len()];
        return true;
    }
    let mut moved = false;
    for ((peak, hold), level) in peaks.iter_mut().zip(holds.iter_mut()).zip(levels) {
        let before = *peak;
        match *level >= *peak {
            true => {
                *peak = *level;
                *hold = HOLD;
            }
            false if *hold > 0. => *hold -= elapsed,
            false => *peak = (*peak - FALL * elapsed).max(*level),
        }
        moved |= *peak != before;
    }
    moved
}

/// Walks `shown` a share of the way towards `target`, adopting it outright when the band
/// count changes or a band comes within `REST` of it. Returns whether any level moved.
fn ease(shown: &mut Vec<f32>, target: Vec<f32>, rate: f32) -> bool {
    if shown.len() != target.len() {
        *shown = target;
        return true;
    }
    let mut moved = false;
    for (shown, target) in shown.iter_mut().zip(&target) {
        let next = match (target - *shown).abs() < REST {
            true => *target,
            false => *shown + (target - *shown) * rate,
        };
        moved |= next != *shown;
        *shown = next;
    }
    moved
}
