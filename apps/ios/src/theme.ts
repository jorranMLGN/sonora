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
    glass: 'rgba(13, 13, 15, 0.55)',
  },
  black: {
    background: '#000000',
    surface: '#0e0e10',
    surfaceRaised: '#1a1a1e',
    border: '#242429',
    placeholder: '#1f1f24',
    glass: 'rgba(0, 0, 0, 0.6)',
  },
  dim: {
    background: '#1c1d23',
    surface: '#26272f',
    surfaceRaised: '#31323b',
    border: '#3b3c46',
    placeholder: '#383943',
    glass: 'rgba(28, 29, 35, 0.55)',
  },
} as const;

/**
 * The gradients a missing cover is drawn with, dark enough for white type, picked by a hash of
 * the item so the same playlist always gets the same one.
 */
const artworkWashes = [
  ['#4a3a8c', '#1d1838'],
  ['#1f5f70', '#0f2730'],
  ['#7a3352', '#2a1420'],
  ['#36633a', '#152414'],
  ['#7a5a26', '#2a1f0f'],
  ['#2f4c8a', '#121b33'],
  ['#5e3878', '#22142c'],
  ['#2a6a64', '#0e2523'],
] as const;

/** Each service's own colour, for the dot on its chip. */
const brands: Record<string, string> = {
  spotify: '#1ed760',
  youtube: '#ff0033',
  soundcloud: '#ff5500',
};

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
  /** A small tag, such as the service on a row. */
  tag: 10,
  caption: 12,
  body: 15,
  /** A row's subtitle and a chip's label. */
  label: 13,
  /** A row's title. */
  row: 16,
  title: 17,
  heading: 22,
  display: 28,
  /** The large title a page opens with, before it collapses into the bar. */
  hero: 34,
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
  /** The tint over a blurred bar, so what scrolls under it stays legible. */
  glass: string;
  /** The wash a row takes while a finger is on it. */
  pressed: string;
  /** The shade over blurred artwork behind the player and a page's header. */
  scrim: string;
  /** The white of a glyph drawn on a cover. */
  onArtwork: string;
};

export type Radii = { small: number; medium: number; large: number; pill: number };
export type Typography = { [K in keyof typeof typeRamp]: number };

/** One complete look. Built once per combination of settings and shared, so it can key caches. */
export type Theme = {
  colors: Colors;
  radii: Radii;
  typography: Typography;
  /** The gradients a missing cover is drawn with; see `artworkWash`. */
  washes: readonly (readonly [string, string])[];
  /** Each service's colour by slug; `brand` reads it with a fallback. */
  brands: Record<string, string>;
};

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
      pressed: 'rgba(255, 255, 255, 0.07)',
      scrim: 'rgba(0, 0, 0, 0.35)',
      onArtwork: 'rgba(255, 255, 255, 0.55)',
    },
    radii: { ...corners[look.corners] },
    typography,
    washes: artworkWashes,
    brands,
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
  rowArtwork: 52,
  miniArtwork: 42,
  tileArtwork: 148,
  headerArtwork: 224,
  playerArtwork: 300,
  tabBar: 52,
  miniPlayer: 62,
  /** The bar at the top of a page, under the status bar. */
  topBar: 44,
  /** A filter chip, fixed so a page's header has a known height. */
  chip: 34,
  /** The search field at the top of Search. */
  field: 40,
  /** The glyph on a missing cover, as a share of the cover's side. */
  glyphShare: 0.38,
  /** The bars of the now-playing mark over a row's artwork. */
  equalizerBar: 3,
  equalizerHeight: 14,
  /** The grabber at the top of the player sheet. */
  grabberWidth: 36,
  grabberHeight: 5,
  /** A round button beside a page's play button. */
  roundButton: 48,
  icon: 24,
  /** A glyph beside a label, such as the sort button's. */
  iconSmall: 16,
  iconLarge: 32,
  toggle: 64,
  scrubberTrack: 4,
  scrubberThumb: 14,
  /** The scrubber's touch area: tall, so a drag lands on it rather than on the page. */
  scrubberHit: 48,
  /** The time bubble over the scrubber's thumb while it is dragged. */
  scrubberTipWidth: 64,
  scrubberTipGap: 8,
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

/** The gradient a missing cover with `seed` is drawn with, the same one every time. */
export function artworkWash(theme: Theme, seed: string): readonly [string, string] {
  // FNV-1a: cheap, and spreads short titles evenly over the washes.
  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return theme.washes[(hash >>> 0) % theme.washes.length];
}

/** The colour of the service `slug`, or the accent for one without its own. */
export function brand(theme: Theme, slug: string): string {
  return theme.brands[slug] ?? theme.colors.accent;
}

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
