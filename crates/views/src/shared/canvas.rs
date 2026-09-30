//! Draws the playing track's Canvas. Each frame it asks the player for its newest picture and
//! swaps the image only when that picture is new, dropping the old one from the atlas, so a
//! 25 fps clip uploads 25 textures a second and never piles them up.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{App, Context, ObjectFit, Pixels, RenderImage, Window, img, px};
use image::{Frame, RgbaImage};
use state::Sonora;

/// How long the frame loop outlives the last render, so a canvas taken off screen stops asking.
const LINGER: Duration = Duration::from_millis(500);

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
    /// When `pull` last ran, which is when this view was last on screen.
    drawn: Instant,
    /// `Canvas::generation` of the clip `image` came from.
    generation: u64,
}

impl CanvasArt {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let canvas = Sonora::global(cx).canvas.clone();
        cx.observe(&canvas, |_, _, cx| cx.notify()).detach();
        // a resume after a pause has no new frame to wake the loop, so playback does
        let playback = Sonora::global(cx).playback.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        Self {
            image: None,
            index: 0,
            width: px(0.),
            height: px(0.),
            radius: px(0.),
            armed: false,
            drawn: Instant::now(),
            generation: 0,
        }
    }

    /// The box the owner lays the canvas out in; the clip covers it, cropped like a cover.
    pub(crate) fn set_side(&mut self, width: Pixels, height: Pixels, radius: Pixels) {
        (self.width, self.height, self.radius) = (width, height, radius);
    }

    /// Takes the player's newest frame when it is new, and keeps the loop armed while one plays.
    fn pull(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wide =
            (f32::from(self.width.max(self.height * (9. / 16.))) * window.scale_factor()).ceil();
        let entity = Sonora::global(cx).canvas.clone();
        // the read borrow ends here, before the image work needs `cx` mutably
        let (generation, newest) = {
            let canvas = entity.read(cx);
            let Some(player) = canvas.player() else {
                self.release(window, cx);
                return;
            };
            player.watch(wide as u32);
            (canvas.generation(), player.frame())
        };
        // another clip since the last draw, maybe while this view was off screen: its first
        // frame is not here yet, and the one held belongs to the song before
        if generation != self.generation {
            self.release(window, cx);
            self.generation = generation;
        }
        if let Some(frame) = newest
            && frame.index != self.index
            && let Some(pixels) =
                RgbaImage::from_raw(frame.width, frame.height, frame.pixels.clone())
        {
            let fresh = Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
            if let Some(old) = self.image.replace(fresh) {
                cx.drop_image(old, Some(window));
            }
            self.index = frame.index;
        }
        self.drawn = Instant::now();
        self.arm(window, cx);
    }

    /// Looks for a new frame once a display frame, and repaints only when one arrived, so a
    /// 25 fps clip costs 25 repaints a second on a 120 Hz screen. Stops once this view has not
    /// been drawn for `LINGER`.
    fn arm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.armed {
            return;
        }
        self.armed = true;
        let view = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            view.update(cx, |this, cx| {
                this.armed = false;
                if this.drawn.elapsed() > LINGER {
                    return;
                }
                let newest = Sonora::global(cx)
                    .canvas
                    .read(cx)
                    .player()
                    .and_then(|player| player.frame())
                    .map(|frame| frame.index);
                match newest.is_some_and(|index| index != this.index) {
                    true => cx.notify(),
                    false => this.arm(window, cx),
                }
            })
            .ok();
        });
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
