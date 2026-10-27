use gpui::prelude::*;
use gpui::{AnyElement, App, Context};
use i18n::t;
use state::FullscreenControlsAutohide;
use state::SpectrumPlace;
use ui::Switch;

use super::rows::segmented;
use super::{Group, Row, SettingsView, Slot};

impl SettingsView {
    pub(super) fn now_playing_groups(&self, cx: &App) -> Vec<Group> {
        vec![
            Group::new("settings-group-background", "icons/blend.svg")
                .rows([Slot::Ambient, Slot::AmbientMotion]),
            self.video_group(cx),
            self.spectrum_group(SpectrumPlace::App, cx),
            self.spectrum_group(SpectrumPlace::Mini, cx),
            Group::new("settings-group-controls", "icons/sliders-horizontal.svg")
                .rows([Slot::FullscreenControlsAutohide, Slot::OsFullscreen]),
        ]
    }

    /// The fullscreen background sampled from the cover. It carries the cover's hues itself,
    /// so fullscreen tints from the artwork with the adaptive theme off as well.
    pub(super) fn ambient_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).ambient();

        self.row(
            t!("settings-ambient"),
            Switch::new("ambient", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_ambient(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn ambient_motion_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).ambient_motion();
        let disabled = !self.settings.read(cx).ambient();

        Row::new(
            t!("settings-ambient-motion"),
            Switch::new("ambient-motion", on)
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_ambient_motion(!on, cx));
                }))
                .into_any_element(),
        )
        .disabled(disabled)
        .into_any_element()
    }

    pub(super) fn visualizer_absolute_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).visualizer_absolute();

        self.row(
            t!("settings-visualizer-absolute"),
            Switch::new("visualizer-absolute", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_visualizer_absolute(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn fullscreen_controls_autohide_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).fullscreen_controls_autohide();

        let picker = segmented(
            "fullscreen-controls-autohide",
            FullscreenControlsAutohide::ALL.map(|fca| (fca, i18n::lookup(fca.key(), None))),
            chosen,
            cx.listener(|this, fca: &FullscreenControlsAutohide, _, cx| {
                let fca = *fca;
                this.settings.update(cx, |settings, cx| {
                    settings.set_fullscreen_controls_autohide(fca, cx)
                });
                cx.notify();
            }),
        );

        self.row(
            t!("settings-fullscreen-controls-autohide"),
            picker.into_any_element(),
        )
    }

    pub(super) fn os_fullscreen_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).os_fullscreen();

        self.row(
            t!("settings-os-fullscreen"),
            Switch::new("os-fullscreen", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_os_fullscreen(!on, cx));
                }))
                .into_any_element(),
        )
    }
}
