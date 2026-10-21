use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnimationExt as _, App, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, Size, Subscription, Task, Window, WindowControlArea, div, px,
    size,
};
use i18n::t;
use state::Sonora;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ui::WindowFrame;
use ui::motion::Motion;
use ui::{ActiveTheme as _, Button};

use crate::Root;
use crate::shared::canvas::CanvasArt;
use crate::shared::effects;

/// How long after the window's size last changed it is snapped to the video's shape.
const SETTLE: Duration = Duration::from_millis(200);
/// How long a change of size after a snap counts as the window manager answering it, and is not
/// snapped again.
const ECHO: Duration = Duration::from_millis(400);
/// How many `SETTLE`s a snap waits for a new movie's first frame before taking `WIDE_SHAPE`.
const TRIES: u32 = 10;
/// The video's shape before its first frame says otherwise, as a width and height.
const WIDE_SHAPE: (u32, u32) = (16, 9);
/// How far the window may be from the video's shape and still count as snapped.
const SNUG: Pixels = px(1.);

/// Told the video's shape, as a width and height, whenever the window takes a new one.
type OnShape = Box<dyn Fn(&mut Window, u32, u32)>;

/// Which side of the window a snap keeps: the one the last resize moved most.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lead {
    Width,
    Height,
}

/// The picture-in-picture window: the playing music video filling a floating window, with one
/// button back to the main window that shows while the pointer is over it. The window itself,
/// and where the video plays, belong to `sonora::pip`; closing this window is how it hands the
/// video back.
pub struct PipView {
    art: Entity<CanvasArt>,
    /// Whether a press on the window may still turn into a drag; see `MiniPlayer`.
    grabbed: bool,
    /// The smallest and largest size a snap may give the window.
    min: Size<Pixels>,
    max: Size<Pixels>,
    /// The window's size after the last snap, which tells the side a resize since moved most.
    base: Size<Pixels>,
    /// When this view last resized the window itself.
    snapped: Option<Instant>,
    snap: Option<Task<()>>,
    /// The shape last handed to `on_shape`, so it hears of each change once.
    shape: Option<(u32, u32)>,
    on_shape: Option<OnShape>,
    _bounds: Subscription,
    _video: Subscription,
}

impl PipView {
    /// Follows the window's size into `AppSettings::set_pip_size`, so the next PiP opens as
    /// large as this one was left, and snaps the window to the video's shape once a resize
    /// settles, never below `min` or above `max`. A maximized or fullscreen window is neither
    /// remembered nor snapped.
    pub fn new(
        min: Size<Pixels>,
        max: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let art = cx.new(|cx| {
            let mut art = CanvasArt::video(cx);
            art.fill(Pixels::ZERO);
            // the window only matches the video's shape once a resize settles
            art.cover();
            art
        });
        let bounds = cx.observe_window_bounds(window, |this, window, cx| this.resized(window, cx));
        // a new movie may have another shape
        let video = Sonora::global(cx).video.clone();
        let video = cx.observe_in(&video, window, |this, _, window, cx| {
            this.schedule(window, cx)
        });

        Self {
            art,
            grabbed: false,
            min,
            max,
            base: window.viewport_size(),
            snapped: None,
            snap: None,
            shape: None,
            on_shape: None,
            _bounds: bounds,
            _video: video,
        }
    }

    /// Calls `f` with the video's shape when the window opens and whenever it changes, so the
    /// window manager can hold that shape during a resize; the snap stays the fallback.
    pub fn on_shape(mut self, f: impl Fn(&mut Window, u32, u32) + 'static) -> Self {
        self.on_shape = Some(Box::new(f));
        self
    }

    /// Hands the current video's shape to `on_shape`, `WIDE_SHAPE` while it has no frame yet, unless
    /// that shape was handed over already.
    pub fn lock_shape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let video = Sonora::global(cx).video.read(cx);
        let shape = match video.movie().and_then(|movie| movie.frame()) {
            Some(frame) if frame.width > 0 && frame.height > 0 => (frame.width, frame.height),
            _ => WIDE_SHAPE,
        };
        self.handover(shape, window);
    }

    /// Passes `shape` to `on_shape` if it differs from the last one passed.
    fn handover(&mut self, shape: (u32, u32), window: &mut Window) {
        if self.shape == Some(shape) {
            return;
        }
        self.shape = Some(shape);
        if let Some(on_shape) = &self.on_shape {
            on_shape(window, shape.0, shape.1);
        }
    }

    /// Remembers the window's new size, and snaps it once the resize settles unless the change
    /// is the answer to this view's own snap.
    fn resized(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_maximized() || window.is_fullscreen() {
            return;
        }
        let size = window.bounds().size;
        let size = (f32::from(size.width), f32::from(size.height));
        let settings = Sonora::global(cx).settings.clone();
        settings.update(cx, |settings, cx| settings.set_pip_size(size, cx));
        if self.snapped.is_some_and(|at| at.elapsed() < ECHO) {
            return;
        }
        self.schedule(window, cx);
    }

    /// Snaps the window `SETTLE` from now, replacing a snap already waiting. A movie with no
    /// frame yet is waited for, a `SETTLE` at a time, up to `TRIES` times.
    fn schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.snap = Some(cx.spawn_in(window, async move |this, cx| {
            for left in (0..TRIES).rev() {
                cx.background_executor().timer(SETTLE).await;
                let waiting = this
                    .update_in(cx, |this, window, cx| this.fit(left == 0, window, cx))
                    .unwrap_or(false);
                if !waiting {
                    break;
                }
            }
        }));
    }

    /// Resizes the window so its content has the video's shape, keeping the side the user
    /// moved most. Answers `true` while a movie has no frame to take the shape from, unless
    /// this is the `last` try, which takes `WIDE_SHAPE`.
    fn fit(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if window.is_maximized() || window.is_fullscreen() {
            return false;
        }
        let video = Sonora::global(cx).video.read(cx);
        let frame = video.movie().map(|movie| movie.frame());
        let shape = match frame {
            Some(Some(frame)) if frame.width > 0 && frame.height > 0 => (frame.width, frame.height),
            Some(None) if !last => return true,
            _ => WIDE_SHAPE,
        };
        self.handover(shape, window);
        let aspect = shape.0 as f32 / shape.1 as f32;
        let now = window.viewport_size();
        let target = snapped(
            now,
            lead(self.base, now, aspect),
            aspect,
            self.min,
            self.max,
        );
        self.base = target;
        let off = (now.width - target.width)
            .abs()
            .max((now.height - target.height).abs());
        if off < SNUG {
            return false;
        }
        self.snapped = Some(Instant::now());
        window.resize(target);
        false
    }
}

impl Render for PipView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        // the back button shows only while the pointer is over the window
        let hovered = window.is_window_hovered();
        let animates = ui::motion::animates(cx);

        let back = div()
            .absolute()
            .top(theme.metrics.pad)
            .left(theme.metrics.pad)
            .child(
                Button::new("video-back")
                    .ghost()
                    .when(effects(), Button::frosted)
                    .small()
                    .icon("icons/chevron-left.svg")
                    .label(t!("video-back"))
                    .on_click(|_, window, cx| {
                        window.remove_window();
                        raise_main(cx);
                    }),
            );

        let root = div()
            .id("pip")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme.background)
            .text_color(theme.foreground)
            .window_control_area(WindowControlArea::Drag)
            // crossing the window's edge flips `is_window_hovered`; make sure that repaints
            .on_hover(cx.listener(|_, _: &bool, _, cx| cx.notify()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.grabbed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.grabbed = false),
            )
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, _| this.grabbed = false))
            // as in the mini player: the window manager takes the grab on the first move
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if this.grabbed {
                    this.grabbed = false;
                    window.start_window_move();
                }
            }))
            .child(self.art.clone())
            .when(hovered, |this| match animates {
                true => this.child(back.with_animation(
                    "pip-back",
                    Motion::Quick.animation(),
                    |back, t| back.opacity(t),
                )),
                false => this.child(back),
            });
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let root = root.child(WindowFrame::new());
        root
    }
}

/// Brings the main window forward, if one is open, as the back button promises.
fn raise_main(cx: &mut App) {
    let main = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<Root>());
    if let Some(main) = main {
        main.update(cx, |_, window, _| window.activate_window())
            .ok();
    }
}

/// Which side a resize from `base` to `now` moved most, a change of height weighed as the
/// width it would make at `aspect`.
fn lead(base: Size<Pixels>, now: Size<Pixels>, aspect: f32) -> Lead {
    let wide = f32::from((now.width - base.width).abs());
    let tall = f32::from((now.height - base.height).abs()) * aspect;
    match tall > wide {
        true => Lead::Height,
        false => Lead::Width,
    }
}

/// The size nearest `now` whose shape is `aspect` (width over height), keeping the `lead`
/// side, then kept between `min` and `max` without losing the shape. A shape that cannot fit
/// those limits at all is clamped to them. Whole pixels.
fn snapped(
    now: Size<Pixels>,
    lead: Lead,
    aspect: f32,
    min: Size<Pixels>,
    max: Size<Pixels>,
) -> Size<Pixels> {
    let [min_w, min_h, max_w, max_h] =
        [min.width, min.height, max.width, max.height].map(f32::from);
    let (mut width, mut height) = match lead {
        Lead::Width => (f32::from(now.width), f32::from(now.width) / aspect),
        Lead::Height => (f32::from(now.height) * aspect, f32::from(now.height)),
    };
    if width > max_w {
        (width, height) = (max_w, max_w / aspect);
    }
    if height > max_h {
        (width, height) = (max_h * aspect, max_h);
    }
    if width < min_w {
        (width, height) = (min_w, min_w / aspect);
    }
    if height < min_h {
        (width, height) = (min_h * aspect, min_h);
    }
    size(
        px(width.clamp(min_w, max_w.max(min_w)).round()),
        px(height.clamp(min_h, max_h.max(min_h)).round()),
    )
}

#[cfg(test)]
mod tests {
    use gpui::{px, size};

    use super::*;

    const MIN: Size<Pixels> = size(px(160.), px(90.));
    const SCREEN: Size<Pixels> = size(px(1920.), px(1080.));

    #[test]
    fn a_resized_pip_takes_the_shape_of_its_video() {
        let wide = 16. / 9.;
        assert_eq!(
            snapped(size(px(640.), px(400.)), Lead::Width, wide, MIN, SCREEN),
            size(px(640.), px(360.))
        );
        assert_eq!(
            snapped(size(px(500.), px(360.)), Lead::Height, wide, MIN, SCREEN),
            size(px(640.), px(360.))
        );
        assert_eq!(
            snapped(size(px(400.), px(400.)), Lead::Width, 4. / 3., MIN, SCREEN),
            size(px(400.), px(300.))
        );
    }

    #[test]
    fn a_snapped_pip_stays_within_its_limits() {
        let wide = 16. / 9.;
        // too small: grows to the smallest size of the same shape
        assert_eq!(
            snapped(size(px(100.), px(90.)), Lead::Width, wide, MIN, SCREEN),
            size(px(160.), px(90.))
        );
        // too tall for the screen: shrinks until it fits
        assert_eq!(
            snapped(size(px(400.), px(1200.)), Lead::Height, wide, MIN, SCREEN),
            size(px(1920.), px(1080.))
        );
        // a portrait video on a short screen keeps its shape below the screen's height
        assert_eq!(
            snapped(size(px(600.), px(400.)), Lead::Width, 9. / 16., MIN, SCREEN),
            size(px(600.), px(1067.))
        );
        assert_eq!(
            snapped(size(px(700.), px(400.)), Lead::Width, 9. / 16., MIN, SCREEN),
            size(px(608.), px(1080.))
        );
    }

    #[test]
    fn a_snap_keeps_the_side_moved_most() {
        let base = size(px(480.), px(270.));
        let wide = 16. / 9.;
        assert_eq!(lead(base, size(px(600.), px(280.)), wide), Lead::Width);
        assert_eq!(lead(base, size(px(490.), px(340.)), wide), Lead::Height);
    }
}
