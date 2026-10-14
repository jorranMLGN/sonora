import { useEffect, useRef, useState } from 'react';
import { Text, View } from 'react-native';
import { Gesture, GestureDetector, GestureHandlerRootView } from 'react-native-gesture-handler';
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { clock } from '../format';
import { useReduceMotion } from '../settings';
import { makeStyles } from '../styles';
import { sizes } from '../theme';

/** How often the core reports the position (`TICK` in `crates/mobile/src/player.rs`). */
const TICK_MS = 500;
/** A jump larger than this between reports is a seek or a new track, not playback. */
const JUMP_MS = TICK_MS * 3;
/** How long a seek waits for the core to report a position near it before giving up. */
const SETTLE_MS = 3000;
/** How close a reported position must come to a seek to count as having arrived. */
const ARRIVED_MS = 1500;
/** How much the thumb grows under a finger. */
const DRAG_SCALE = 1.6;
/** How long the time bubble takes to come up over the thumb, and how far it rises as it does. */
const TIP_MS = 140;
const TIP_RISE = 6;
/** The spring of the thumb growing under a finger and settling back. */
const GROW_SPRING = { damping: 14, stiffness: 260 };

/**
 * A draggable progress bar. The finger, the thumb and the time bubble all run on the UI thread
 * through Gesture Handler and Reanimated, so a drag stays smooth however busy JS is; only the
 * bubble's text crosses to JS, once a second. Between the core's position reports it glides, and
 * after a seek it holds the new place until the core catches up, so the thumb never jumps back.
 * `onScrub` hears the position while dragging (once a second) and null after.
 */
export function Scrubber({
  value,
  max,
  playing,
  onSeek,
  onScrub,
}: {
  value: number;
  max: number;
  playing: boolean;
  onSeek: (ms: number) => void;
  onScrub?: (ms: number | null) => void;
}) {
  const styles = useStyles();
  const still = useReduceMotion();
  const [label, setLabel] = useState('0:00');

  const progress = useSharedValue(0);
  const width = useSharedValue(0);
  const total = useSharedValue(max);
  const held = useSharedValue(0);
  const dragging = useSharedValue(false);
  const second = useSharedValue(-1);

  useEffect(() => {
    total.value = max;
  }, [max, total]);

  const pending = useRef<{ ms: number; since: number } | null>(null);
  const shown = useRef(0);
  const fractionOf = (ms: number) => (max > 0 ? Math.max(0, Math.min(1, ms / max)) : 0);

  // Follow the core's reports: glide over a tick while playing, snap on a seek or a new track.
  useEffect(() => {
    if (dragging.value) return;
    const wait = pending.current;
    if (wait !== null) {
      const arrived = Math.abs(value - wait.ms) < ARRIVED_MS;
      if (!arrived && Date.now() - wait.since < SETTLE_MS) return;
      pending.current = null;
    }
    const jump = Math.abs(value - shown.current) > JUMP_MS;
    shown.current = value;
    cancelAnimation(progress);
    if (!playing || jump || still) {
      progress.value = fractionOf(value);
      return;
    }
    // Aim one tick ahead, where playback will be when the next report lands.
    progress.value = withTiming(fractionOf(value + TICK_MS), {
      duration: TICK_MS,
      easing: Easing.linear,
    });
    // `fractionOf` reads `max`, which is a dependency.
  }, [value, max, playing, still, progress, dragging]);

  // The JS side of a drag: the bubble's text and `onScrub`, at most once a second.
  const scrubbed = (ms: number | null) => {
    if (ms !== null) setLabel(clock(ms));
    onScrub?.(ms);
  };
  const released = (ms: number) => {
    pending.current = { ms, since: Date.now() };
    shown.current = ms;
    onSeek(ms);
    onScrub?.(null);
  };

  const pan = Gesture.Pan()
    // Take the bar at the first touch, so neither the list nor the sheet gets the drag.
    .minDistance(0)
    .shouldCancelWhenOutside(false)
    .onBegin((event) => {
      dragging.value = true;
      cancelAnimation(progress);
      const fraction = width.value > 0 ? Math.min(1, Math.max(0, event.x / width.value)) : 0;
      progress.value = fraction;
      held.value = withSpring(1, GROW_SPRING);
      const ms = fraction * total.value;
      second.value = Math.floor(ms / 1000);
      scheduleOnRN(scrubbed, ms);
    })
    .onUpdate((event) => {
      const fraction = width.value > 0 ? Math.min(1, Math.max(0, event.x / width.value)) : 0;
      progress.value = fraction;
      const ms = fraction * total.value;
      const now = Math.floor(ms / 1000);
      if (now !== second.value) {
        second.value = now;
        scheduleOnRN(scrubbed, ms);
      }
    })
    .onEnd(() => {
      scheduleOnRN(released, progress.value * total.value);
    })
    .onFinalize((_event, success) => {
      dragging.value = false;
      second.value = -1;
      held.value = withSpring(0, GROW_SPRING);
      if (!success) scheduleOnRN(scrubbed, null);
    });

  const fillStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: (progress.value - 1) * width.value }],
  }));
  const thumbStyle = useAnimatedStyle(() => ({
    transform: [
      { translateX: progress.value * width.value - sizes.scrubberThumb / 2 },
      { scale: 1 + held.value * (DRAG_SCALE - 1) },
    ],
  }));
  const tipStyle = useAnimatedStyle(() => {
    // The bubble rides over the thumb, kept inside the bar's width at either end.
    const room = Math.max(0, width.value - sizes.scrubberTipWidth);
    const x = Math.min(
      room,
      Math.max(0, progress.value * width.value - sizes.scrubberTipWidth / 2),
    );
    return {
      opacity: withTiming(held.value > 0.5 ? 1 : 0, { duration: TIP_MS }),
      transform: [{ translateX: x }, { translateY: (1 - Math.min(1, held.value)) * TIP_RISE }],
    };
  });

  return (
    <GestureHandlerRootView style={styles.root}>
      <GestureDetector gesture={pan}>
        <View
          style={styles.hit}
          onLayout={(event) => {
            width.value = event.nativeEvent.layout.width;
          }}
        >
          <View style={styles.track} pointerEvents="none">
            <Animated.View style={[styles.fill, fillStyle]} />
          </View>
          <Animated.View pointerEvents="none" style={[styles.thumb, thumbStyle]} />
          <Animated.View pointerEvents="none" style={[styles.tip, tipStyle]}>
            <Text style={styles.tipText}>{label}</Text>
          </Animated.View>
        </View>
      </GestureDetector>
    </GestureHandlerRootView>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  // Its own gesture root: the player is a native modal, outside any root the app sets up.
  root: { flexGrow: 0 },
  hit: { height: sizes.scrubberHit, justifyContent: 'center' },
  track: {
    height: sizes.scrubberTrack,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
    overflow: 'hidden',
  },
  fill: { width: '100%', height: '100%', backgroundColor: colors.accent },
  thumb: {
    position: 'absolute',
    left: 0,
    width: sizes.scrubberThumb,
    height: sizes.scrubberThumb,
    borderRadius: radii.pill,
    backgroundColor: colors.text,
  },
  tip: {
    position: 'absolute',
    left: 0,
    bottom: sizes.scrubberHit / 2 + sizes.scrubberThumb / 2 + sizes.scrubberTipGap,
    width: sizes.scrubberTipWidth,
    alignItems: 'center',
    paddingVertical: sizes.scrubberTipGap / 2,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
  },
  tipText: {
    color: colors.text,
    fontSize: typography.caption,
    fontWeight: '600',
    fontVariant: ['tabular-nums'],
  },
}));
