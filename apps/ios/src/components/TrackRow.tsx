import { Ionicons } from '@expo/vector-icons';
import { memo, useEffect, useRef } from 'react';
import { ActionSheetIOS, StyleSheet, Text, View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';

import type { Track } from '../bridge';
import { bridge } from '../bridgeImpl';
import { clock } from '../format';
import { t } from '../i18n';
import { haptic, pressScale, springs } from '../motion';
import { useReduceMotion } from '../settings';
import { attempt, savedKey, toggleSaved, useSaved, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { Equalizer } from './Equalizer';
import { Press } from './Press';
import { ProviderBadge } from './ProviderBadge';

/** How big a heart swells when it fills, before it settles. */
const HEART_SWELL = 1.35;

/** Opens the system action sheet for a track: play next, add to the queue, save or unsave. */
export function trackActions(track: Track) {
  const saved = useStore.getState().saved[savedKey('tracks', track.id)] === true;
  const actions = [
    { label: t('mobile-play-next'), run: () => attempt(bridge.enqueueNext([track.id])) },
    { label: t('mobile-add-to-queue'), run: () => attempt(bridge.enqueueLast([track.id])) },
    {
      label: t(saved ? 'mobile-unsave' : 'mobile-save'),
      run: () => {
        if (!saved) haptic.success();
        toggleSaved('tracks', track.id);
      },
    },
  ];
  ActionSheetIOS.showActionSheetWithOptions(
    {
      title: track.title,
      message: track.subtitle,
      options: [...actions.map((action) => action.label), t('mobile-cancel')],
      cancelButtonIndex: actions.length,
      destructiveButtonIndex: saved ? actions.length - 1 : undefined,
    },
    (index) => actions[index]?.run(),
  );
}

/**
 * The heart beside a track, filled while the track is in its provider's library. Filling it
 * swells it with a spring and taps a success haptic.
 */
export function Heart({ id, size = sizes.icon }: { id: string; size?: number }) {
  const saved = useSaved('tracks', id);
  const { colors } = useTheme();
  const still = useReduceMotion();
  const scale = useSharedValue(1);
  const was = useRef(saved);
  useEffect(() => {
    // Only a heart that just filled celebrates, not one that loaded filled.
    if (saved && !was.current && !still) {
      scale.value = withSequence(
        withTiming(HEART_SWELL, { duration: 120 }),
        withSpring(1, springs.bouncy),
      );
    }
    was.current = saved;
  }, [saved, still, scale]);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));
  return (
    <Press
      hitSlop={spacing.sm}
      scale={pressScale.button}
      onPress={() => {
        if (!saved) haptic.success();
        toggleSaved('tracks', id);
      }}
      accessibilityLabel={t(saved ? 'mobile-unsave' : 'mobile-save')}
    >
      <Animated.View style={animated}>
        <Ionicons
          name={saved ? 'heart' : 'heart-outline'}
          size={size}
          color={saved ? colors.accent : colors.textDim}
        />
      </Animated.View>
    </Press>
  );
}

/**
 * One track in a list; long press opens its actions unless `onLongPress` overrides them. The
 * current track shows the now-playing mark over its artwork and its title in the accent. An
 * unplayable track is dimmed and ignores a tap, but its long press still works, so it can be
 * queued, saved or removed like any other.
 */
export const TrackRow = memo(function TrackRow({
  track,
  current = false,
  onPress,
  onLongPress,
  heart = true,
}: {
  track: Track;
  current?: boolean;
  onPress: () => void;
  onLongPress?: () => void;
  /** Whether the row shows the save toggle; the queue leaves it out. */
  heart?: boolean;
}) {
  const styles = useStyles();
  return (
    <Press
      highlight
      style={styles.row}
      onPress={track.playable ? onPress : undefined}
      onLongPress={onLongPress ?? (() => trackActions(track))}
    >
      <View style={!track.playable && styles.faded}>
        <Artwork uri={track.artwork} size={sizes.rowArtwork} seed={track.title} />
        {current && (
          <View style={styles.playing}>
            <Equalizer />
          </View>
        )}
      </View>
      <View style={styles.meta}>
        <Text
          numberOfLines={1}
          style={[styles.title, !track.playable && styles.dim, current && styles.accent]}
        >
          {track.title}
        </Text>
        <View style={styles.line}>
          {track.explicit && (
            <View style={styles.explicit} accessibilityLabel={t('mobile-explicit')}>
              <Text style={styles.explicitMark}>{t('mobile-explicit-mark')}</Text>
            </View>
          )}
          <Text numberOfLines={1} style={styles.subtitle}>
            {track.subtitle}
          </Text>
        </View>
        <ProviderBadge slug={track.provider} />
      </View>
      {track.durationMs > 0 && <Text style={styles.duration}>{clock(track.durationMs)}</Text>}
      {heart && <Heart id={track.id} />}
    </Press>
  );
});

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    gap: spacing.md,
  },
  faded: { opacity: 0.4 },
  playing: {
    ...StyleSheet.absoluteFill,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: radii.small,
    backgroundColor: colors.overlay,
  },
  meta: { flex: 1, gap: spacing.xs / 2 },
  title: { color: colors.text, fontSize: typography.row, fontWeight: '500', letterSpacing: -0.2 },
  line: { flexDirection: 'row', alignItems: 'center', gap: spacing.xs + spacing.xs / 2 },
  subtitle: { flexShrink: 1, color: colors.textDim, fontSize: typography.label },
  explicit: {
    paddingHorizontal: spacing.xs,
    borderRadius: radii.small,
    backgroundColor: colors.textDim,
  },
  explicitMark: { color: colors.background, fontSize: typography.tag, fontWeight: '800' },
  duration: {
    color: colors.textDim,
    fontSize: typography.label,
    fontVariant: ['tabular-nums'],
  },
  accent: { color: colors.accent },
  dim: { color: colors.textDim },
}));
