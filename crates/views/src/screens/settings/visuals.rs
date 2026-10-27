use gpui::prelude::*;
use gpui::{AnyElement, App, Context, SharedString, div};
use i18n::t;
use state::{
    CodecState, MiniCorner, SpectrumPlace, SpectrumProfile, SpectrumSettings, VIDEO_HEIGHTS,
    WaveformPlace,
};
use ui::{
    ActiveTheme as _, Button, MenuItem, Picker, Scrubber, ScrubberState, Switch, Text,
    VisualizerColor, VisualizerStyle, WaveDensity, WaveStyle,
};

use super::{Group, Row, SettingsView, Slot};

/// One control of a place's spectrum. The app's has three more, for where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Knob {
    Style,
    Intensity,
    Color,
    Gradient,
    BarWidth,
    Peaks,
    Stereo,
    Response,
    Opacity,
    Strip,
    Fullscreen,
    StripHeight,
}

impl Knob {
    const SLIDERS: [Self; 5] = [
        Self::Intensity,
        Self::BarWidth,
        Self::Response,
        Self::Opacity,
        Self::StripHeight,
    ];
}

/// The slider states of every place's spectrum, made once with the page.
pub(super) fn spectrum_sliders() -> std::collections::HashMap<(SpectrumPlace, Knob), ScrubberState>
{
    [SpectrumPlace::App, SpectrumPlace::Mini]
        .into_iter()
        .flat_map(|place| Knob::SLIDERS.map(move |knob| (place, knob)))
        .map(|(place, knob)| {
            let id = format!("spectrum-{}-{}", place_id(place), knob_id(knob));
            ((place, knob), ScrubberState::new(id))
        })
        .collect()
}

impl SettingsView {
    /// The waveform drawn along the seek bars, and where.
    pub(super) fn waveform_group(&self, cx: &App) -> Group {
        Group::new("settings-group-waveform", "icons/music.svg")
            .rows([Slot::Waveform])
            .rows(self.waveform_slots(cx))
    }

    /// What plays in place of the cover: a Spotify canvas, a YouTube video, and the decoder
    /// both need.
    pub(super) fn video_group(&self, cx: &App) -> Group {
        let video = self.settings.read(cx).video();
        // offered where Cisco ships a decoder, and shown wherever one is installed
        let codec =
            self.codec.read(cx).supported() || self.codec.read(cx).state() == CodecState::Ready;
        Group::new("settings-video", "icons/video.svg")
            .rows([Slot::Canvas, Slot::Video])
            .rows(
                video
                    .then_some([Slot::VideoQuality, Slot::PipCorner])
                    .into_iter()
                    .flatten(),
            )
            .rows(codec.then_some(Slot::CanvasCodec))
    }

    /// One place's spectrum, its style first.
    pub(super) fn spectrum_group(&self, place: SpectrumPlace, cx: &App) -> Group {
        let title = match place {
            SpectrumPlace::App => "settings-group-spectrum-app",
            SpectrumPlace::Mini => "settings-group-spectrum-mini",
        };
        let group = Group::new(title, "icons/music-2.svg").rows(self.spectrum_slots(place, cx));
        match place {
            SpectrumPlace::App => group,
            SpectrumPlace::Mini => {
                group.rows(self.spectrum_shown(cx).then_some(Slot::VisualizerAbsolute))
            }
        }
    }

    /// The title and detail a search matches one of these settings by.
    pub(super) fn visuals_meta(&self, slot: Slot) -> Option<(SharedString, SharedString)> {
        Some(match slot {
            Slot::Canvas => (t!("settings-canvas"), t!("settings-canvas-detail")),
            Slot::Video => (t!("settings-video"), t!("settings-video-detail")),
            Slot::VideoQuality => (
                t!("settings-video-quality"),
                t!("settings-video-quality-detail"),
            ),
            Slot::PipCorner => (t!("settings-pip-corner"), t!("settings-pip-corner-detail")),
            Slot::Waveform => (t!("settings-waveform"), t!("settings-waveform-detail")),
            Slot::WaveformStyle => (
                t!("settings-waveform-style"),
                t!("settings-waveform-style-detail"),
            ),
            Slot::WaveformDensity => (
                t!("settings-waveform-density"),
                t!("settings-waveform-density-detail"),
            ),
            Slot::WaveformIn(place) => {
                let (title, detail) = waveform_place_keys(place);
                (i18n::lookup(title, None), i18n::lookup(detail, None))
            }
            Slot::WaveformMeasure => (
                t!("settings-waveform-measure"),
                t!("settings-waveform-measure-detail"),
            ),
            Slot::CanvasCodec => (
                t!("settings-canvas-codec"),
                t!("settings-canvas-codec-missing"),
            ),
            Slot::Spectrum(place, knob) => {
                let (title, detail) = knob_keys(knob);
                let group = match place {
                    SpectrumPlace::App => t!("settings-group-spectrum-app"),
                    SpectrumPlace::Mini => t!("settings-group-spectrum-mini"),
                };
                (
                    i18n::lookup(title, None),
                    format!("{} {group}", i18n::lookup(detail, None)).into(),
                )
            }
            _ => return None,
        })
    }

    /// Builds one of these settings' rows, or `None` for a slot that is not one of them.
    pub(super) fn visuals_element(&self, slot: Slot, cx: &mut Context<Self>) -> Option<AnyElement> {
        Some(match slot {
            Slot::Canvas => self.canvas_row(cx),
            Slot::Video => self.video_row(cx),
            Slot::VideoQuality => self.video_quality_row(cx),
            Slot::PipCorner => self.pip_corner_row(cx),
            Slot::Waveform => self.waveform_row(cx),
            Slot::WaveformStyle => self.waveform_style_row(cx),
            Slot::WaveformDensity => self.waveform_density_row(cx),
            Slot::WaveformIn(place) => self.waveform_place_row(place, cx),
            Slot::WaveformMeasure => self.waveform_measure_row(cx),
            Slot::CanvasCodec => self.codec_row(cx),
            Slot::Spectrum(place, knob) => self.spectrum_row(place, knob, cx),
            _ => return None,
        })
    }

    fn switch_row(
        &self,
        title: SharedString,
        detail: SharedString,
        id: &'static str,
        on: bool,
        flip: impl Fn(&mut state::AppSettings, bool, &mut Context<state::AppSettings>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Row::new(
            title,
            Switch::new(id, on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| flip(settings, !on, cx));
                    cx.notify();
                }))
                .into_any_element(),
        )
        .detail(detail)
        .into_any_element()
    }

    fn canvas_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).canvas();
        self.switch_row(
            t!("settings-canvas"),
            t!("settings-canvas-detail"),
            "canvas",
            on,
            |settings, on, cx| settings.set_canvas(on, cx),
            cx,
        )
    }

    fn video_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).video();
        self.switch_row(
            t!("settings-video"),
            t!("settings-video-detail"),
            "video",
            on,
            |settings, on, cx| settings.set_video(on, cx),
            cx,
        )
    }

    /// The tallest video stream to fetch; the sidebar and the floating window ask for less.
    fn video_quality_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).video_max_height();
        let picker = Picker::new("video-quality", &self.popovers, format!("{chosen}p"))
            .width(Picker::NARROW)
            .items(VIDEO_HEIGHTS.map(|height| {
                MenuItem::new(format!("video-quality-{height}"), format!("{height}p"))
                    .selected(height == chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_video_max_height(height, cx));
                    }))
            }));

        Row::new(t!("settings-video-quality"), picker.into_any_element())
            .detail(t!("settings-video-quality-detail"))
            .into_any_element()
    }

    fn pip_corner_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).pip_corner();
        let picker = Picker::new(
            "pip-corner",
            &self.popovers,
            i18n::lookup(chosen.key(), None),
        )
        .width(Picker::NARROW)
        .items(MiniCorner::ALL.map(|corner| {
            MenuItem::new(corner.key(), i18n::lookup(corner.key(), None))
                .selected(corner == chosen)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_pip_corner(corner, cx));
                }))
        }));

        Row::new(t!("settings-pip-corner"), picker.into_any_element())
            .detail(t!("settings-pip-corner-detail"))
            .into_any_element()
    }

    fn waveform_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).waveform();
        self.switch_row(
            t!("settings-waveform"),
            t!("settings-waveform-detail"),
            "waveform",
            on,
            |settings, on, cx| settings.set_waveform(on, cx),
            cx,
        )
    }

    /// The rows under the waveform switch, shown only while it is on.
    fn waveform_slots(&self, cx: &App) -> Vec<Slot> {
        if !self.settings.read(cx).waveform() {
            return Vec::new();
        }
        [Slot::WaveformStyle, Slot::WaveformDensity]
            .into_iter()
            .chain(WaveformPlace::ALL.map(Slot::WaveformIn))
            .chain([Slot::WaveformMeasure])
            .collect()
    }

    fn waveform_style_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).waveform_style();
        let picker = Picker::new("waveform-style", &self.popovers, chosen.label())
            .width(Picker::NARROW)
            .items(WaveStyle::ALL.map(|style| {
                MenuItem::new(style.id(), style.label())
                    .selected(style == chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_waveform_style(style, cx));
                    }))
            }));

        Row::new(t!("settings-waveform-style"), picker.into_any_element())
            .detail(t!("settings-waveform-style-detail"))
            .into_any_element()
    }

    fn waveform_density_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).waveform_density();
        let picker = Picker::new("waveform-density", &self.popovers, chosen.label())
            .width(Picker::NARROW)
            .items(WaveDensity::ALL.map(|density| {
                MenuItem::new(density.id(), density.label())
                    .selected(density == chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_waveform_density(density, cx)
                        });
                    }))
            }));

        Row::new(t!("settings-waveform-density"), picker.into_any_element())
            .detail(t!("settings-waveform-density-detail"))
            .into_any_element()
    }

    fn waveform_place_row(&self, place: WaveformPlace, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).waveform_place(place);
        let (title, detail) = waveform_place_keys(place);
        self.switch_row(
            i18n::lookup(title, None),
            i18n::lookup(detail, None),
            title,
            on,
            move |settings, on, cx| settings.set_waveform_place(place, on, cx),
            cx,
        )
    }

    fn waveform_measure_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).measure_waveforms();
        self.switch_row(
            t!("settings-waveform-measure"),
            t!("settings-waveform-measure-detail"),
            "waveform-measure",
            on,
            |settings, on, cx| settings.set_measure_waveforms(on, cx),
            cx,
        )
    }

    /// Cisco's decoder for canvases and videos: where it stands, a download when there is none,
    /// and a way to remove the one Sonora fetched. The ready detail is Cisco's required
    /// attribution.
    fn codec_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.codec.read(cx).state();
        let detail = match state {
            CodecState::Ready => t!("settings-canvas-codec-ready"),
            CodecState::Failed => t!("settings-canvas-codec-failed"),
            CodecState::Installing => t!("settings-canvas-codec-installing"),
            _ => t!("settings-canvas-codec-missing"),
        };
        let action = match state {
            CodecState::Ready => Button::new("remove-codec")
                .label(t!("settings-canvas-codec-remove"))
                .small()
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.codec.update(cx, |codec, cx| codec.uninstall(cx));
                })),
            _ => Button::new("fetch-codec")
                .label(t!("settings-canvas-codec-fetch"))
                .small()
                .outline()
                .disabled(state == CodecState::Installing)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.codec.update(cx, |codec, cx| codec.install(cx));
                })),
        };

        Row::new(t!("settings-canvas-codec"), action.into_any_element())
            .detail(detail)
            .into_any_element()
    }

    /// Whether either spectrum is drawn at all, so the setting shared by both has something to
    /// change.
    fn spectrum_shown(&self, cx: &App) -> bool {
        let spectrum = self.settings.read(cx).spectrum();
        [SpectrumPlace::App, SpectrumPlace::Mini]
            .into_iter()
            .any(|place| spectrum.profile(place).style().shown())
    }

    /// The rows one place's spectrum shows. The style comes first and switches it off; the rest
    /// only while it is drawn, and the bar options only for a style made of bars.
    fn spectrum_slots(&self, place: SpectrumPlace, cx: &App) -> Vec<Slot> {
        let spectrum = self.settings.read(cx).spectrum();
        let style = spectrum.profile(place).style();
        let row = |knob| Slot::Spectrum(place, knob);
        let mut slots = vec![row(Knob::Style)];
        if !style.shown() {
            return slots;
        }
        if place == SpectrumPlace::App {
            slots.push(row(Knob::Strip));
            if spectrum.in_strip {
                slots.push(row(Knob::StripHeight));
            }
            slots.push(row(Knob::Fullscreen));
        }
        slots.extend([row(Knob::Intensity), row(Knob::Color), row(Knob::Gradient)]);
        if matches!(
            style,
            VisualizerStyle::Bars | VisualizerStyle::Both | VisualizerStyle::Mirror
        ) {
            slots.extend([row(Knob::BarWidth), row(Knob::Peaks)]);
        }
        slots.extend([row(Knob::Stereo), row(Knob::Response), row(Knob::Opacity)]);
        slots
    }

    fn spectrum_row(&self, place: SpectrumPlace, knob: Knob, cx: &mut Context<Self>) -> AnyElement {
        let spectrum = self.settings.read(cx).spectrum();
        let profile = spectrum.profile(place).clone();
        let (title, detail) = knob_keys(knob);

        let control = match knob {
            Knob::Style => {
                let chosen = profile.style();
                Picker::new(picker_key(place, knob), &self.popovers, chosen.label())
                    .width(Picker::NARROW)
                    .items(VisualizerStyle::ALL.map(|style| {
                        MenuItem::new(style.id(), style.label())
                            .selected(style == chosen)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.spectrum(place, cx, |profile| profile.set_style(style));
                            }))
                    }))
                    .into_any_element()
            }
            Knob::Color => self.spectrum_color(place, knob, Some(profile.color()), false, cx),
            Knob::Gradient => self.spectrum_color(place, knob, profile.gradient(), true, cx),
            Knob::Peaks | Knob::Stereo | Knob::Strip | Knob::Fullscreen => {
                let on = match knob {
                    Knob::Peaks => profile.peaks,
                    Knob::Stereo => profile.stereo,
                    Knob::Strip => spectrum.in_strip,
                    _ => spectrum.in_fullscreen,
                };
                Switch::new(picker_key(place, knob), on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_spectrum(
                                |spectrum| match knob {
                                    Knob::Peaks => spectrum.profile_mut(place).peaks = !on,
                                    Knob::Stereo => spectrum.profile_mut(place).stereo = !on,
                                    Knob::Strip => spectrum.in_strip = !on,
                                    _ => spectrum.in_fullscreen = !on,
                                },
                                cx,
                            )
                        });
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Knob::Intensity
            | Knob::BarWidth
            | Knob::Response
            | Knob::Opacity
            | Knob::StripHeight => self.spectrum_slider(place, knob, &spectrum, cx),
        };

        Row::new(i18n::lookup(title, None), control)
            .detail(i18n::lookup(detail, None))
            .into_any_element()
    }

    /// A colour picker with a swatch of what it resolves to. The fade picker also offers none.
    fn spectrum_color(
        &self,
        place: SpectrumPlace,
        knob: Knob,
        chosen: Option<VisualizerColor>,
        optional: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let label = chosen.map_or_else(|| t!("settings-spectrum-gradient-none"), |c| c.label());
        let set = move |this: &mut Self, color: Option<VisualizerColor>, cx: &mut Context<Self>| {
            this.spectrum(place, cx, |profile| match knob {
                Knob::Gradient => profile.set_gradient(color),
                _ => profile.set_color(color.unwrap_or_default()),
            });
        };

        let mut picker =
            Picker::new(picker_key(place, knob), &self.popovers, label).width(Picker::NARROW);
        if optional {
            picker = picker.item(
                MenuItem::new("none", t!("settings-spectrum-gradient-none"))
                    .selected(chosen.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| set(this, None, cx))),
            );
        }
        picker = picker.items(VisualizerColor::SOURCES.map(|source| {
            MenuItem::new(source.id(), source.label())
                .selected(chosen == Some(source))
                .on_click(cx.listener(move |this, _, _, cx| set(this, Some(source), cx)))
        }));
        // A colour typed into settings.json is not something a menu can offer, so it joins the
        // list as the entry it already is rather than disappearing from it.
        if let Some(custom @ VisualizerColor::Custom(_)) = chosen {
            picker = picker.item(MenuItem::new("custom", custom.label()).selected(true));
        }

        div()
            .flex()
            .items_center()
            .gap_2()
            .when_some(chosen, |this, color| {
                this.child(
                    div()
                        .size(theme.text(Text::Body))
                        .flex_none()
                        .rounded_full()
                        .bg(color.resolve(cx)),
                )
            })
            .child(picker)
            .into_any_element()
    }

    /// A slider for one of a spectrum's numbers, with its value read out beside it.
    fn spectrum_slider(
        &self,
        place: SpectrumPlace,
        knob: Knob,
        spectrum: &SpectrumSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let (low, high) = knob_range(knob);
        let value = knob_value(knob, spectrum, place);
        let fraction = ((value - low) / (high - low)).clamp(0., 1.);
        let reading = match knob {
            Knob::BarWidth => t!("settings-spectrum-pixels", value = format!("{value:.1}")),
            Knob::Response => t!("settings-spectrum-millis", value = value.round() as i64),
            _ => t!(
                "settings-spectrum-percent",
                value = (value * 100.).round() as i64
            ),
        };
        let Some(state) = self.spectrum_sliders.get(&(place, knob)) else {
            return div().into_any_element();
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div().w(theme.metrics.cover).child(
                    Scrubber::new(state, fraction)
                        .colors(theme.progress_bar, theme.muted, theme.foreground)
                        .on_move(cx.listener(move |this, fraction: &f32, _, cx| {
                            let value = low + fraction * (high - low);
                            this.settings.update(cx, |settings, cx| {
                                settings.set_spectrum(
                                    |spectrum| set_knob(knob, spectrum, place, value),
                                    cx,
                                )
                            });
                        })),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .w(theme.metrics.control * 1.5)
                    .whitespace_nowrap()
                    .text_right()
                    .child(reading),
            )
            .into_any_element()
    }

    /// Changes one place's profile.
    fn spectrum(
        &mut self,
        place: SpectrumPlace,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut SpectrumProfile),
    ) {
        self.settings.update(cx, |settings, cx| {
            settings.set_spectrum(|spectrum| change(spectrum.profile_mut(place)), cx)
        });
        cx.notify();
    }
}

fn place_id(place: SpectrumPlace) -> &'static str {
    match place {
        SpectrumPlace::App => "app",
        SpectrumPlace::Mini => "mini",
    }
}

fn knob_id(knob: Knob) -> &'static str {
    match knob {
        Knob::Style => "style",
        Knob::Intensity => "intensity",
        Knob::Color => "color",
        Knob::Gradient => "gradient",
        Knob::BarWidth => "bar-width",
        Knob::Peaks => "peaks",
        Knob::Stereo => "stereo",
        Knob::Response => "response",
        Knob::Opacity => "opacity",
        Knob::Strip => "strip",
        Knob::Fullscreen => "fullscreen",
        Knob::StripHeight => "strip-height",
    }
}

/// The title and detail keys of the switch that draws the waveform in `place`.
fn waveform_place_keys(place: WaveformPlace) -> (&'static str, &'static str) {
    match place {
        WaveformPlace::Bar => ("settings-waveform-bar", "settings-waveform-bar-detail"),
        WaveformPlace::Fullscreen => (
            "settings-waveform-fullscreen",
            "settings-waveform-fullscreen-detail",
        ),
        WaveformPlace::Mini => ("settings-waveform-mini", "settings-waveform-mini-detail"),
    }
}

/// The popover and element key for one place's control. It has to be static, so the two places
/// each spell theirs out.
fn picker_key(place: SpectrumPlace, knob: Knob) -> &'static str {
    match (place, knob) {
        (SpectrumPlace::App, Knob::Style) => "spectrum-app-style",
        (SpectrumPlace::App, Knob::Color) => "spectrum-app-color",
        (SpectrumPlace::App, Knob::Gradient) => "spectrum-app-gradient",
        (SpectrumPlace::App, Knob::Peaks) => "spectrum-app-peaks",
        (SpectrumPlace::App, Knob::Stereo) => "spectrum-app-stereo",
        (SpectrumPlace::App, Knob::Strip) => "spectrum-app-strip",
        (SpectrumPlace::App, Knob::Fullscreen) => "spectrum-app-fullscreen",
        (SpectrumPlace::Mini, Knob::Style) => "spectrum-mini-style",
        (SpectrumPlace::Mini, Knob::Color) => "spectrum-mini-color",
        (SpectrumPlace::Mini, Knob::Gradient) => "spectrum-mini-gradient",
        (SpectrumPlace::Mini, Knob::Peaks) => "spectrum-mini-peaks",
        (SpectrumPlace::Mini, Knob::Stereo) => "spectrum-mini-stereo",
        _ => "spectrum-other",
    }
}

/// The title and detail keys of a control.
fn knob_keys(knob: Knob) -> (&'static str, &'static str) {
    match knob {
        Knob::Style => ("settings-visualizer", "settings-visualizer-detail"),
        Knob::Intensity => (
            "settings-spectrum-intensity",
            "settings-spectrum-intensity-detail",
        ),
        Knob::Color => ("settings-spectrum-color", "settings-spectrum-color-detail"),
        Knob::Gradient => (
            "settings-spectrum-gradient",
            "settings-spectrum-gradient-detail",
        ),
        Knob::BarWidth => (
            "settings-spectrum-bar-width",
            "settings-spectrum-bar-width-detail",
        ),
        Knob::Peaks => ("settings-spectrum-peaks", "settings-spectrum-peaks-detail"),
        Knob::Stereo => (
            "settings-spectrum-stereo",
            "settings-spectrum-stereo-detail",
        ),
        Knob::Response => (
            "settings-spectrum-response",
            "settings-spectrum-response-detail",
        ),
        Knob::Opacity => (
            "settings-spectrum-opacity",
            "settings-spectrum-opacity-detail",
        ),
        Knob::Strip => ("settings-spectrum-strip", "settings-spectrum-strip-detail"),
        Knob::Fullscreen => (
            "settings-spectrum-fullscreen",
            "settings-spectrum-fullscreen-detail",
        ),
        Knob::StripHeight => (
            "settings-spectrum-strip-height",
            "settings-spectrum-strip-height-detail",
        ),
    }
}

fn knob_range(knob: Knob) -> (f32, f32) {
    match knob {
        Knob::Intensity => (ui::MIN_INTENSITY, ui::MAX_INTENSITY),
        Knob::BarWidth => SpectrumProfile::BAR_WIDTH,
        Knob::Response => SpectrumProfile::RESPONSE,
        Knob::Opacity => SpectrumProfile::OPACITY,
        _ => SpectrumSettings::STRIP_HEIGHT,
    }
}

fn knob_value(knob: Knob, spectrum: &SpectrumSettings, place: SpectrumPlace) -> f32 {
    let profile = spectrum.profile(place);
    match knob {
        Knob::Intensity => profile.intensity,
        Knob::BarWidth => profile.bar_width,
        Knob::Response => profile.response,
        Knob::Opacity => profile.opacity,
        _ => spectrum.strip_height,
    }
}

fn set_knob(knob: Knob, spectrum: &mut SpectrumSettings, place: SpectrumPlace, value: f32) {
    let profile = spectrum.profile_mut(place);
    match knob {
        Knob::Intensity => profile.intensity = value,
        Knob::BarWidth => profile.bar_width = value,
        Knob::Response => profile.response = value,
        Knob::Opacity => profile.opacity = value,
        _ => spectrum.strip_height = value,
    }
}
