import { useEffect, useRef } from 'react';
import { TextInput, View } from 'react-native';
import { Gesture, GestureDetector, GestureHandlerRootView } from 'react-native-gesture-handler';
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedProps,
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

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
/** How long the time bubble takes to fade once the finger lifts; it comes up at once. */
const TIP_MS = 80;
const TIP_RISE = 6;
/** The spring of the thumb growing under a finger and settling back, without overshoot. */
const GROW_SPRING = { damping: 22, stiffness: 320 };


/** A read-only text field: its text can be set from the UI thread, which a Text's cannot. */
const AnimatedText = Animated.createAnimatedComponent(TextInput);

/** `ms` as m:ss, on the UI thread; the same format as `clock` in `../format`. */
function time(ms: number): string {
  'worklet';
  const whole = Math.max(0, Math.floor(ms / 1000));
  const seconds = whole % 60;
  return `${Math.floor(whole / 60)}:${seconds < 10 ? '0' : ''}${seconds}`;
}

/**
 * A draggable progress bar. The finger, the thumb and the time bubble, its text included, all run
 * on the UI thread through Gesture Handler and Reanimated, so a drag stays smooth however busy
 * JS is. Between the core's position reports it glides, and
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

  const progress = useSharedValue(0);
  const width = useSharedValue(0);
  const total = useSharedValue(max);
  const held = useSharedValue(0);
  // The bubble's own fade, started only on touch and release, never per frame.
  const tip = useSharedValue(0);
  // Where the finger put the thumb: what a release seeks to, even while the thumb still slides.
  const target = useSharedValue(0);
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

  // The JS side of a drag: `onScrub`, at most once a second.
  const scrubbed = (ms: number | null) => onScrub?.(ms);
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
      target.value = fraction;
      // The thumb and the bubble are under the finger in the same frame as the touch.
      progress.value = fraction;
      held.value = withSpring(1, GROW_SPRING);
      tip.value = 1;
      const ms = fraction * total.value;
      second.value = Math.floor(ms / 1000);
      scheduleOnRN(scrubbed, ms);
    })
    .onUpdate((event) => {
      const fraction = width.value > 0 ? Math.min(1, Math.max(0, event.x / width.value)) : 0;
      target.value = fraction;
      progress.value = fraction;
      const ms = fraction * total.value;
      const now = Math.floor(ms / 1000);
      if (now !== second.value) {
        second.value = now;
        scheduleOnRN(scrubbed, ms);
      }
    })
    // A tap may end without the pan ever activating, so the seek happens on every release.
    .onFinalize(() => {
      dragging.value = false;
      second.value = -1;
      held.value = withSpring(0, GROW_SPRING);
      tip.value = withTiming(0, { duration: TIP_MS });
      scheduleOnRN(released, target.value * total.value);
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
      opacity: tip.value,
      transform: [{ translateX: x }, { translateY: (1 - tip.value) * TIP_RISE }],
    };
  });

  const tipText = useAnimatedProps(() => {
    const text = time(progress.value * total.value);
    // `text` is not a public prop of TextInput, but it is what the native view draws.
    return { text, defaultValue: text } as Record<string, string>;
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
            <AnimatedText
              editable={false}
              underlineColorAndroid="transparent"
              style={styles.tipText}
              animatedProps={tipText}
            />
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
    textAlign: 'center',
    padding: 0,
  },
}));
