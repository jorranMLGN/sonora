use gpui::prelude::*;
use gpui::{Context, Entity, Render, Window, div};
use state::{AppSettings, Playback, PlaybackState, Sonora};
use ui::{ActiveTheme as _, Visualizer};

use crate::shared::visualizer::VisualizerDrive;

/// The band of spectrum that sits directly above the player bar. It is a view of its own so
/// the frame it asks for while sound is playing repaints only this strip, never the bar it
/// rests on or the page behind it.
pub(crate) struct SpectrumStrip {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    visualizer: VisualizerDrive,
}

impl SpectrumStrip {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let playback = Sonora::global(cx).playback.clone();
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        Self {
            playback,
            settings,
            visualizer: VisualizerDrive::default(),
        }
    }
}

impl Render for SpectrumStrip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let spectrum = self.settings.read(cx).spectrum();
        let profile = spectrum.app;
        let look = profile.look();
        let height = theme.metrics.player_bar * spectrum.strip_height;
        let sounding = matches!(self.playback.read(cx).state(), PlaybackState::Playing);
        let wanted =
            spectrum.in_strip && look.style.shown() && sounding && ui::motion::animates(cx);
        let bands = wanted.then(|| self.playback.read(cx).spectrum()).flatten();
        let showing = bands.is_some();
        match bands {
            Some(bands) => self
                .visualizer
                .show(cx.entity_id(), bands, profile.settle(), window),
            None => self.visualizer.hide(),
        }

        div().when(showing, |this| {
            this.child(
                Visualizer::new(self.visualizer.levels(), height)
                    .look(look)
                    .behind(theme.background)
                    .opacity(profile.opacity),
            )
        })
    }
}
