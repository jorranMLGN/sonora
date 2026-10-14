/**
 * The app's motion and touch feedback in one place: the springs everything moves with, the
 * durations of its fades, and the haptics, so the whole app feels of one hand.
 *
 * The springs follow Apple's: a perceptual duration and a damping ratio, where 1 settles
 * without overshoot. Haptics follow the Human Interface Guidelines: a selection tick for
 * choosing between values, a light impact for playback, a medium one when a menu opens, and a
 * notification for an outcome. They stay quiet when the listener turned them off.
 */
import * as Haptics from 'expo-haptics';

import { useSettings } from './settings';

/** The springs, from a finger's quick press to a sheet's slow settle. */
export const springs = {
  /** A press letting go: quick and without overshoot. */
  press: { duration: 220, dampingRatio: 1 },
  /** Chips, toggles and anything that snaps between two places. */
  snappy: { duration: 350, dampingRatio: 0.82 },
  /** Larger moves, such as the player's artwork shrinking on pause. */
  smooth: { duration: 500, dampingRatio: 1 },
  /** A small celebration: a heart filling, the play button changing glyph. */
  bouncy: { duration: 450, dampingRatio: 0.55 },
} as const;

/** How long the plain fades take. */
export const durations = {
  /** A finger landing on something: as good as at once. */
  pressIn: 80,
  /** An image or a row fading in. */
  fade: 200,
  /** One list giving way to another when a chip changes what is shown. */
  swap: 240,
  /** Blurred artwork behind the player changing with the track. */
  backdrop: 450,
} as const;

/** How far things shrink under a finger. Rows barely move; tiles and buttons a little more. */
export const pressScale = {
  row: 0.985,
  tile: 0.96,
  chip: 0.94,
  button: 0.88,
} as const;

/** Whether the listener wants haptics; read outside render, where the taps happen. */
function enabled(): boolean {
  return useSettings.getState().haptics;
}

/** The taps, each named for the moment it belongs to. Failures (no Taptic Engine) are ignored. */
export const haptic = {
  /** Choosing a chip, a sort or a tab. */
  select(): void {
    if (enabled()) Haptics.selectionAsync().catch(() => undefined);
  },
  /** Play, pause, skip, and the player settling open. */
  light(): void {
    if (enabled()) Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => undefined);
  },
  /** A long press opening a menu. */
  medium(): void {
    if (enabled()) Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium).catch(() => undefined);
  },
  /** Something saved to the library. */
  success(): void {
    if (enabled()) {
      Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => undefined);
    }
  },
  /** An error the listener is about to read. */
  error(): void {
    if (enabled()) {
      Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error).catch(() => undefined);
    }
  },
};
