use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render,
    SharedString, Window, WindowControlArea, div, px, relative, svg,
};
use i18n::t;
use state::{AppSettings, Playback, PlaybackState, Queue, Sonora};
use ui::{ActiveTheme as _, Artwork, Button, Scrubber, ScrubberState, Text, Visualizer, clock};

use crate::shared::provider_mark;
use crate::shared::transport::{next, previous, toggle, volume_icon};
use crate::shared::visualizer::VisualizerDrive;

const ART: gpui::Pixels = px(64.);
const MARK: gpui::Pixels = px(12.);
const VOLUME: gpui::Pixels = px(56.);

pub struct MiniPlayer {
    playback: Entity<Playback>,
    queue: Entity<Queue>,
    settings: Entity<AppSettings>,
    visualizer: VisualizerDrive,
    seek: ScrubberState,
    volume: ScrubberState,
    pending: Option<f32>,
    muted: Option<f32>,
    grabbed: bool,
}

impl MiniPlayer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let playback = Sonora::global(cx).playback.clone();
        let queue = Sonora::global(cx).queue.clone();
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&queue, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        Self {
            playback,
            queue,
            settings,
            visualizer: VisualizerDrive::default(),
            seek: ScrubberState::new("mini-seek"),
            volume: ScrubberState::new("mini-volume"),
            pending: None,
            muted: None,
            grabbed: false,
        }
    }

    fn loudness(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let level = self.playback.read(cx).volume();
        let restore = self.muted.unwrap_or(0.7);

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .ml_1()
            .child(
                Button::new("mini-volume-toggle")
                    .ghost()
                    .small()
                    .icon(volume_icon(level))
                    .tooltip(match level <= 0.001 {
                        true => "player-unmute",
                        false => "player-mute",
                    })
                    .tint(theme.muted_foreground)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let wanted = match level <= 0.001 {
                            true => restore,
                            false => 0.,
                        };
                        this.muted = match wanted {
                            0. => Some(level),
                            _ => None,
                        };
                        this.playback
                            .update(cx, |playback, cx| playback.set_volume(wanted, cx));
                    })),
            )
            .child(
                div().w(VOLUME).flex_none().child(
                    Scrubber::new(&self.volume, level)
                        .colors(theme.progress_bar, theme.secondary, theme.foreground)
                        .on_move(cx.listener(|this, fraction: &f32, _, cx| {
                            let level = *fraction;
                            this.muted = None;
                            this.playback
                                .update(cx, |playback, cx| playback.set_volume(level, cx));
                        })),
                ),
            )
    }

    fn commit_seek(&mut self, cx: &mut Context<Self>) {
        let Some(fraction) = self.pending.take() else {
            return;
        };
        self.playback
            .update(cx, |playback, cx| playback.seek_fraction(fraction, cx));
    }
}

impl Render for MiniPlayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let held = self.playback.read(cx);
        let track = held.track().cloned();
        let seekable = track.is_some();
        let progress = self.pending.unwrap_or_else(|| held.progress());
        let elapsed = held.position();
        let total = track
            .as_ref()
            .map(|track| track.duration)
            .unwrap_or(Duration::ZERO);

        let title = track
            .as_ref()
            .map(|track| SharedString::from(track.name.clone()))
            .unwrap_or_else(|| t!("player-nothing-playing"));
        let artists = track
            .as_ref()
            .map(|track| SharedString::from(track.artists.clone()))
            .unwrap_or_default();
        let mark = track
            .as_ref()
            .and_then(|track| track.id.as_deref())
            .and_then(|id| provider_mark(id, cx));
        let cover = track.as_ref().and_then(|track| track.cover.clone());

        let profile = self.settings.read(cx).spectrum().mini;
        let look = profile.look();
        let sounding = matches!(held.state(), PlaybackState::Playing);
        let bands = (look.style.shown() && sounding && ui::motion::animates(cx))
            .then(|| held.spectrum())
            .flatten();
        let spectrum_on = bands.is_some();
        let rise = (window.viewport_size().height - theme.radius).max(px(0.));
        match bands {
            Some(bands) => self
                .visualizer
                .show(cx.entity_id(), bands, profile.settle(), window),
            None => self.visualizer.hide(),
        }

        div()
            .size_full()
            .relative()
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
            // the window manager takes the grab on the first move, not on the
            // press: asking on the press leaves it dragging nothing, and it
            // swallows the clicks that follow
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if this.grabbed {
                    this.grabbed = false;
                    window.start_window_move();
                }
            }))
            .bg(theme.background)
            .text_color(theme.foreground)
            .border_1()
            .border_color(theme.table_row_border)
            .rounded(theme.radius)
            .line_height(relative(1.3))
            // gpui masks content with a rectangle, never with a radius, so the spectrum
            // draws the window's own corner into its floor rather than being clipped to it.
            // The spectrum is the whole window's backdrop, painted before the content so
            // every control lands on top of it. It stops a radius short of the top: only the
            // floor rounds, so a loud band there would reach into the upper corners.
            .when(spectrum_on, |this| {
                this.child(
                    Visualizer::new(self.visualizer.levels(), rise)
                        .look(look)
                        .behind(theme.background)
                        .floor_radius(theme.radius)
                        .opacity(profile.opacity)
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0(),
                )
            })
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .child(Artwork::new(cover).size(ART))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().min_w_0().truncate().child(title))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .min_w_0()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(div().min_w_0().truncate().child(artists))
                                    .children(mark.map(|icon| {
                                        svg()
                                            .path(icons::path(icon))
                                            .size(MARK)
                                            .flex_none()
                                            .text_color(theme.muted_foreground)
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(previous(&self.playback, false, cx))
                                    .child(toggle(&self.playback, false, false, cx))
                                    .child(next(&self.playback, &self.queue, false, cx))
                                    .child(self.loudness(cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(theme.text(Text::Tiny))
                                    .text_color(theme.muted_foreground)
                                    .child(div().flex_none().child(clock(elapsed)))
                                    .child(
                                        div().flex_1().min_w_0().child(
                                            Scrubber::new(&self.seek, progress)
                                                .colors(
                                                    theme.progress_bar,
                                                    theme.secondary,
                                                    theme.foreground,
                                                )
                                                .enabled(seekable)
                                                .on_move(cx.listener(
                                                    |this, fraction: &f32, _, cx| {
                                                        this.pending = Some(*fraction);
                                                        cx.notify();
                                                    },
                                                ))
                                                .on_release(cx.listener(
                                                    |this, _: &MouseUpEvent, _, cx| {
                                                        this.commit_seek(cx)
                                                    },
                                                )),
                                        ),
                                    )
                                    .child(div().flex_none().child(clock(total))),
                            ),
                    )
                    .child(
                        div().flex_none().self_start().child(
                            Button::new("mini-close")
                                .ghost()
                                .small()
                                .icon("icons/x.svg")
                                .tooltip("mini-close")
                                .on_click(|_, window, _| window.remove_window()),
                        ),
                    ),
            )
    }
}
