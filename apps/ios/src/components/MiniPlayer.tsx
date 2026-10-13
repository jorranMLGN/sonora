import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';

import { bridge } from '../bridgeImpl';
import { t } from '../i18n';
import { attempt, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';

/** The strip above the tab bar: current track, a progress hairline, toggle; opens the player. */
export function MiniPlayer() {
  const router = useRouter();
  const player = useStore((s) => s.player);
  const positionMs = useStore((s) => s.positionMs);
  const { colors } = useTheme();
  const styles = useStyles();
  if (!player?.track) return null;
  const fraction = player.durationMs > 0 ? Math.min(1, positionMs / player.durationMs) : 0;
  return (
    <Pressable
      style={styles.bar}
      onPress={() => router.push('/player')}
      accessibilityLabel={t('mobile-open-player')}
    >
      <View style={[styles.progress, { width: `${fraction * 100}%` }]} />
      <Artwork uri={player.track.artwork} size={sizes.miniArtwork} />
      <View style={styles.meta}>
        <Text numberOfLines={1} style={styles.title}>
          {player.track.title}
        </Text>
        <Text numberOfLines={1} style={styles.subtitle}>
          {player.track.subtitle}
        </Text>
      </View>
      {player.status === 'loading' ? (
        <ActivityIndicator color={colors.text} style={styles.button} />
      ) : (
        <Pressable
          hitSlop={spacing.sm}
          style={styles.button}
          onPress={() => attempt(bridge.toggle())}
          accessibilityLabel={t(player.status === 'playing' ? 'mobile-pause' : 'mobile-play')}
        >
          <Ionicons
            name={player.status === 'playing' ? 'pause' : 'play'}
            size={sizes.icon}
            color={colors.text}
          />
        </Pressable>
      )}
      <Pressable
        hitSlop={spacing.sm}
        style={styles.button}
        onPress={() => attempt(bridge.next())}
        accessibilityLabel={t('mobile-next')}
      >
        <Ionicons name="play-skip-forward" size={sizes.icon} color={colors.text} />
      </Pressable>
    </Pressable>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  bar: {
    height: sizes.miniPlayer,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    backgroundColor: colors.surfaceRaised,
  },
  progress: {
    position: 'absolute',
    top: 0,
    left: 0,
    height: sizes.hairline * 2,
    backgroundColor: colors.accent,
  },
  meta: { flex: 1 },
  title: { color: colors.text, fontSize: typography.body, fontWeight: '600' },
  subtitle: { color: colors.textDim, fontSize: typography.caption },
  button: { padding: spacing.xs },
}));
