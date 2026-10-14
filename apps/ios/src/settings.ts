import { useMemo } from 'react';
import { AccessibilityInfo, LayoutAnimation } from 'react-native';
import { create } from 'zustand';

import type { LibraryPart } from './bridge';
import { preference, setPreference } from './preferences';
import { defaultLook, type Look } from './theme';

/** The page behind everything: the default dark, true black for OLED screens, or a softer dim. */
export type Background = Look['background'];
/** How round corners are drawn across the app. */
export type Corners = Look['corners'];
export type TextSize = Look['textSize'];
/** The shape of the artwork on the full player. */
export type ArtworkShape = 'square' | 'rounded' | 'circle';
/** How the library draws albums, artists and playlists: rows, or a grid of covers. */
export type LibraryLayout = 'list' | 'grid';
/** The order the library lists a part in: as the service gives it, by title, or by creator. */
export type LibrarySort = 'recent' | 'title' | 'creator';

/** Everything the listener can set, each with its default below. */
export type Settings = {
  /** A hex colour, one of `accents` unless set some other way. */
  accent: string;
  background: Background;
  corners: Corners;
  textSize: TextSize;
  artwork: ArtworkShape;
  /** Calms decorative animations; the system's Reduce Motion does the same on its own. */
  reduceMotion: boolean;
  /** Small taps under the finger when something is chosen, saved or played. */
  haptics: boolean;
  hideExplicit: boolean;
  /** Shows the service a track, album, artist or playlist comes from beside its name. */
  providerBadges: boolean;
  /** The slugs of the providers whose shelves Home leaves out. */
  homeHidden: string[];
  /** The part the library opens on. */
  libraryTab: LibraryPart;
  libraryLayout: LibraryLayout;
  librarySort: LibrarySort;
};

export type SettingKey = keyof Settings;

export const DEFAULTS: Settings = {
  ...defaultLook,
  artwork: 'square',
  reduceMotion: false,
  haptics: true,
  hideExplicit: false,
  providerBadges: false,
  homeHidden: [],
  libraryTab: 'tracks',
  libraryLayout: 'list',
  librarySort: 'recent',
};

/** The values a setting with a fixed set of choices takes, in the order Settings offers them. */
export const CHOICES = {
  background: ['dark', 'black', 'dim'],
  corners: ['sharp', 'default', 'round'],
  textSize: ['small', 'default', 'large'],
  artwork: ['square', 'rounded', 'circle'],
  libraryTab: ['tracks', 'albums', 'artists', 'playlists'],
  libraryLayout: ['list', 'grid'],
  librarySort: ['recent', 'title', 'creator'],
} as const satisfies { [K in SettingKey]?: readonly Settings[K][] };

/** What every setting's key in the user defaults starts with, after the module's `sonora.`. */
const PREFIX = 'settings.';

type SettingsState = Settings & {
  /** Whether iOS asks for reduced motion; kept apart from the listener's own switch. */
  systemReduceMotion: boolean;
  set<K extends SettingKey>(key: K, value: Settings[K]): void;
  /** Puts every setting back to its default. */
  reset(): void;
};

/** The settings, read once at launch and written through on every change. */
export const useSettings = create<SettingsState>((set, get) => ({
  ...load(),
  systemReduceMotion: false,
  set: (key, value) => {
    ease(get());
    setPreference(PREFIX + key, encode(value));
    set({ [key]: value } as Partial<Settings>);
  },
  reset: () => {
    ease(get());
    (Object.keys(DEFAULTS) as SettingKey[]).forEach((key) => setPreference(PREFIX + key, null));
    set({ ...DEFAULTS });
  },
}));

AccessibilityInfo.isReduceMotionEnabled()
  .then((on) => useSettings.setState({ systemReduceMotion: on }))
  .catch(() => undefined);
AccessibilityInfo.addEventListener('reduceMotionChanged', (on) =>
  useSettings.setState({ systemReduceMotion: on }),
);

/** Whether decorative animations stay still, by the listener's choice or the system's. */
export function useReduceMotion(): boolean {
  return useSettings((s) => s.reduceMotion || s.systemReduceMotion);
}

/** Leaves explicit tracks out when the listener asked to hide them. */
export function visible<T extends { explicit: boolean }>(tracks: T[], hideExplicit: boolean): T[] {
  return hideExplicit ? tracks.filter((track) => !track.explicit) : tracks;
}

/** `visible` for a component: the tracks it should draw and play, kept while nothing changes. */
export function useVisible<T extends { explicit: boolean }>(tracks: T[]): T[] {
  const hide = useSettings((s) => s.hideExplicit);
  return useMemo(() => visible(tracks, hide), [tracks, hide]);
}

/** Lets the layout a change moves glide into place, unless motion is reduced. */
function ease(state: SettingsState): void {
  if (state.reduceMotion || state.systemReduceMotion) return;
  LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
}

/** Every setting as stored, with anything missing or no longer valid at its default. */
function load(): Settings {
  const out = { ...DEFAULTS };
  (Object.keys(DEFAULTS) as SettingKey[]).forEach((key) => {
    const raw = preference(PREFIX + key);
    if (raw !== null) (out as Record<SettingKey, unknown>)[key] = decode(key, raw);
  });
  return out;
}

function encode(value: Settings[SettingKey]): string {
  if (Array.isArray(value)) return value.join(',');
  return String(value);
}

/** Reads a stored string back as the type of the setting's default. */
function decode(key: SettingKey, raw: string): Settings[SettingKey] {
  const fallback = DEFAULTS[key];
  if (typeof fallback === 'boolean') return raw === 'true';
  if (Array.isArray(fallback)) return raw.split(',').filter((part) => part !== '');
  if (key === 'accent') return /^#[0-9a-f]{6}$/i.test(raw) ? raw : fallback;
  const choices: readonly string[] | undefined = (CHOICES as Record<string, readonly string[]>)[key];
  return choices === undefined || choices.includes(raw) ? raw : fallback;
}
