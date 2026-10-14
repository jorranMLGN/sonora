import { useEffect } from 'react';
import { View } from 'react-native';
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from 'react-native-reanimated';

import { useReduceMotion } from '../settings';
import { makeStyles } from '../styles';
import { sizes, spacing } from '../theme';

/** How many stand-in rows fill a list while its first page loads. */
const ROWS = 9;
/** How faint and how solid the stand-ins breathe between, and how long a breath takes. */
const DIM = 0.45;
const BRIGHT = 1;
const BREATH_MS = 850;
/** The widths of a stand-in's two lines, varied per row so the list does not look stamped. */
const TITLE_WIDTHS = ['62%', '48%', '70%', '55%'] as const;
const SUBTITLE_WIDTHS = ['36%', '28%', '42%', '31%'] as const;

/** The opacity every stand-in on screen breathes with together. */
function useBreath() {
  const still = useReduceMotion();
  const level = useSharedValue(BRIGHT);
  useEffect(() => {
    if (still) return;
    level.value = withRepeat(
      withTiming(DIM, { duration: BREATH_MS, easing: Easing.inOut(Easing.quad) }),
      -1,
      true,
    );
    return () => cancelAnimation(level);
  }, [still, level]);
  return useAnimatedStyle(() => ({ opacity: level.value }));
}

/**
 * Stand-in rows shaped like the rows that are coming, breathing softly, so a list that is still
 * loading keeps its shape instead of flashing a spinner. `round` draws artist rows.
 */
export function SkeletonRows({ round = false }: { round?: boolean }) {
  const styles = useStyles();
  const breath = useBreath();
  return (
    <Animated.View style={breath}>
      {Array.from({ length: ROWS }, (_, index) => (
        <View key={index} style={styles.row}>
          <View style={[styles.art, round && styles.round]} />
          <View style={styles.lines}>
            <View style={[styles.title, { width: TITLE_WIDTHS[index % TITLE_WIDTHS.length] }]} />
            <View
              style={[styles.subtitle, { width: SUBTITLE_WIDTHS[index % SUBTITLE_WIDTHS.length] }]}
            />
          </View>
        </View>
      ))}
    </Animated.View>
  );
}

/** Stand-in shelves for Home: a heading and a row of tiles each. */
export function SkeletonShelves() {
  const styles = useStyles();
  const breath = useBreath();
  return (
    <Animated.View style={breath}>
      {[0, 1, 2].map((shelf) => (
        <View key={shelf} style={styles.shelf}>
          <View style={styles.heading} />
          <View style={styles.tiles}>
            {[0, 1, 2].map((tile) => (
              <View key={tile} style={styles.tile} />
            ))}
          </View>
        </View>
      ))}
    </Animated.View>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
  },
  art: {
    width: sizes.rowArtwork,
    height: sizes.rowArtwork,
    borderRadius: radii.small,
    backgroundColor: colors.surfaceRaised,
  },
  round: { borderRadius: sizes.rowArtwork / 2 },
  lines: { flex: 1, gap: spacing.sm },
  title: {
    height: typography.row * 0.75,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
  },
  subtitle: {
    height: typography.label * 0.75,
    borderRadius: radii.pill,
    backgroundColor: colors.surface,
  },
  shelf: { marginBottom: spacing.xl },
  heading: {
    width: '45%',
    height: typography.heading * 0.8,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.md,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
  },
  tiles: { flexDirection: 'row', gap: spacing.md, paddingHorizontal: spacing.lg },
  tile: {
    width: sizes.tileArtwork,
    height: sizes.tileArtwork,
    borderRadius: radii.medium,
    backgroundColor: colors.surfaceRaised,
  },
}));
