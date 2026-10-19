import { StyleSheet } from 'react-native';

import { useSettings } from './settings';
import { buildTheme, type Look, type Theme } from './theme';

/** Every theme built so far, by look, so a look always maps to the same object. */
const themes = new Map<string, Theme>();

/** The theme for `look`, built once and shared. */
export function themeFor(look: Look): Theme {
  const key = `${look.accent}|${look.background}|${look.corners}|${look.textSize}`;
  let theme = themes.get(key);
  if (theme === undefined) {
    theme = buildTheme(look);
    themes.set(key, theme);
  }
  return theme;
}

/** The theme the listener chose; a component using it draws again when a setting changes it. */
export function useTheme(): Theme {
  const accent = useSettings((s) => s.accent);
  const background = useSettings((s) => s.background);
  const corners = useSettings((s) => s.corners);
  const textSize = useSettings((s) => s.textSize);
  return themeFor({ accent, background, corners, textSize });
}

/**
 * Turns a function from the theme to a style sheet into a hook giving that sheet for the active
 * theme. A sheet is built once per theme, so calling the hook on every render costs a lookup.
 */
export function makeStyles<T extends StyleSheet.NamedStyles<T> | StyleSheet.NamedStyles<any>>(
  build: (theme: Theme) => T & StyleSheet.NamedStyles<any>,
): () => T {
  const sheets = new WeakMap<Theme, T>();
  return function useStyles() {
    const theme = useTheme();
    let sheet = sheets.get(theme);
    if (sheet === undefined) {
      sheet = StyleSheet.create(build(theme));
      sheets.set(theme, sheet);
    }
    return sheet;
  };
}
