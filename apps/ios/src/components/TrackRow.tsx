import { Alert, Pressable, StyleSheet, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { bridge } from '../bridgeImpl';
import { clock } from '../format';
import { t } from '../i18n';
import { attempt } from '../store';
import { colors, sizes, spacing, typography } from '../theme';
import { Artwork } from './Artwork';

/** Opens the action sheet for a track: play next, add to the queue, save. */
export function trackActions(track: Track) {
  Alert.alert(track.title, track.subtitle, [
    { text: t('mobile-play-next'), onPress: () => attempt(bridge.enqueueNext([track.id])) },
    { text: t('mobile-add-to-queue'), onPress: () => attempt(bridge.enqueueLast([track.id])) },
    { text: t('mobile-save'), onPress: () => attempt(bridge.setSaved(track.id, true)) },
    { text: t('mobile-cancel'), style: 'cancel' },
  ]);
}

/**
 * One track in a list; long press opens its actions unless `onLongPress` overrides them. An
 * unplayable track is dimmed and ignores a tap, but its long press still works, so it can be
 * queued, saved or removed like any other.
 */
export function TrackRow({
  track,
  current = false,
  onPress,
  onLongPress,
}: {
  track: Track;
  current?: boolean;
  onPress: () => void;
  onLongPress?: () => void;
}) {
  return (
    <Pressable
      style={[styles.row, current && styles.current]}
      onPress={track.playable ? onPress : undefined}
      onLongPress={onLongPress ?? (() => trackActions(track))}
    >
      <Artwork uri={track.artwork} size={sizes.rowArtwork} />
      <View style={styles.meta}>
        <Text numberOfLines={1} style={[styles.title, !track.playable && styles.dim, current && styles.accent]}>
          {track.title}
        </Text>
        <Text numberOfLines={1} style={styles.subtitle}>
          {track.explicit ? `${t('mobile-explicit')} · ` : ''}
          {track.subtitle}
        </Text>
      </View>
      {track.durationMs > 0 && <Text style={styles.subtitle}>{clock(track.durationMs)}</Text>}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    gap: spacing.md,
  },
  current: { backgroundColor: colors.surface },
  meta: { flex: 1 },
  title: { color: colors.text, fontSize: typography.body, fontWeight: '500' },
  subtitle: { color: colors.textDim, fontSize: typography.caption },
  accent: { color: colors.accent },
  dim: { color: colors.textDim },
});
