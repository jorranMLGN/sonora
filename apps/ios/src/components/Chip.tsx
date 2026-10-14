import { type ReactNode, useEffect } from 'react';
import { ScrollView, View } from 'react-native';
import Animated, {
  interpolateColor,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';

import { durations, haptic, pressScale } from '../motion';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Press } from './Press';

/**
 * A filter chip. Choosing it washes its fill over to the accent and its label to the text on
 * the accent, and ticks the Taptic Engine. `dot` puts a small coloured mark before the label,
 * such as a service's own colour.
 */
export function Chip({
  label,
  on,
  onPress,
  dot,
}: {
  label: string;
  on: boolean;
  onPress: () => void;
  dot?: string;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  const chosen = useSharedValue(on ? 1 : 0);
  useEffect(() => {
    chosen.value = withTiming(on ? 1 : 0, { duration: durations.fade });
  }, [on, chosen]);
  const off = colors.surfaceRaised;
  const accent = colors.accent;
  const text = colors.text;
  const onAccent = colors.onAccent;
  const fill = useAnimatedStyle(() => ({
    backgroundColor: interpolateColor(chosen.value, [0, 1], [off, accent]),
  }));
  const ink = useAnimatedStyle(() => ({
    color: interpolateColor(chosen.value, [0, 1], [text, onAccent]),
  }));
  return (
    <Press
      scale={pressScale.chip}
      onPress={() => {
        if (!on) haptic.select();
        onPress();
      }}
      accessibilityLabel={label}
    >
      <Animated.View style={[styles.chip, fill]}>
        {dot && <View style={[styles.dot, { backgroundColor: dot }, on && styles.dotOn]} />}
        <Animated.Text numberOfLines={1} style={[styles.label, on && styles.labelOn, ink]}>
          {label}
        </Animated.Text>
      </Animated.View>
    </Press>
  );
}

/** A row of chips that scrolls sideways when they do not fit, inset like the rows below it. */
export function ChipRow({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      style={styles.row}
      contentContainerStyle={styles.rowContent}
    >
      {children}
    </ScrollView>
  );
}

/** The height of one `ChipRow`, for a page header that has to know it before drawing. */
export const CHIP_ROW_HEIGHT = sizes.chip + spacing.sm;

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  // A horizontal ScrollView may shrink by default, which squeezed the chips and cut their text.
  row: { flexGrow: 0, flexShrink: 0, height: CHIP_ROW_HEIGHT },
  rowContent: { paddingHorizontal: spacing.lg, gap: spacing.sm, alignItems: 'center' },
  chip: {
    height: sizes.chip,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs + spacing.xs / 2,
    paddingHorizontal: spacing.md + spacing.xs,
    borderRadius: radii.pill,
  },
  dot: { width: sizes.dot, height: sizes.dot, borderRadius: radii.pill },
  // On the accent the service's colour can vanish, so the dot takes a ring of the page.
  dotOn: { borderWidth: sizes.hairline, borderColor: colors.onAccent },
  label: { color: colors.text, fontSize: typography.label, fontWeight: '500' },
  labelOn: { fontWeight: '600' },
}));
