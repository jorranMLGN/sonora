import { useEffect } from 'react';
import { View } from 'react-native';
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withDelay,
  withRepeat,
  withSequence,
  withTiming,
} from 'react-native-reanimated';

import { useReduceMotion } from '../settings';
import { useStore } from '../store';
import { makeStyles } from '../styles';
import { sizes, spacing } from '../theme';

/** Each bar's lowest and highest share of the mark's height, and how long one rise takes. */
const BARS = [
  { low: 0.3, high: 1, ms: 420, delay: 0 },
  { low: 0.5, high: 0.75, ms: 360, delay: 140 },
  { low: 0.25, high: 0.9, ms: 480, delay: 70 },
] as const;
/** How tall the bars rest while playback is paused. */
const PAUSED = 0.3;

/**
 * The now-playing mark: three bars that bounce while the player plays and rest low while it is
 * paused, drawn over the artwork of the row that is current. Reduce Motion keeps them still.
 */
export function Equalizer() {
  const styles = useStyles();
  const playing = useStore((s) => s.player?.status === 'playing');
  const still = useReduceMotion();
  return (
    <View style={styles.mark} pointerEvents="none">
      {BARS.map((bar, index) => (
        <Bar key={index} {...bar} moving={playing && !still} />
      ))}
    </View>
  );
}

function Bar({
  low,
  high,
  ms,
  delay,
  moving,
}: {
  low: number;
  high: number;
  ms: number;
  delay: number;
  moving: boolean;
}) {
  const styles = useStyles();
  const level = useSharedValue(PAUSED);
  useEffect(() => {
    const ease = Easing.inOut(Easing.quad);
    level.value = moving
      ? withDelay(
          delay,
          withRepeat(
            withSequence(
              withTiming(high, { duration: ms, easing: ease }),
              withTiming(low, { duration: ms, easing: ease }),
            ),
            -1,
          ),
        )
      : withTiming(PAUSED, { duration: ms });
    return () => cancelAnimation(level);
  }, [moving, low, high, ms, delay, level]);
  const animated = useAnimatedStyle(() => ({ transform: [{ scaleY: level.value }] }));
  return <Animated.View style={[styles.bar, animated]} />;
}

const useStyles = makeStyles(({ colors, radii }) => ({
  mark: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: spacing.xs / 2,
    height: sizes.equalizerHeight,
  },
  bar: {
    width: sizes.equalizerBar,
    height: sizes.equalizerHeight,
    borderRadius: radii.pill,
    backgroundColor: colors.accent,
    transformOrigin: 'bottom',
  },
}));
