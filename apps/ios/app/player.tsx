import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { ActivityIndicator, FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { bridge } from '../src/bridgeImpl';
import { Artwork } from '../src/components/Artwork';
import { ErrorBanner } from '../src/components/Screen';
import { Scrubber } from '../src/components/Scrubber';
import { TrackRow } from '../src/components/TrackRow';
import { clock } from '../src/format';
import { t } from '../src/i18n';
import { attempt, useStore } from '../src/store';
import { colors, radii, sizes, spacing, typography } from '../src/theme';

/** The full player sheet: artwork, scrubber, transport and the queue below. */
export default function Player() {
  const router = useRouter();
  const player = useStore((s) => s.player);
  const positionMs = useStore((s) => s.positionMs);
  const queue = useStore((s) => s.queue);
  const playing = player?.status === 'playing';

  const header = (
    <View style={styles.header}>
      <Artwork uri={player?.track?.artwork ?? null} size={sizes.playerArtwork} />
      <View style={styles.titles}>
        <Text numberOfLines={1} style={styles.title}>
          {player?.track?.title ?? t('mobile-nothing-playing')}
        </Text>
        <Text numberOfLines={1} style={styles.subtitle}>
          {player?.track?.subtitle ?? ''}
        </Text>
      </View>
      <View style={styles.scrubber}>
        <Scrubber
          value={positionMs}
          max={player?.durationMs ?? 0}
          onSeek={(ms) => attempt(bridge.seek(ms))}
        />
        <View style={styles.times}>
          <Text style={styles.subtitle}>{clock(positionMs)}</Text>
          <Text style={styles.subtitle}>{clock(player?.durationMs ?? 0)}</Text>
        </View>
      </View>
      <View style={styles.controls}>
        <IconButton
          name="shuffle"
          on={player?.shuffle ?? false}
          label={t('mobile-shuffle')}
          onPress={() => attempt(bridge.setShuffle(!(player?.shuffle ?? false)))}
        />
        <IconButton
          name="play-skip-back"
          size={sizes.iconLarge}
          label={t('mobile-previous')}
          onPress={() => attempt(bridge.previous())}
        />
        {player?.status === 'loading' ? (
          <View style={styles.toggle}>
            <ActivityIndicator color={colors.onAccent} />
          </View>
        ) : (
          <Pressable
            style={styles.toggle}
            onPress={() => attempt(bridge.toggle())}
            accessibilityLabel={t(playing ? 'mobile-pause' : 'mobile-play')}
          >
            <Ionicons
              name={playing ? 'pause' : 'play'}
              size={sizes.iconLarge}
              color={colors.onAccent}
            />
          </Pressable>
        )}
        <IconButton
          name="play-skip-forward"
          size={sizes.iconLarge}
          label={t('mobile-next')}
          onPress={() => attempt(bridge.next())}
        />
        <IconButton
          name="repeat"
          on={(player?.repeat ?? 'off') !== 'off'}
          badge={player?.repeat === 'one' ? '1' : undefined}
          label={t('mobile-repeat')}
          onPress={() => attempt(bridge.cycleRepeat())}
        />
      </View>
      <Text style={styles.queueHeading}>{t('mobile-queue')}</Text>
    </View>
  );

  return (
    <SafeAreaView style={styles.screen} edges={['bottom']}>
      <Pressable
        style={styles.close}
        onPress={() => router.back()}
        hitSlop={spacing.md}
        accessibilityLabel={t('mobile-close')}
      >
        <Ionicons name="chevron-down" size={sizes.iconLarge} color={colors.text} />
      </Pressable>
      <ErrorBanner />
      <FlatList
        data={queue?.items ?? []}
        keyExtractor={(item, index) => `${index}:${item.id}`}
        ListHeaderComponent={header}
        ListEmptyComponent={<Text style={styles.empty}>{t('mobile-queue-empty')}</Text>}
        renderItem={({ item, index }) => {
          const current = queue?.current === index;
          return (
            <View style={styles.queueRow}>
              <View style={styles.queueTrack}>
                <TrackRow
                  track={item}
                  current={current}
                  onPress={() =>
                    attempt(
                      bridge.play(
                        (queue?.items ?? []).map((track) => track.id),
                        index,
                        queue?.origin ?? null,
                      ),
                    )
                  }
                  onLongPress={() => {}}
                />
              </View>
              {!current && (
                <Pressable
                  hitSlop={spacing.sm}
                  style={styles.remove}
                  onPress={() => attempt(bridge.remove(index))}
                  accessibilityLabel={t('mobile-remove')}
                >
                  <Ionicons name="close" size={sizes.icon} color={colors.textDim} />
                </Pressable>
              )}
            </View>
          );
        }}
      />
    </SafeAreaView>
  );
}

function IconButton({
  name,
  on = false,
  size = sizes.icon,
  badge,
  label,
  onPress,
}: {
  name: keyof typeof Ionicons.glyphMap;
  on?: boolean;
  size?: number;
  badge?: string;
  label: string;
  onPress: () => void;
}) {
  return (
    <Pressable hitSlop={spacing.sm} onPress={onPress} accessibilityLabel={label}>
      <Ionicons name={name} size={size} color={on ? colors.accent : colors.text} />
      {badge && <Text style={styles.badge}>{badge}</Text>}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  close: { alignSelf: 'center', padding: spacing.sm },
  header: { alignItems: 'center', gap: spacing.lg, paddingHorizontal: spacing.lg },
  titles: { alignSelf: 'stretch' },
  title: { color: colors.text, fontSize: typography.heading, fontWeight: '700' },
  subtitle: { color: colors.textDim, fontSize: typography.caption },
  scrubber: { alignSelf: 'stretch' },
  times: { flexDirection: 'row', justifyContent: 'space-between' },
  controls: {
    alignSelf: 'stretch',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  toggle: {
    width: sizes.toggle,
    height: sizes.toggle,
    borderRadius: radii.pill,
    backgroundColor: colors.accent,
    alignItems: 'center',
    justifyContent: 'center',
  },
  badge: {
    position: 'absolute',
    right: -spacing.xs,
    top: -spacing.xs,
    color: colors.accent,
    fontSize: typography.caption,
    fontWeight: '700',
  },
  queueHeading: {
    alignSelf: 'flex-start',
    color: colors.text,
    fontSize: typography.title,
    fontWeight: '700',
    marginTop: spacing.md,
  },
  queueRow: { flexDirection: 'row', alignItems: 'center' },
  queueTrack: { flex: 1 },
  remove: { paddingRight: spacing.lg },
  empty: { color: colors.textDim, textAlign: 'center', marginTop: spacing.lg },
});
