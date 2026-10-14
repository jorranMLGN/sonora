import { type ReactNode } from 'react';
import { type Insets, Pressable, type StyleProp, type ViewStyle } from 'react-native';
import Animated, {
  interpolate,
  interpolateColor,
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';

import { durations, haptic, pressScale, springs } from '../motion';
import { useReduceMotion } from '../settings';
import { useTheme } from '../styles';

const AnimatedPressable = Animated.createAnimatedComponent(Pressable);

/** How long a finger rests before a press counts as long, a little under the system's. */
const LONG_PRESS_MS = 350;

/**
 * A pressable that answers the finger: it shrinks a touch while held and springs back on
 * release, and with `highlight` takes a soft wash too, as a row does. A long press taps the
 * Taptic Engine before `onLongPress` runs, since it always opens a menu. Reduce Motion keeps the
 * wash and drops the shrink.
 */
export function Press({
  children,
  onPress,
  onLongPress,
  scale = pressScale.row,
  highlight = false,
  disabled = false,
  hitSlop,
  style,
  accessibilityLabel,
}: {
  children: ReactNode;
  onPress?: () => void;
  onLongPress?: () => void;
  /** How small the pressable gets under the finger; one of `pressScale` reads best. */
  scale?: number;
  highlight?: boolean;
  disabled?: boolean;
  hitSlop?: number | Insets;
  style?: StyleProp<ViewStyle>;
  accessibilityLabel?: string;
}) {
  const { colors } = useTheme();
  const still = useReduceMotion();
  const down = useSharedValue(0);
  const target = still ? 1 : scale;
  const wash = colors.pressed;
  const clear = colors.transparent;
  const animated = useAnimatedStyle(() => {
    const transform = [{ scale: interpolate(down.value, [0, 1], [1, target]) }];
    return highlight
      ? { transform, backgroundColor: interpolateColor(down.value, [0, 1], [clear, wash]) }
      : { transform };
  });
  return (
    <AnimatedPressable
      disabled={disabled}
      hitSlop={hitSlop}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      onPressIn={() => {
        down.value = withTiming(1, { duration: durations.pressIn });
      }}
      onPressOut={() => {
        down.value = withSpring(0, springs.press);
      }}
      onPress={onPress}
      onLongPress={
        onLongPress &&
        (() => {
          haptic.medium();
          onLongPress();
        })
      }
      delayLongPress={LONG_PRESS_MS}
      style={[style, animated]}
    >
      {children}
    </AnimatedPressable>
  );
}
