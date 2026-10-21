use gpui::prelude::*;
use gpui::{
    App, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Render, Subscription, Window, WindowControlArea, div,
};
use i18n::t;
use state::Sonora;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ui::WindowFrame;
use ui::{ActiveTheme as _, Button};

use crate::Root;
use crate::shared::canvas::CanvasArt;
use crate::shared::effects;

/// The picture-in-picture window: the playing music video filling a floating window, with one
/// button back to the main window. The window itself, and where the video plays, belong to
/// `sonora::pip`; closing this window is how it hands the video back.
pub struct PipView {
    art: Entity<CanvasArt>,
    /// Whether a press on the window may still turn into a drag; see `MiniPlayer`.
    grabbed: bool,
    _bounds: Subscription,
}

impl PipView {
    /// Follows the window's size into `AppSettings::set_pip_size`, so the next PiP opens as
    /// large as this one was left. A maximized or fullscreen window is not a size to remember.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let art = cx.new(CanvasArt::video);
        let bounds = cx.observe_window_bounds(window, |_, window, cx| {
            if window.is_maximized() || window.is_fullscreen() {
                return;
            }
            let size = window.bounds().size;
            let size = (f32::from(size.width), f32::from(size.height));
            let settings = Sonora::global(cx).settings.clone();
            settings.update(cx, |settings, cx| settings.set_pip_size(size, cx));
        });

        Self {
            art,
            grabbed: false,
            _bounds: bounds,
        }
    }
}

impl Render for PipView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        self.art.update(cx, |art, _| art.fill(Pixels::ZERO));

        let root = div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme.background)
            .text_color(theme.foreground)
            .window_control_area(WindowControlArea::Drag)
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
            .child(
                div()
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
                    ),
            );
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
