//! Draws the playing track's Canvas, or its music video. Each frame it asks the player for its
//! newest picture and swaps the image only when that picture is new, dropping the old one from
//! the atlas, so a 25 fps clip uploads 25 textures a second and never piles them up.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{App, Context, ObjectFit, Pixels, RenderImage, Window, img, px};
use image::{Frame, RgbaImage};
use music::video::Frame as Picture;
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

/// Whether the current track's music video has a movie to draw. Unlike `canvas_playing` it
/// ignores reduce motion and the effects switch: a video is content the listener asked for,
/// not decoration. Where it shows is up to the caller, which reads `Video::place`.
pub(crate) fn video_showing(cx: &App) -> bool {
    Sonora::global(cx).video.read(cx).movie().is_some()
}

/// Which player a `CanvasArt` draws from.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    /// Spotify's looping Canvas, cropped to fill its box.
    Canvas,
    /// The music video, letterboxed inside its box unless `CanvasArt::cover` crops it.
    Video,
}

pub(crate) struct CanvasArt {
    source: Source,
    image: Option<Arc<RenderImage>>,
    index: u64,
    width: Pixels,
    height: Pixels,
    radius: Pixels,
    /// Whether the clip fills whatever box its parent gives it instead of the `set_side` one.
    /// Such a clip is rounded only at its bottom corners, by `radius`.
    fill: bool,
    /// Whether a video is cropped to cover its box, as a Canvas always is, instead of
    /// letterboxed inside it.
    cover: bool,
    armed: bool,
    /// When `pull` last ran, which is when this view was last on screen.
    drawn: Instant,
    /// `Canvas::generation` or `Video::generation` of the clip `image` came from.
    generation: u64,
}

impl CanvasArt {
    /// Draws the playing track's Canvas.
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let canvas = Sonora::global(cx).canvas.clone();
        cx.observe(&canvas, |_, _, cx| cx.notify()).detach();
        Self::with(Source::Canvas, cx)
    }

    /// Draws the playing track's music video, contained in its box rather than cropped.
    pub(crate) fn video(cx: &mut Context<Self>) -> Self {
        let video = Sonora::global(cx).video.clone();
        cx.observe(&video, |_, _, cx| cx.notify()).detach();
        Self::with(Source::Video, cx)
    }

    fn with(source: Source, cx: &mut Context<Self>) -> Self {
        // a resume after a pause has no new frame to wake the loop, so playback does
        let playback = Sonora::global(cx).playback.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        Self {
            source,
            image: None,
            index: 0,
            width: px(0.),
            height: px(0.),
            radius: px(0.),
            fill: false,
            cover: false,
            armed: false,
            drawn: Instant::now(),
            generation: 0,
        }
    }

    /// The box the owner lays the clip out in. A canvas covers it, cropped like a cover; a video
    /// sits inside it, letterboxed.
    pub(crate) fn set_side(&mut self, width: Pixels, height: Pixels, radius: Pixels) {
        (self.width, self.height, self.radius) = (width, height, radius);
        self.fill = false;
    }

    /// Makes the clip fill the window-sized box its parent lays it in, with no lag behind a
    /// resize, rounding only its bottom corners by `radius` so the top meets the title bar.
    pub(crate) fn fill(&mut self, radius: Pixels) {
        self.radius = radius;
        self.fill = true;
    }

    /// Crops a video to cover its box rather than letterboxing it, for a box whose shape only
    /// briefly differs from the video's, such as a window being resized.
    pub(crate) fn cover(&mut self) {
        self.cover = true;
    }

    /// How the clip fits its box: a Canvas, or a video asked to `cover`, is cropped to fill
    /// it, and any other video is letterboxed.
    fn fit(&self) -> ObjectFit {
        match self.source == Source::Canvas || self.cover {
            true => ObjectFit::Cover,
            false => ObjectFit::Contain,
        }
    }

    /// Takes the player's newest frame when it is new, and keeps the loop armed while one plays.
    fn pull(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (width, height) = match self.fill {
            true => (window.viewport_size().width, window.viewport_size().height),
            false => (self.width, self.height),
        };
        let drawn = match (self.source, self.fit()) {
            // an upright clip cropped to the box is as wide as the box, or wider
            (Source::Canvas, _) => width.max(height * (9. / 16.)),
            // a wide clip cropped to the box is as wide as the box, or wider
            (Source::Video, ObjectFit::Cover) => width.max(height * (16. / 9.)),
            // a wide clip fitted into the box is as wide as the box, or narrower
            (Source::Video, _) => width.min(height * (16. / 9.)),
        };
        let wide = (f32::from(drawn) * window.scale_factor()).ceil();
        let Some((generation, newest)) = newest(self.source, Some(wide as u32), cx) else {
            self.release(window, cx);
            return;
        };
        // another clip since the last draw, maybe while this view was off screen
        if generation != self.generation {
            let recent = self.drawn.elapsed() <= LINGER;
            match keep_frame(self.source, true, newest.is_some(), recent) {
                // any frame of the new clip replaces the held one, whatever its index
                true => self.index = 0,
                false => self.release(window, cx),
            }
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
                let newest = newest(this.source, None, cx)
                    .and_then(|(_, frame)| frame)
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
        let playing = match self.source {
            Source::Canvas => canvas_playing(cx),
            Source::Video => video_showing(cx),
        };
        match playing {
            true => self.pull(window, cx),
            false => self.release(window, cx),
        }
        let Some(image) = self.image.clone() else {
            return gpui::div().into_any_element();
        };
        img(image)
            .map(|this| match self.fill {
                true => this
                    .size_full()
                    .rounded_bl(self.radius)
                    .rounded_br(self.radius),
                false => this.w(self.width).h(self.height).rounded(self.radius),
            })
            .object_fit(self.fit())
            .into_any_element()
    }
}

/// The generation of `source`'s clip and its newest frame, or `None` while nothing plays. With
/// a `width` it also tells the player it is drawn that many pixels wide this frame.
fn newest(source: Source, width: Option<u32>, cx: &App) -> Option<(u64, Option<Arc<Picture>>)> {
    let sonora = Sonora::global(cx);
    match source {
        Source::Canvas => {
            let canvas = sonora.canvas.read(cx);
            let player = canvas.player()?;
            if let Some(width) = width {
                player.watch(width);
            }
            Some((canvas.generation(), player.frame()))
        }
        Source::Video => {
            let video = sonora.video.read(cx);
            let movie = video.movie()?;
            if let Some(width) = width {
                // the reload catching up behind the movie decodes at the same width
                video.watch(width);
            }
            Some((video.generation(), movie.frame()))
        }
    }
}

/// Whether the picture held stays up when the clip's generation may have `changed`. A Canvas
/// drops it at once, since its next clip is another song. A video keeps it while the new movie
/// has no frame yet, which is a reload at another height of the same video, but only when
/// this view drew it a moment ago: an older picture may be from the track before.
fn keep_frame(source: Source, changed: bool, has_frame: bool, drawn_recently: bool) -> bool {
    !changed || (source == Source::Video && !has_frame && drawn_recently)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reloaded_video_keeps_its_last_picture_until_the_new_one() {
        assert!(keep_frame(Source::Video, true, false, true));
        assert!(!keep_frame(Source::Video, true, true, true));
        assert!(keep_frame(Source::Video, false, true, false));
    }

    #[test]
    fn a_stale_picture_or_a_new_canvas_is_dropped() {
        assert!(!keep_frame(Source::Video, true, false, false));
        assert!(!keep_frame(Source::Canvas, true, false, true));
    }

    /// `pull` frees each frame before the next is allocated, yet a frame stuck below a live
    /// cover takes etagere's coalescing path, whose bucket counter is never reset and overflowed
    /// a debug build after about 65,000 frames. Four hours at 30 fps must stay up.
    #[test]
    fn hours_of_video_frames_do_not_overflow_the_atlas() {
        use etagere::{BucketedAtlasAllocator, size2};

        let mut atlas = BucketedAtlasAllocator::new(size2(1024, 1024));
        let mut frame = atlas.allocate(size2(960, 540)).expect("first frame fits");
        let small = atlas.allocate(size2(64, 64)).expect("small cover fits");
        let _cover = atlas.allocate(size2(200, 200)).expect("cover fits");
        atlas.deallocate(small.id);
        for _ in 0..4 * 3600 * 30 {
            atlas.deallocate(frame.id);
            frame = atlas.allocate(size2(960, 540)).expect("next frame fits");
        }
    }
}
