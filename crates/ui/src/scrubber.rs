use crate::metrics::Text;
use crate::theme::ActiveTheme as _;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Div, DragMoveEvent, ElementId, Empty, Hsla, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, Render, SharedString, Stateful, StyleRefinement, Window, canvas,
    div, fill, point, px, relative, size,
};

const TRACK: f32 = 0.5;
const THUMB: f32 = 1.5;
const HIT: f32 = 3.;
const BUBBLE: f32 = 7.;
/// A waveform bar's width and the space after it. They stay put at any font size, as the
/// bars of a waveform are a texture rather than a control.
const BAR: Pixels = px(2.);
const GAP: Pixels = px(1.);
/// The quietest bar is still drawn this tall, so a silent stretch reads as part of the track.
const FLOOR: Pixels = px(1.);
/// The playhead line drawn over a waveform.
const PLAYHEAD: Pixels = px(1.);
/// How much of a waveform's height the reflection under the baseline takes, and how strongly
/// it is drawn against the bars above it.
const REFLECTION: f32 = 0.3;
const REFLECTED: f32 = 0.35;
/// How strongly the bars between the playhead and the pointer are lit, against played ones.
const PREVIEWED: f32 = 0.55;

type Slide = Box<dyn Fn(&f32, &mut Window, &mut App) + 'static>;
type Release = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>;

fn track(pad: Pixels) -> Pixels {
    px((pad / px(1.) * TRACK).round())
}

fn thumb(pad: Pixels) -> Pixels {
    px((pad / px(1.) * THUMB).round())
}

fn hit(pad: Pixels) -> Pixels {
    px((pad / px(1.) * HIT).round())
}

/// Where a waveform bar stands against the playhead and the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Played,
    Previewed,
    Ahead,
}

/// How many bars, each `BAR` wide with a `GAP` between, fit in `width`.
fn bar_count(width: Pixels) -> usize {
    ((width + GAP) / (BAR + GAP)).floor().max(0.) as usize
}

/// Resamples `levels` to `count` values. Shrinking keeps the loudest of each span, so a short
/// peak never falls between two bars; growing repeats the nearest one.
fn pooled(levels: &[f32], count: usize) -> Vec<f32> {
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

/// Which part of the track a bar centred at `share` of the width belongs to. `hover` is where
/// the pointer is; only the stretch ahead of the playhead up to it is previewed.
fn part(share: f32, fraction: f32, hover: Option<f32>) -> Part {
    if share <= fraction {
        return Part::Played;
    }
    match hover.is_some_and(|at| share <= at) {
        true => Part::Previewed,
        false => Part::Ahead,
    }
}

/// The colours a waveform is painted in.
#[derive(Clone, Copy)]
struct Palette {
    filled: Hsla,
    empty: Hsla,
    thumb: Hsla,
}

/// What one waveform paint needs besides its bounds.
struct Drawn {
    levels: Arc<[f32]>,
    fraction: f32,
    hover: Option<f32>,
    playhead: bool,
    palette: Palette,
}

/// Paints the levels as bars across `bounds`: up from a baseline, with a dimmer reflection
/// under it, lit up to the fraction and previewed up to the pointer, then the playhead over
/// them. One pass of quads, snapped to the device pixel grid so the bars stay crisp.
fn paint_waveform(bounds: Bounds<Pixels>, drawn: &Drawn, window: &mut Window) {
    let scale = window.scale_factor();
    let snap = |value: Pixels| px((value / px(1.) * scale).round() / scale);
    let width = bounds.size.width;
    let count = bar_count(width);
    if count == 0 {
        return;
    }
    let bars = pooled(&drawn.levels, count);
    let pitch = width / count as f32;
    let base = snap(bounds.size.height * (1. - REFLECTION));
    let below = bounds.size.height - base;
    let palette = drawn.palette;

    for (index, level) in bars.iter().enumerate() {
        let left = pitch * index as f32;
        let share = (left + BAR / 2.) / width;
        let color = match part(share, drawn.fraction, drawn.hover) {
            Part::Played => palette.filled,
            Part::Previewed => palette.filled.opacity(PREVIEWED),
            Part::Ahead => palette.empty,
        };
        let x = bounds.origin.x + snap(left);
        let tall = snap((base * *level).max(FLOOR));
        window.paint_quad(fill(
            Bounds::new(point(x, bounds.origin.y + base - tall), size(BAR, tall)),
            color,
        ));
        let reflected = snap(below * *level) - GAP;
        if reflected > Pixels::ZERO {
            window.paint_quad(fill(
                Bounds::new(point(x, bounds.origin.y + base + GAP), size(BAR, reflected)),
                color.opacity(REFLECTED),
            ));
        }
    }

    if drawn.playhead {
        let x = bounds.origin.x + snap(width * drawn.fraction - PLAYHEAD / 2.);
        window.paint_quad(fill(
            Bounds::new(
                point(x, bounds.origin.y),
                size(PLAYHEAD, bounds.size.height),
            ),
            palette.thumb,
        ));
    }
}

#[derive(Clone)]
struct Grab(SharedString);

impl Render for Grab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub struct ScrubberState {
    id: SharedString,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl ScrubberState {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    pub fn hovered(&self, position: Point<Pixels>, pad: Pixels) -> Option<f32> {
        self.bounds
            .get()
            .contains(&position)
            .then(|| self.fraction_at(position.x, pad))
    }

    fn fraction_at(&self, x: Pixels, pad: Pixels) -> f32 {
        let bounds = self.bounds.get();
        let pin = thumb(pad);
        let travel = bounds.size.width - pin;
        if travel <= px(0.) {
            return 0.;
        }
        ((x - bounds.origin.x - pin / 2.) / travel).clamp(0., 1.)
    }

    fn climb_at(&self, y: Pixels, pad: Pixels) -> f32 {
        let bounds = self.bounds.get();
        let pin = thumb(pad);
        let travel = bounds.size.height - pin;
        if travel <= px(0.) {
            return 0.;
        }
        1. - ((y - bounds.origin.y - pin / 2.) / travel).clamp(0., 1.)
    }
}

#[derive(IntoElement)]
pub struct Scrubber {
    base: Stateful<Div>,
    id: SharedString,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    fraction: f32,
    filled: Hsla,
    empty: Hsla,
    thumb: Hsla,
    enabled: bool,
    vertical: bool,
    bubble: Option<(f32, SharedString)>,
    lift: Pixels,
    waveform: Option<Arc<[f32]>>,
    on_move: Option<Slide>,
    on_release: Option<Release>,
}

impl Scrubber {
    pub fn new(state: &ScrubberState, fraction: f32) -> Self {
        Self {
            base: div().id(ElementId::Name(state.id.clone())),
            id: state.id.clone(),
            bounds: state.bounds.clone(),
            fraction: fraction.clamp(0., 1.),
            filled: gpui::white(),
            empty: gpui::black(),
            thumb: gpui::white(),
            enabled: true,
            vertical: false,
            bubble: None,
            lift: px(12.),
            waveform: None,
            on_move: None,
            on_release: None,
        }
    }

    pub fn bubble(mut self, fraction: f32, text: impl Into<SharedString>) -> Self {
        self.bubble = Some((fraction.clamp(0., 1.), text.into()));
        self
    }

    /// Draws the track as bars of `levels` (each 0 to 1, first to last) in place of the line,
    /// lit up to the fraction, with a thin playhead in the thumb colour. Horizontal only; the
    /// bars fill the scrubber's height, so a caller wanting them taller sets `h`.
    pub fn waveform(mut self, levels: Arc<[f32]>) -> Self {
        self.waveform = Some(levels).filter(|levels| !levels.is_empty());
        self
    }

    pub fn lift(mut self, lift: Pixels) -> Self {
        self.lift = lift;
        self
    }

    pub fn colors(mut self, filled: Hsla, empty: Hsla, thumb: Hsla) -> Self {
        self.filled = filled;
        self.empty = empty;
        self.thumb = thumb;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn on_move(mut self, handler: impl Fn(&f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_move = Some(Box::new(handler));
        self
    }

    pub fn on_release(
        mut self,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_release = Some(Box::new(handler));
        self
    }
}

impl Styled for Scrubber {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Scrubber {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pad = cx.theme().metrics.pad;
        let line = track(pad);
        let pin = thumb(pad);
        let reach = hit(pad);
        let bubble_width = px((pad / px(1.) * BUBBLE).round());
        let popover = cx.theme().popover;
        let popover_border = cx.theme().border;
        let popover_text = cx.theme().popover_foreground;
        let text_size = cx.theme().text(Text::Tiny);

        let Self {
            mut base,
            id,
            bounds,
            fraction,
            filled,
            empty,
            thumb,
            enabled,
            vertical,
            bubble,
            lift,
            waveform,
            on_move,
            on_release,
        } = self;
        let overrides = std::mem::take(base.style());

        let state = Rc::new(ScrubberState {
            id: id.clone(),
            bounds: bounds.clone(),
        });
        let on_move = on_move.map(Rc::new);
        let on_release = on_release.map(Rc::new);

        let at = move |state: &ScrubberState, position: Point<Pixels>| match vertical {
            true => state.climb_at(position.y, pad),
            false => state.fraction_at(position.x, pad),
        };

        let down = {
            let state = state.clone();
            let on_move = on_move.clone();
            move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                // the grab belongs to the scrubber alone: a window drag region
                // underneath must not read it as a press on the window
                cx.stop_propagation();
                if let Some(handler) = on_move.as_ref() {
                    handler(&at(&state, event.position), window, cx);
                }
            }
        };

        let dragged = {
            let state = state.clone();
            let on_move = on_move.clone();
            let mine = id.clone();
            move |event: &DragMoveEvent<Grab>, window: &mut Window, cx: &mut App| {
                if event.drag(cx).0 != mine {
                    return;
                }
                if let Some(handler) = on_move.as_ref() {
                    handler(&at(&state, event.event.position), window, cx);
                }
            }
        };

        let released = move |event: &MouseUpEvent, window: &mut Window, cx: &mut App| {
            if let Some(handler) = on_release.as_ref() {
                handler(event, window, cx);
            }
        };

        let inset = match enabled {
            true => pin / 2.,
            false => Pixels::ZERO,
        };

        let hover = bubble.as_ref().map(|(at, _)| *at);
        let bar = match (vertical, waveform) {
            (false, Some(levels)) => {
                let drawn = Drawn {
                    levels,
                    fraction,
                    hover,
                    playhead: enabled,
                    palette: Palette {
                        filled,
                        empty,
                        thumb,
                    },
                };
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(pin / 2.)
                    .right(pin / 2.)
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| paint_waveform(bounds, &drawn, window),
                        )
                        .size_full(),
                    )
                    .when_some(bubble, |this, (at, text)| {
                        this.child(
                            div()
                                .absolute()
                                .bottom(relative(1.))
                                .left(relative(at))
                                .ml(Pixels::ZERO - bubble_width / 2.)
                                .w(bubble_width)
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .px_1p5()
                                        .rounded_md()
                                        .bg(popover)
                                        .border_1()
                                        .border_color(popover_border)
                                        .text_color(popover_text)
                                        .text_size(text_size)
                                        .child(text),
                                ),
                        )
                    })
            }
            (true, _) => div()
                .relative()
                .h_full()
                .w(line)
                .rounded_full()
                .bg(empty)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(inset)
                        .bottom(inset)
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom(Pixels::ZERO - inset)
                                .top(relative(1. - fraction))
                                .rounded_full()
                                .bg(filled),
                        )
                        .when(enabled, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left((line - pin) / 2.)
                                    .bottom(relative(fraction))
                                    .mb(Pixels::ZERO - pin / 2.)
                                    .size(pin)
                                    .rounded_full()
                                    .bg(thumb),
                            )
                        })
                        .when_some(bubble, |this, (at, text)| {
                            this.child(
                                div()
                                    .absolute()
                                    .map(|this| {
                                        if lift >= Pixels::ZERO {
                                            this.right(line + lift)
                                        } else {
                                            this.left(line - lift)
                                        }
                                    })
                                    .bottom(relative(at))
                                    .mb(Pixels::ZERO - pin / 2.)
                                    .h(pin)
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .px_1p5()
                                            .rounded_md()
                                            .bg(popover)
                                            .border_1()
                                            .border_color(popover_border)
                                            .text_color(popover_text)
                                            .text_size(text_size)
                                            .whitespace_nowrap()
                                            .child(text),
                                    ),
                            )
                        }),
                ),
            (false, None) => div()
                .relative()
                .w_full()
                .h(line)
                .rounded_full()
                .bg(empty)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(inset)
                        .right(inset)
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(Pixels::ZERO - inset)
                                .right(relative(1. - fraction))
                                .rounded_full()
                                .bg(filled),
                        )
                        .when(enabled, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top((line - pin) / 2.)
                                    .left(relative(fraction))
                                    .ml(Pixels::ZERO - pin / 2.)
                                    .size(pin)
                                    .rounded_full()
                                    .bg(thumb),
                            )
                        })
                        .when_some(bubble, |this, (at, text)| {
                            this.child(
                                div()
                                    .absolute()
                                    .map(|this| {
                                        if lift >= Pixels::ZERO {
                                            this.bottom(lift)
                                        } else {
                                            this.top(Pixels::ZERO - lift)
                                        }
                                    })
                                    .left(relative(at))
                                    .ml(Pixels::ZERO - bubble_width / 2.)
                                    .w(bubble_width)
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .px_1p5()
                                            .rounded_md()
                                            .bg(popover)
                                            .border_1()
                                            .border_color(popover_border)
                                            .text_color(popover_text)
                                            .text_size(text_size)
                                            .child(text),
                                    ),
                            )
                        }),
                ),
        };

        let mut scrubber = base
            .relative()
            .flex()
            .when_else(
                vertical,
                |this| this.justify_center().h_full().w(reach),
                |this| this.items_center().w_full().h(reach),
            )
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .when(enabled, |this| {
                this.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, down)
                    .on_drag(Grab(id.clone()), |grab, _, _, cx| cx.new(|_| grab.clone()))
                    .on_drag_move(dragged)
                    .on_mouse_up(MouseButton::Left, released.clone())
                    .on_mouse_up_out(MouseButton::Left, released)
            })
            .child(bar);
        scrubber.style().refine(&overrides);
        scrubber
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::{Part, ScrubberState, bar_count, hit, part, pooled, thumb, track};

    #[test]
    fn bars_fill_the_width_at_their_pitch() {
        assert_eq!(bar_count(px(300.)), 100);
        assert_eq!(bar_count(px(301.)), 100);
        assert_eq!(bar_count(px(2.)), 1);
        assert_eq!(bar_count(px(0.)), 0);
    }

    #[test]
    fn pooling_keeps_the_loudest_of_each_span() {
        assert_eq!(
            pooled(&[0.1, 0.9, 0.2, 0.3, 0.8, 0.1], 3),
            vec![0.9, 0.3, 0.8]
        );
        assert_eq!(pooled(&[0.2, 0.6], 4), vec![0.2, 0.2, 0.6, 0.6]);
        assert!(pooled(&[], 4).is_empty());
    }

    #[test]
    fn a_bar_is_played_previewed_or_ahead() {
        assert_eq!(part(0.2, 0.5, None), Part::Played);
        assert_eq!(part(0.6, 0.5, None), Part::Ahead);
        assert_eq!(part(0.6, 0.5, Some(0.7)), Part::Previewed);
        assert_eq!(part(0.8, 0.5, Some(0.7)), Part::Ahead);
        // pointing behind the playhead previews nothing
        assert_eq!(part(0.3, 0.5, Some(0.2)), Part::Played);
    }

    const PAD: gpui::Pixels = px(8.);
    const ORIGIN: f32 = 20.;
    const WIDTH: f32 = 300.;

    fn measured() -> ScrubberState {
        let state = ScrubberState::new("test");
        state.bounds.set(Bounds {
            origin: point(px(ORIGIN), px(0.)),
            size: size(px(WIDTH), hit(PAD)),
        });
        state
    }

    #[test]
    fn the_thumb_lands_under_the_pointer() {
        let state = measured();
        let pin = thumb(PAD);
        let travel = px(WIDTH) - pin;

        for step in 0..=100 {
            let fraction = step as f32 / 100.;
            let center = px(ORIGIN) + pin / 2. + travel * fraction;
            let read = state.fraction_at(center, PAD);
            assert!((read - fraction).abs() < 1e-4, "{fraction} read as {read}");
        }
    }

    #[test]
    fn the_ends_clamp() {
        let state = measured();

        assert_eq!(state.fraction_at(px(0.), PAD), 0.);
        assert_eq!(state.fraction_at(px(ORIGIN), PAD), 0.);
        assert_eq!(state.fraction_at(px(ORIGIN + WIDTH), PAD), 1.);
        assert_eq!(state.fraction_at(px(9999.), PAD), 1.);
    }

    #[test]
    fn an_unmeasured_track_reads_zero() {
        let state = ScrubberState::new("test");

        assert_eq!(state.fraction_at(px(50.), PAD), 0.);
    }

    #[test]
    fn the_bubble_shows_exactly_where_the_grab_lands() {
        let state = measured();
        let reach = hit(PAD);
        let x = px(ORIGIN + WIDTH / 2.);

        assert!(reach > track(PAD) * 4.);
        assert!(state.hovered(point(x, px(0.)), PAD).is_some());
        assert!(state.hovered(point(x, reach - px(1.)), PAD).is_some());
        assert!(state.hovered(point(x, px(-1.)), PAD).is_none());
        assert!(state.hovered(point(x, reach), PAD).is_none());
    }
}
