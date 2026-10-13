/**
 * Theme tokens. Every colour, radius and type size lives here; screens read them through
 * `useTheme` and `makeStyles` in `./styles`, which build the look the listener chose in Settings.
 * The plain `colors`, `radii` and `typography` below are the default look, for the rare code
 * that runs outside a component.
 */

/** The accents Settings offers, the first being the default. `name` is its Fluent key suffix. */
export const accents = [
  { name: 'orange', color: '#ff5500' },
  { name: 'red', color: '#ff3b5c' },
  { name: 'pink', color: '#ff4fa3' },
  { name: 'purple', color: '#a66bff' },
  { name: 'blue', color: '#3d8bff' },
  { name: 'teal', color: '#1fc8b9' },
  { name: 'green', color: '#30d158' },
  { name: 'yellow', color: '#ffc21a' },
] as const;

/** The colours a background style sets: the page and the surfaces stacked on it. */
const backgrounds = {
  dark: {
    background: '#0d0d0f',
    surface: '#17171b',
    surfaceRaised: '#222228',
    border: '#2c2c33',
    placeholder: '#2a2a31',
  },
  black: {
    background: '#000000',
    surface: '#0e0e10',
    surfaceRaised: '#1a1a1e',
    border: '#242429',
    placeholder: '#1f1f24',
  },
  dim: {
    background: '#1c1d23',
    surface: '#26272f',
    surfaceRaised: '#31323b',
    border: '#3b3c46',
    placeholder: '#383943',
  },
} as const;

/** Corner radii per roundness. `pill` stays round in every one. */
const corners = {
  sharp: { small: 0, medium: 2, large: 4, pill: 999 },
  default: { small: 4, medium: 8, large: 14, pill: 999 },
  round: { small: 8, medium: 14, large: 22, pill: 999 },
} as const;

/** How much each text size scales the type ramp. */
const textScales = { small: 0.9, default: 1, large: 1.15 } as const;

/** The type ramp at the default text size. */
const typeRamp = {
  caption: 12,
  body: 15,
  title: 17,
  heading: 22,
  display: 28,
} as const;

/** Text on the accent: white, or near black on a light accent such as yellow. */
const onAccentLight = '#ffffff';
const onAccentDark = '#0d0d0f';
/** The relative luminance above which an accent takes dark text. */
const LIGHT_ACCENT = 0.5;

/** What `buildTheme` takes: the appearance settings. */
export type Look = {
  accent: string;
  background: keyof typeof backgrounds;
  corners: keyof typeof corners;
  textSize: keyof typeof textScales;
};

export type Colors = {
  background: string;
  surface: string;
  surfaceRaised: string;
  border: string;
  text: string;
  textDim: string;
  accent: string;
  /** The accent as a soft wash, for the glows and rings behind it. */
  accentSoft: string;
  accentFaint: string;
  success: string;
  onAccent: string;
  danger: string;
  overlay: string;
  placeholder: string;
  transparent: string;
};

export type Radii = { small: number; medium: number; large: number; pill: number };
export type Typography = { [K in keyof typeof typeRamp]: number };

/** One complete look. Built once per combination of settings and shared, so it can key caches. */
export type Theme = { colors: Colors; radii: Radii; typography: Typography };

/** Builds the theme for a look; `./styles` caches it, so call that rather than this. */
export function buildTheme(look: Look): Theme {
  const scale = textScales[look.textSize];
  const typography = Object.fromEntries(
    Object.entries(typeRamp).map(([name, size]) => [name, Math.round(size * scale)]),
  ) as Typography;
  return {
    colors: {
      ...backgrounds[look.background],
      text: '#f4f4f6',
      textDim: '#9a9aa5',
      accent: look.accent,
      accentSoft: wash(look.accent, 0.16),
      accentFaint: wash(look.accent, 0.06),
      success: '#3ddc84',
      onAccent: luminance(look.accent) > LIGHT_ACCENT ? onAccentDark : onAccentLight,
      danger: '#ff5a5f',
      overlay: 'rgba(0, 0, 0, 0.6)',
      transparent: 'transparent',
    },
    radii: { ...corners[look.corners] },
    typography,
  };
}

/** The look of a fresh install. */
export const defaultLook: Look = {
  accent: accents[0].color,
  background: 'dark',
  corners: 'default',
  textSize: 'default',
};

const fallback = buildTheme(defaultLook);

/** The default look's colours, for code outside a component; screens use `useTheme`. */
export const colors: Colors = fallback.colors;
export const radii: Radii = fallback.radii;
export const typography: Typography = fallback.typography;

export const spacing = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
  xxl: 32,
} as const;

export const sizes = {
  rowArtwork: 48,
  miniArtwork: 40,
  tileArtwork: 140,
  headerArtwork: 200,
  playerArtwork: 300,
  tabBar: 52,
  miniPlayer: 60,
  icon: 24,
  iconLarge: 32,
  toggle: 64,
  scrubberTrack: 4,
  scrubberThumb: 14,
  scrubberHit: 32,
  hairline: 1,
  /** The intro: its equalizer mark, the rings that pulse out of it, the glow behind a page. */
  introMark: 120,
  introBar: 14,
  introRing: 160,
  introGlow: 420,
  introCheck: 96,
  dot: 8,
  dotActive: 24,
  button: 52,
  /** A colour swatch in Settings, and the ring around the chosen one. */
  swatch: 32,
  swatchRing: 2,
  /** The inset of a segmented control's sliding thumb. */
  segmentInset: 2,
} as const;

/** `hex` (`#rrggbb`) at `alpha`, as an rgba string. */
function wash(hex: string, alpha: number): string {
  const [r, g, b] = channels(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** The WCAG relative luminance of `hex`, from 0 for black to 1 for white. */
function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((value) => {
    const c = value / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function channels(hex: string): [number, number, number] {
  const value = parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}
