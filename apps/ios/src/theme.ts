/** Dark theme tokens. Screens read colours, radii and spacing from here, never literals. */
export const colors = {
  background: '#0d0d0f',
  surface: '#17171b',
  surfaceRaised: '#222228',
  border: '#2c2c33',
  text: '#f4f4f6',
  textDim: '#9a9aa5',
  accent: '#ff5500',
  /** The accent as a soft wash, for the glows and rings behind it. */
  accentSoft: 'rgba(255, 85, 0, 0.16)',
  accentFaint: 'rgba(255, 85, 0, 0.06)',
  success: '#3ddc84',
  onAccent: '#ffffff',
  danger: '#ff5a5f',
  overlay: 'rgba(0, 0, 0, 0.6)',
  placeholder: '#2a2a31',
  transparent: 'transparent',
} as const;

export const radii = {
  small: 4,
  medium: 8,
  large: 14,
  pill: 999,
} as const;

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
} as const;

export const typography = {
  caption: 12,
  body: 15,
  title: 17,
  heading: 22,
  display: 28,
} as const;
