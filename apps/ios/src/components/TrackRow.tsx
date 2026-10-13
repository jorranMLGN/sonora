import { Ionicons } from '@expo/vector-icons';
import { Alert, Pressable, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { bridge } from '../bridgeImpl';
import { clock } from '../format';
import { t } from '../i18n';
import { attempt, savedKey, toggleSaved, useSaved, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { ProviderBadge } from './ProviderBadge';

/** Opens the action sheet for a track: play next, add to the queue, save or unsave. */
export function trackActions(track: Track) {
  const saved = useStore.getState().saved[savedKey('tracks', track.id)] === true;
  Alert.alert(track.title, track.subtitle, [
    { text: t('mobile-play-next'), onPress: () => attempt(bridge.enqueueNext([track.id])) },
    { text: t('mobile-add-to-queue'), onPress: () => attempt(bridge.enqueueLast([track.id])) },
    {
      text: t(saved ? 'mobile-unsave' : 'mobile-save'),
      onPress: () => toggleSaved('tracks', track.id),
    },
    { text: t('mobile-cancel'), style: 'cancel' },
  ]);
}

/** The heart beside a track, filled while the track is in its provider's library. */
function Heart({ id }: { id: string }) {
  const saved = useSaved('tracks', id);
  const { colors } = useTheme();
  return (
    <Pressable
      hitSlop={spacing.sm}
      onPress={() => toggleSaved('tracks', id)}
      accessibilityLabel={t(saved ? 'mobile-unsave' : 'mobile-save')}
    >
      <Ionicons
        name={saved ? 'heart' : 'heart-outline'}
        size={sizes.icon}
        color={saved ? colors.accent : colors.textDim}
      />
    </Pressable>
  );
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
        <ProviderBadge slug={track.provider} />
      </View>
      {track.durationMs > 0 && <Text style={styles.subtitle}>{clock(track.durationMs)}</Text>}
      {heart && <Heart id={track.id} />}
    </Pressable>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
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
}));
