use gpui::prelude::*;
use gpui::{
    App, Background, Bounds, Div, Hsla, IntoElement, PathBuilder, Pixels, Point, RenderOnce, Rgba,
    SharedString, StyleRefinement, Window, canvas, div, linear_color_stop, linear_gradient, point,
    px,
};
use i18n::t;

use crate::theme::ActiveTheme as _;

const GAP: f32 = 3.;
/// How wide a bar is drawn, and the space left between two. Thin enough that a strip holds
/// several bars for every band, which is what makes it read as a smooth surface.
const BAR: f32 = 3.;
const BAR_GAP: f32 = 1.5;
const OPACITY: f32 = 0.32;
const FLOOR: f32 = 0.03;
const STROKE: f32 = 2.;
const CHANNELS: [f32; 2] = [0.62, 1.];
const GLOW: f32 = 10.;
const GLOW_STROKE: f32 = 2.4;
const GLOW_OPACITY: f32 = 0.9;
/// A share of the band's own height, not of the box: a fixed share of the box leaves the line
/// floating over stubby bars on a quiet track.
const BEDDED: f32 = 0.55;
const RIDE: f32 = 0.4;
const FILL: f32 = 0.22;
const FILL_BASE: f32 = 0.05;
const LINE_BLUR: f32 = 0.7;
const CONTRAST: f32 = 0.34;
/// The band a dot sits in, as a share of the column it owns.
const DOT: f32 = 0.55;
/// How tall a peak mark stands, and how much stronger than the bars it is drawn.
const PEAK: f32 = 2.;
const PEAK_OPACITY: f32 = 0.85;
pub const MIN_INTENSITY: f32 = 0.25;
pub const MAX_INTENSITY: f32 = 2.5;

/// How the spectrum is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisualizerStyle {
    None,
    Bars,
    Wave,
    #[default]
    Both,
    /// Bars growing from the middle, up and down at once.
    Mirror,
    /// A dot on each band's crest instead of a bar under it.
    Dots,
    /// The wave's stroke alone, with no fill beneath it and no glow behind it.
    Line,
}

impl VisualizerStyle {
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Bars,
        Self::Wave,
        Self::Both,
        Self::Mirror,
        Self::Dots,
        Self::Line,
    ];

    pub fn shown(self) -> bool {
        self != Self::None
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bars => "bars",
            Self::Wave => "wave",
            Self::Both => "both",
            Self::Mirror => "mirror",
            Self::Dots => "dots",
            Self::Line => "line",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "none" => Self::None,
            "bars" => Self::Bars,
            "wave" => Self::Wave,
            "mirror" => Self::Mirror,
            "dots" => Self::Dots,
            "line" => Self::Line,
            _ => Self::default(),
        }
    }

    /// The localized name. Resolved at render, never stored.
    pub fn label(self) -> SharedString {
        match self {
            Self::None => t!("settings-visualizer-style-none"),
            Self::Bars => t!("settings-visualizer-style-bars"),
            Self::Wave => t!("settings-visualizer-style-wave"),
            Self::Both => t!("settings-visualizer-style-both"),
            Self::Mirror => t!("settings-visualizer-style-mirror"),
            Self::Dots => t!("settings-visualizer-style-dots"),
            Self::Line => t!("settings-visualizer-style-line"),
        }
    }

    fn bars(self) -> bool {
        matches!(self, Self::Bars | Self::Both)
    }

    fn wave(self) -> bool {
        matches!(self, Self::Wave | Self::Both | Self::Line)
    }

    /// Whether the wave carries its fill and its glow, or is drawn as a bare stroke.
    fn bodied(self) -> bool {
        self != Self::Line
    }
}

/// Where the spectrum takes its colour from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VisualizerColor {
    #[default]
    Accent,
    /// The hue sampled from the artwork, which needs the adaptive theme to be on; without it
    /// nothing has sampled a cover and this falls back to the accent.
    Cover,
    Text,
    Custom(Hsla),
}

impl VisualizerColor {
    /// The three a picker offers. `Custom` is not among them: it carries a colour, so it is
    /// chosen by writing one rather than by picking a name.
    pub const SOURCES: [Self; 3] = [Self::Accent, Self::Cover, Self::Text];

    pub fn id(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Cover => "cover",
            Self::Text => "text",
            Self::Custom(_) => "custom",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "cover" => Self::Cover,
            "text" => Self::Text,
            other => match crate::theme::parse_color(other) {
                Some(color) => Self::Custom(color),
                None => Self::Accent,
            },
        }
    }

    /// What is stored: a name for the sources, the hex itself for a custom colour.
    pub fn stored(self) -> String {
        match self {
            Self::Custom(color) => crate::theme::hex(color),
            other => other.id().to_owned(),
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::Accent => t!("settings-visualizer-color-accent"),
            Self::Cover => t!("settings-visualizer-color-cover"),
            Self::Text => t!("settings-visualizer-color-text"),
            Self::Custom(_) => t!("settings-visualizer-color-custom"),
        }
    }

    /// The colour itself. `Cover` answers with the hue the adaptive theme sampled, and with
    /// the accent while nothing has sampled one.
    pub fn resolve(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Accent => theme.primary,
            Self::Cover => theme.tint.unwrap_or(theme.primary),
            Self::Text => theme.foreground,
            Self::Custom(color) => color,
        }
    }
}

/// Everything about how the spectrum is drawn that a user picks, gathered so each place that
/// shows one passes a single value instead of a builder call per setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualizerLook {
    pub style: VisualizerStyle,
    pub intensity: f32,
    /// How wide one bar is drawn, in pixels.
    pub bar_width: f32,
    pub peaks: bool,
    pub stereo: bool,
    pub color: VisualizerColor,
    /// The colour the spectrum fades into towards its top, when it fades.
    pub gradient: Option<VisualizerColor>,
}

impl Default for VisualizerLook {
    fn default() -> Self {
        Self {
            style: VisualizerStyle::default(),
            intensity: 1.,
            bar_width: BAR,
            peaks: false,
            stereo: false,
            color: VisualizerColor::default(),
            gradient: None,
        }
    }
}

/// A frame of the spectrum as the visualizer draws it: one band set per channel, and the peak
/// each band reached lately, which the drive keeps and lets fall.
#[derive(Clone, Debug, Default)]
pub struct Levels {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub peak_left: Vec<f32>,
    pub peak_right: Vec<f32>,
}

impl Levels {
    /// Every band swung by `intensity`, never past the ceiling.
    fn scaled(&self, intensity: f32) -> Self {
        if intensity == 1. {
            return self.clone();
        }
        let swing = |bands: &Vec<f32>| {
            bands
                .iter()
                .map(|band| (band * intensity).clamp(0., 1.))
                .collect()
        };
        Self {
            left: swing(&self.left),
            right: swing(&self.right),
            peak_left: swing(&self.peak_left),
            peak_right: swing(&self.peak_right),
        }
    }

    /// The left channel run out to the left edge and the right one to the right, the bass of
    /// both meeting in the middle. Both channels then carry the same, whole curve.
    fn split(&self) -> Self {
        let join = |left: &[f32], right: &[f32]| -> Vec<f32> {
            left.iter().rev().chain(right).copied().collect()
        };
        let bands = join(&self.left, &self.right);
        let peaks = join(&self.peak_left, &self.peak_right);
        Self {
            left: bands.clone(),
            right: bands,
            peak_left: peaks.clone(),
            peak_right: peaks,
        }
    }

    /// Both channels folded together, the louder of the two per band. What the bars draw.
    pub fn mixed(&self) -> Vec<f32> {
        louder(&self.left, &self.right)
    }

    /// The same fold for the peaks.
    fn mixed_peaks(&self) -> Vec<f32> {
        louder(&self.peak_left, &self.peak_right)
    }
}

fn louder(left: &[f32], right: &[f32]) -> Vec<f32> {
    let mut bands = left.to_vec();
    bands.resize(bands.len().max(right.len()), 0.);
    for (band, right) in bands.iter_mut().zip(right) {
        *band = band.max(*right);
    }
    bands
}

/// What a shape is filled with: one colour, or two fading from the foot to the crest.
#[derive(Clone, Copy)]
enum Paint {
    Solid(Hsla),
    Fade { top: Hsla, bottom: Hsla },
}

impl Paint {
    fn opacity(self, factor: f32) -> Self {
        match self {
            Self::Solid(color) => Self::Solid(color.opacity(factor)),
            Self::Fade { top, bottom } => Self::Fade {
                top: top.opacity(factor),
                bottom: bottom.opacity(factor),
            },
        }
    }

    /// The same paint, the top and the foot each taken down by their own share. A fade runs
    /// across whatever it is painted on, so one gradient carries both the colour and the body.
    fn faded(self, top: f32, bottom: f32) -> Background {
        let (high, low) = self.ends();
        linear_gradient(
            180.,
            linear_color_stop(high.opacity(top), 0.),
            linear_color_stop(low.opacity(bottom), 1.),
        )
    }

    fn background(self) -> Background {
        match self {
            Self::Solid(color) => color.into(),
            Self::Fade { .. } => self.faded(1., 1.),
        }
    }

    /// The colour at `share` of the height, from the foot at 0 to the crest at 1.
    fn at(self, share: f32) -> Hsla {
        let (top, bottom) = self.ends();
        mix(bottom, top, share.clamp(0., 1.))
    }

    fn ends(self) -> (Hsla, Hsla) {
        match self {
            Self::Solid(color) => (color, color),
            Self::Fade { top, bottom } => (top, bottom),
        }
    }
}

/// `from` walked `share` of the way to `to`, through RGB so a fade between two hues does not
/// swing round the colour wheel on its way.
fn mix(from: Hsla, to: Hsla, share: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (from.into(), to.into());
    let lerp = |x: f32, y: f32| x + (y - x) * share;
    Rgba {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: lerp(a.a, b.a),
    }
    .into()
}

#[derive(IntoElement)]
pub struct Visualizer {
    base: Div,
    levels: Levels,
    max: Pixels,
    style: VisualizerStyle,
    intensity: f32,
    bar_width: f32,
    peaks: bool,
    stereo: bool,
    floor_radius: Pixels,
    color: VisualizerColor,
    gradient: Option<VisualizerColor>,
    tint: Option<Hsla>,
    behind: Option<Hsla>,
}

impl Visualizer {
    #[track_caller]
    pub fn new(levels: Levels, max: Pixels) -> Self {
        let look = VisualizerLook::default();
        Self {
            base: div(),
            levels,
            max,
            style: look.style,
            intensity: look.intensity,
            bar_width: look.bar_width,
            peaks: look.peaks,
            stereo: look.stereo,
            floor_radius: Pixels::ZERO,
            color: look.color,
            gradient: look.gradient,
            tint: None,
            behind: None,
        }
    }

    /// Everything the user picked about how the spectrum is drawn, in one go.
    pub fn look(mut self, look: VisualizerLook) -> Self {
        self.style = look.style;
        self.intensity = look.intensity.clamp(MIN_INTENSITY, MAX_INTENSITY);
        self.bar_width = look.bar_width.max(1.);
        self.peaks = look.peaks;
        self.stereo = look.stereo;
        self.color = look.color;
        self.gradient = look.gradient;
        self
    }

    /// The corner radius the floor follows, for a visualizer that stands on a rounded edge.
    /// gpui masks content with a rectangle, so a spectrum flush against a rounded window has
    /// to draw the curve itself rather than be clipped to it.
    pub fn floor_radius(mut self, radius: Pixels) -> Self {
        self.floor_radius = radius;
        self
    }

    /// How far the bands swing, as a multiple of what the analyzer reports. Clamped at the
    /// ceiling either way, so turning it up flattens the loud bands rather than overflowing
    /// the box.
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity.clamp(MIN_INTENSITY, MAX_INTENSITY);
        self
    }

    pub fn style_kind(mut self, style: VisualizerStyle) -> Self {
        self.style = style;
        self
    }

    /// Overrides the picked colour; a caller that knows what is actually behind the visualizer
    /// — a cover-derived backdrop, say — passes its own.
    pub fn tint(mut self, tint: Hsla) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn behind(mut self, behind: Hsla) -> Self {
        self.behind = Some(behind);
        self
    }
}

impl Styled for Visualizer {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Visualizer {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            levels,
            max,
            style,
            intensity,
            bar_width,
            peaks,
            stereo,
            floor_radius,
            color,
            gradient,
            tint,
            behind,
        } = self;
        let levels = levels.scaled(intensity);
        let levels = match stereo {
            true => levels.split(),
            false => levels,
        };
        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());
        let behind = behind.unwrap_or(theme.background);
        let base_color = legible(tint.unwrap_or_else(|| color.resolve(cx)), behind);
        let paint = match gradient {
            Some(top) => Paint::Fade {
                top: legible(top.resolve(cx), behind),
                bottom: base_color,
            },
            None => Paint::Solid(base_color),
        };

        let ride = match style {
            VisualizerStyle::Both => RIDE,
            _ => 0.,
        };
        let bar_paint = match style {
            VisualizerStyle::Both => paint.opacity(OPACITY * BEDDED),
            _ => paint.opacity(OPACITY),
        };
        let shape = Shape {
            max,
            ride,
            radius: floor_radius,
            bar: bar_width,
            peaks,
        };

        let mut visualizer = base
            .h(max)
            .relative()
            .when(style.bars(), |this| {
                this.child(bars(&levels, shape, bar_paint, paint))
            })
            .when(style == VisualizerStyle::Mirror, |this| {
                this.child(mirror(&levels, shape, paint.opacity(OPACITY), paint))
            })
            .when(style == VisualizerStyle::Dots, |this| {
                this.child(dots(&levels, max, paint.opacity(OPACITY * 2.)))
            })
            .when(style.wave(), |this| {
                // A blur is applied to a whole layer after it is painted and spreads past every
                // edge in it, so on a rounded floor the glow would haze over the corner the
                // rest of the wave is careful to keep clear. There, the wave goes without it.
                let glows = style.bodied() && floor_radius <= Pixels::ZERO;
                this.when(glows, |this| this.child(glow(levels.clone(), paint)))
                    .child(line(levels, paint, style.bodied(), floor_radius))
            });

        visualizer.style().refine(&overrides);
        visualizer
    }
}

/// What every bar-drawing style shares about the box it draws in.
#[derive(Clone, Copy)]
struct Shape {
    max: Pixels,
    ride: f32,
    radius: Pixels,
    /// How wide one bar is drawn, in pixels.
    bar: f32,
    peaks: bool,
}

/// The bars, painted as paths rather than laid out as elements. A corner radius on an element
/// is clamped to half its size, and a bar's height is the music's to decide: a quiet band at
/// the edge is a couple of pixels tall and could never carry the window's curve. Painted, each
/// bar is simply cut to the rounded floor wherever it stands in a corner.
///
/// There are far more bars than bands. Each is read off the same smooth curve the wave is drawn
/// through, so neighbours rise and fall together rather than stepping from band to band, and
/// all of them go into one path so a wide strip costs one paint instead of hundreds.
fn bars(levels: &Levels, shape: Shape, paint: Paint, marks: Paint) -> impl IntoElement {
    let bands = levels.mixed();
    let peaks = levels.mixed_peaks();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let Some((count, width, gap)) = columns(bounds, bands.len(), shape.bar) else {
                return;
            };
            let floor = Floor::new(bounds, shape.radius);
            let lift = |level: f32| shape.max * level.clamp(FLOOR, 1.) / (1. + shape.ride);
            let marked = shape.peaks && peaks.len() >= 2;
            let mut body = PathBuilder::fill();
            let mut caps = PathBuilder::fill();
            for index in 0..count {
                let left = bounds.origin.x + (width + gap) * index as f32;
                let at = (index as f32 + 0.5) / count as f32;
                bar(
                    &mut body,
                    &floor,
                    left,
                    left + width,
                    lift(sample(&bands, at)),
                );
                if marked {
                    let peak = sample(&peaks, at);
                    if peak > FLOOR * 2. {
                        cap(
                            &mut caps,
                            &floor,
                            left,
                            left + width,
                            floor.bottom - lift(peak),
                        );
                    }
                }
            }
            painted(body, paint.background(), window);
            if marked {
                painted(caps, marks.opacity(PEAK_OPACITY).background(), window);
            }
        },
    )
    .absolute()
    .inset_0()
}

/// A peak mark whose top sits at `top`, added to `builder`, kept inside the rounded floor.
fn cap(builder: &mut PathBuilder, floor: &Floor, left: Pixels, right: Pixels, top: Pixels) {
    let bottom = top + px(PEAK);
    let (inside_left, inside_right) = floor.sides(bottom);
    let (left, right) = (left.max(inside_left), right.min(inside_right));
    if right <= left || bottom > floor.at(left).min(floor.at(right)) {
        return;
    }
    builder.move_to(point(left, top));
    builder.line_to(point(right, top));
    builder.line_to(point(right, bottom));
    builder.line_to(point(left, bottom));
    builder.close();
}

/// Paints a built shape, logging rather than failing when it could not be built.
fn painted(builder: PathBuilder, background: Background, window: &mut Window) {
    match builder.build() {
        Ok(path) => window.paint_path(path, background),
        Err(error) => log::warn!("visualizer: cannot build a shape: {error}"),
    }
}

/// The band curve at `at`, from 0 at the left edge to 1 at the right. A band sits at the middle
/// of its own column, the same place the wave puts its point, and the curve between two is a
/// Catmull-Rom spline so it passes through every band on its way.
fn sample(bands: &[f32], at: f32) -> f32 {
    let last = bands.len() as isize - 1;
    let t = (at * bands.len() as f32 - 0.5).clamp(0., last as f32);
    let index = t.floor() as isize;
    let f = t - index as f32;
    let band = |offset: isize| bands[(index + offset).clamp(0, last) as usize];
    let (p0, p1, p2, p3) = (band(-1), band(0), band(1), band(2));
    let value = 0.5
        * (2. * p1
            + (p2 - p0) * f
            + (2. * p0 - 5. * p1 + 4. * p2 - p3) * f * f
            + (3. * (p1 - p2) + p3 - p0) * f * f * f);
    value.clamp(0., 1.)
}

/// The bottom of the box a visualizer stands in, rounded into its two lower corners.
struct Floor {
    left: Pixels,
    right: Pixels,
    bottom: Pixels,
    radius: Pixels,
}

/// How many straight pieces a bar's cut is drawn with. A bar is a few pixels wide, so this is
/// well under a pixel per piece.
const CUT: usize = 6;

impl Floor {
    fn new(bounds: Bounds<Pixels>, radius: Pixels) -> Self {
        let span = bounds.size.width.min(bounds.size.height * 2.) / 2.;
        Self {
            left: bounds.origin.x,
            right: bounds.origin.x + bounds.size.width,
            bottom: bounds.origin.y + bounds.size.height,
            radius: radius.max(Pixels::ZERO).min(span),
        }
    }

    /// How far into the corner's circle `x` stands, from its centre: nothing outside the
    /// two corners.
    fn reach(&self, x: Pixels) -> Option<Pixels> {
        let r = self.radius;
        match (x < self.left + r, x > self.right - r) {
            (true, _) => Some(self.left + r - x),
            (_, true) => Some(x - (self.right - r)),
            _ => None,
        }
    }

    /// The lowest point still inside the rounded box at `x`.
    fn at(&self, x: Pixels) -> Pixels {
        let r = self.radius;
        match self.reach(x) {
            None => self.bottom,
            Some(reach) => {
                let reach = reach.min(r);
                let lift = r - px((f32::from(r).powi(2) - f32::from(reach).powi(2))
                    .max(0.)
                    .sqrt());
                self.bottom - lift
            }
        }
    }

    /// Where the rounded box's side stands at height `y`, on the left and on the right.
    fn sides(&self, y: Pixels) -> (Pixels, Pixels) {
        let r = self.radius;
        let over = y - (self.bottom - r);
        if over <= Pixels::ZERO {
            return (self.left, self.right);
        }
        let inset = r - px((f32::from(r).powi(2) - f32::from(over.min(r)).powi(2))
            .max(0.)
            .sqrt());
        (self.left + inset, self.right - inset)
    }
}

/// One bar from `left` to `right` standing `height` tall, added to `builder` as its own
/// outline. Its top is rounded; a bar in a corner is cut to the rounded floor instead, and keeps
/// a flat top there since it is too short to carry both.
fn bar(builder: &mut PathBuilder, floor: &Floor, left: Pixels, right: Pixels, height: Pixels) {
    let top = floor.bottom - height;
    let (inside_left, inside_right) = floor.sides(top);
    let (left, right) = (left.max(inside_left), right.min(inside_right));
    if right <= left {
        return;
    }

    let cornered = floor.reach(left).is_some() || floor.reach(right).is_some();
    match cornered {
        true => {
            builder.move_to(point(left, top));
            builder.line_to(point(right, top));
            // a bar in a corner follows the curve along its foot
            for step in (0..=CUT).rev() {
                let x = left + (right - left) * (step as f32 / CUT as f32);
                builder.line_to(point(x, floor.at(x).max(top)));
            }
        }
        false => {
            let cap = ((right - left) / 2.).min(height).max(Pixels::ZERO);
            let radii = point(cap, cap);
            builder.move_to(point(left, top + cap));
            builder.arc_to(radii, Pixels::ZERO, false, true, point(left + cap, top));
            builder.line_to(point(right - cap, top));
            builder.arc_to(radii, Pixels::ZERO, false, true, point(right, top + cap));
            builder.line_to(point(right, floor.bottom));
            builder.line_to(point(left, floor.bottom));
        }
    }
    builder.close();
}

/// Bars that grow from the middle line, up and down at once. Drawn like the plain bars —
/// many of them, read off the smooth curve, one path — but rounded at both ends, and kept above
/// the rounded floor at the foot since a loud one reaches all the way down.
fn mirror(levels: &Levels, shape: Shape, paint: Paint, marks: Paint) -> impl IntoElement {
    let bands = levels.mixed();
    let peaks = levels.mixed_peaks();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let Some((count, width, gap)) = columns(bounds, bands.len(), shape.bar) else {
                return;
            };
            let floor = Floor::new(bounds, shape.radius);
            let middle = bounds.origin.y + bounds.size.height / 2.;
            let half = |level: f32| shape.max * level.clamp(FLOOR, 1.) / 2.;
            let marked = shape.peaks && peaks.len() >= 2;
            let mut body = PathBuilder::fill();
            let mut caps = PathBuilder::fill();
            for index in 0..count {
                let left = bounds.origin.x + (width + gap) * index as f32;
                let right = left + width;
                let at = (index as f32 + 0.5) / count as f32;
                let ground = floor.at(left).min(floor.at(right));
                let reach = half(sample(&bands, at));
                pill(
                    &mut body,
                    left,
                    right,
                    middle - reach,
                    (middle + reach).min(ground),
                );
                if marked {
                    let peak = sample(&peaks, at);
                    if peak > FLOOR * 2. {
                        let reach = half(peak);
                        cap(&mut caps, &floor, left, right, middle - reach - px(PEAK));
                        cap(&mut caps, &floor, left, right, middle + reach);
                    }
                }
            }
            painted(body, paint.background(), window);
            if marked {
                painted(caps, marks.opacity(PEAK_OPACITY).background(), window);
            }
        },
    )
    .absolute()
    .inset_0()
}

/// How many bars of `bar` pixels fit across `bounds`, how wide each is drawn and the space
/// between two. Nothing when there is no room or no curve to read them off.
fn columns(bounds: Bounds<Pixels>, bands: usize, bar: f32) -> Option<(usize, Pixels, Pixels)> {
    if bands < 2 || bounds.size.width <= px(0.) {
        return None;
    }
    let spacing = bar * BAR_GAP / BAR;
    let count = ((f32::from(bounds.size.width) + spacing) / (bar + spacing))
        .floor()
        .max(1.) as usize;
    let gap = px(spacing);
    let width = (bounds.size.width - gap * (count - 1) as f32) / count as f32;
    (width > px(0.)).then_some((count, width, gap))
}

/// A bar rounded at both ends, from `top` to `bottom`, added to `builder` as its own outline.
fn pill(builder: &mut PathBuilder, left: Pixels, right: Pixels, top: Pixels, bottom: Pixels) {
    if bottom <= top || right <= left {
        return;
    }
    let cap = ((right - left) / 2.).min((bottom - top) / 2.);
    let radii = point(cap, cap);
    builder.move_to(point(left, top + cap));
    builder.arc_to(radii, Pixels::ZERO, false, true, point(left + cap, top));
    builder.line_to(point(right - cap, top));
    builder.arc_to(radii, Pixels::ZERO, false, true, point(right, top + cap));
    builder.line_to(point(right, bottom - cap));
    builder.arc_to(radii, Pixels::ZERO, false, true, point(right - cap, bottom));
    builder.line_to(point(left + cap, bottom));
    builder.arc_to(radii, Pixels::ZERO, false, true, point(left, bottom - cap));
    builder.close();
}

/// A dot on each band's crest, riding where the bar's top would be.
/// The floor the dots stand on, edge to edge.
fn floor() -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .gap(px(GAP))
}

fn dots(levels: &Levels, max: Pixels, paint: Paint) -> Div {
    let bands = levels.mixed();
    let side = max * DOT / bands.len().max(1) as f32;

    floor()
        .items_end()
        .children(bands.into_iter().enumerate().map(|(index, level)| {
            let level = level.clamp(FLOOR, 1.);
            div()
                .id(("visualizer-dot", index))
                .flex_1()
                .flex()
                .justify_center()
                .h(max * level)
                .child(
                    div()
                        .size(side)
                        .rounded_full()
                        .bg(paint.at(level))
                        .flex_none(),
                )
        }))
}

fn glow(levels: Levels, paint: Paint) -> Div {
    div().absolute().inset_0().blur(px(GLOW)).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                wave(bounds, &levels, paint, true, true, Pixels::ZERO, window)
            },
        )
        .size_full(),
    )
}

fn line(levels: Levels, paint: Paint, bodied: bool, radius: Pixels) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| wave(bounds, &levels, paint, false, bodied, radius, window),
    )
    .absolute()
    .inset_0()
    .blur(px(LINE_BLUR))
}

fn wave(
    bounds: Bounds<Pixels>,
    levels: &Levels,
    paint: Paint,
    blurred: bool,
    bodied: bool,
    radius: Pixels,
    window: &mut Window,
) {
    let channels = [&levels.left, &levels.right];
    if channels.iter().any(|bands| bands.len() < 2) || bounds.size.width <= px(0.) {
        return;
    }
    let width = match blurred {
        true => STROKE * GLOW_STROKE,
        false => STROKE,
    };
    let inset = px(width / 2.);
    let floor = bounds.origin.y + bounds.size.height;
    let span = (bounds.size.height - inset * 2.).max(px(0.));
    let rounded = Floor::new(bounds, radius);

    for (bands, weight) in channels.into_iter().zip(CHANNELS) {
        // A band owns a column, not a fence post: its point goes at the middle of the bar the
        // same band draws.
        let step = bounds.size.width / bands.len() as f32;
        // A quiet band at the edge would put its crest below where a rounded floor curves,
        // so every crest keeps above the floor at its own column, stroke included.
        let crest = |x: Pixels, band: f32| {
            let y = floor - inset - span * band.clamp(FLOOR, 1.);
            point(x, y.min(rounded.at(x) - inset))
        };
        let mut points = Vec::with_capacity(bands.len() + 2);
        points.push(crest(bounds.origin.x, bands[0]));
        points.extend(
            bands
                .iter()
                .enumerate()
                .map(|(index, band)| crest(bounds.origin.x + step * (index as f32 + 0.5), *band)),
        );
        points.push(crest(
            bounds.origin.x + bounds.size.width,
            bands[bands.len() - 1],
        ));

        let weight = match blurred {
            true => weight * GLOW_OPACITY,
            false => weight,
        };
        if !blurred && bodied {
            fill(&points, &rounded, paint, weight, window);
        }
        stroke(
            &points,
            px(width),
            paint.opacity(OPACITY * 2. * weight).background(),
            window,
        );
    }
}

fn trace(builder: &mut PathBuilder, points: &[Point<Pixels>]) {
    builder.move_to(points[0]);
    // A Catmull-Rom spline as cubics. Unlike a quadratic through the midpoints it passes
    // through every point, so a crest lands on its band rather than shy of it.
    for index in 0..points.len() - 1 {
        let before = points[index.saturating_sub(1)];
        let from = points[index];
        let to = points[index + 1];
        let after = points[(index + 2).min(points.len() - 1)];
        builder.cubic_bezier_to(
            to,
            point(
                from.x + (to.x - before.x) / 6.,
                from.y + (to.y - before.y) / 6.,
            ),
            point(
                to.x - (after.x - from.x) / 6.,
                to.y - (after.y - from.y) / 6.,
            ),
        );
    }
}

fn stroke(points: &[Point<Pixels>], width: Pixels, background: Background, window: &mut Window) {
    let mut builder = PathBuilder::stroke(width);
    trace(&mut builder, points);
    painted(builder, background, window);
}

fn fill(points: &[Point<Pixels>], floor: &Floor, paint: Paint, weight: f32, window: &mut Window) {
    if points.is_empty() {
        return;
    }
    let mut builder = PathBuilder::fill();
    trace(&mut builder, points);
    // down the right side and round its corner, along the foot, round the other corner
    let r = floor.radius;
    for step in 0..=CUT {
        let x = floor.right - r * (step as f32 / CUT as f32);
        builder.line_to(point(x, floor.at(x)));
    }
    for step in 0..=CUT {
        let x = floor.left + r * (1. - step as f32 / CUT as f32);
        builder.line_to(point(x, floor.at(x)));
    }
    builder.close();
    painted(
        builder,
        paint.faded(FILL * weight, FILL * FILL_BASE * weight),
        window,
    );
}

/// Keeps `tint` readable on `behind` by pushing its lightness away from the surface's.
fn legible(tint: Hsla, behind: Hsla) -> Hsla {
    let distance = (tint.l - behind.l).abs();
    if distance >= CONTRAST {
        return tint;
    }
    let lighter = behind.l + CONTRAST;
    let darker = behind.l - CONTRAST;
    let l = match lighter <= 1. {
        true => lighter,
        false => darker.max(0.),
    };
    Hsla { l, ..tint }
}
