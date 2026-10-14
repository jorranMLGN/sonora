import { Ionicons } from '@expo/vector-icons';
import { Image } from 'expo-image';
import { LinearGradient } from 'expo-linear-gradient';
import { useNavigation, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { SafeAreaView } from 'react-native-safe-area-context';

import { largestArtwork } from '../src/artwork';
import { bridge } from '../src/bridgeImpl';
import { CanvasArt } from '../src/components/CanvasArt';
import { OutputRow } from '../src/components/Output';
import { Press } from '../src/components/Press';
import { Toast } from '../src/components/Screen';
import { Scrubber } from '../src/components/Scrubber';
import { Heart, TrackRow } from '../src/components/TrackRow';
import { clock } from '../src/format';
import { t } from '../src/i18n';
import { durations, haptic, pressScale, springs } from '../src/motion';
import { useReduceMotion, useSettings } from '../src/settings';
import { attempt, useStore } from '../src/store';
import { makeStyles, useTheme } from '../src/styles';
import { sizes, spacing } from '../src/theme';

/** How far the artwork sinks back while playback is paused, as Apple Music's does. */
const PAUSED_SCALE = 0.86;
/** How soft the artwork behind the player is, and how far it overfills the sheet. */
const BACKDROP_BLUR = 80;
const BACKDROP_SCALE = 1.3;
/** How much the play glyph shrinks before it springs back when it changes. */
const GLYPH_DIP = 0.75;

/**
 * The full player sheet: the track's artwork blurred behind everything, the artwork itself,
 * which sinks back a little while paused, the title with its heart, the scrubber, transport and
 * the queue below. Every control answers the finger with a spring and a light tap.
 */
export default function Player() {
  const router = useRouter();
  const player = useStore((s) => s.player);
  const positionMs = useStore((s) => s.positionMs);
  const queue = useStore((s) => s.queue);
  const playing = player?.status === 'playing';
  const { colors } = useTheme();
  const styles = useStyles();
  const artwork = useSettings((s) => s.artwork);
  const still = useReduceMotion();
  // Where the finger holds the scrubber, so the time under it follows the drag.
  const [scrub, setScrub] = useState<number | null>(null);
  const scrubbing = scrub !== null;
  const navigation = useNavigation();
  // While the scrubber is held, the sheet must not take the drag as a swipe down to close.
  useEffect(() => {
    navigation.setOptions({ gestureEnabled: !scrubbing });
  }, [navigation, scrubbing]);

  // The artwork sits full size while playing and sinks back a little while paused.
  const depth = useSharedValue(playing || still ? 1 : PAUSED_SCALE);
  const glyph = useSharedValue(1);
  useEffect(() => {
    const target = playing || still ? 1 : PAUSED_SCALE;
    depth.value = still ? target : withSpring(target, springs.smooth);
    if (!still) {
      glyph.value = withSequence(
        withTiming(GLYPH_DIP, { duration: 90 }),
        withSpring(1, springs.bouncy),
      );
    }
  }, [playing, still, depth, glyph]);
  const artStyle = useAnimatedStyle(() => ({ transform: [{ scale: depth.value }] }));
  const glyphStyle = useAnimatedStyle(() => ({ transform: [{ scale: glyph.value }] }));
  const backdrop = largestArtwork(player?.track?.artwork ?? null);

  const header = (
    <View style={styles.header}>
      <Animated.View style={[styles.art, artStyle]}>
        <CanvasArt
          track={player?.track ?? null}
          size={sizes.playerArtwork}
          playing={playing}
          shape={artwork}
        />
      </Animated.View>
      <View style={styles.titleRow}>
        <View style={styles.titles}>
          <Text numberOfLines={1} style={styles.title}>
            {player?.track?.title ?? t('mobile-nothing-playing')}
          </Text>
          <Text numberOfLines={1} style={styles.artist}>
            {player?.track?.subtitle ?? ''}
          </Text>
        </View>
        {player?.track && <Heart id={player.track.id} size={sizes.iconLarge - spacing.xs} />}
      </View>
      <View style={styles.scrubber}>
        <Scrubber
          value={positionMs}
          max={player?.durationMs ?? 0}
          playing={playing}
          onSeek={(ms) => attempt(bridge.seek(ms))}
          onScrub={setScrub}
        />
        <View style={styles.times}>
          <Text style={styles.time}>{clock(scrub ?? positionMs)}</Text>
          <Text style={styles.time}>{clock(player?.durationMs ?? 0)}</Text>
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
          name="play-back"
          size={sizes.iconLarge}
          label={t('mobile-previous')}
          onPress={() => attempt(bridge.previous())}
        />
        {player?.status === 'loading' ? (
          <View style={styles.toggle}>
            <ActivityIndicator color={colors.onAccent} />
          </View>
        ) : (
          <Press
            scale={pressScale.button}
            style={styles.toggle}
            onPress={() => {
              haptic.light();
              attempt(bridge.toggle());
            }}
            accessibilityLabel={t(playing ? 'mobile-pause' : 'mobile-play')}
          >
            <Animated.View style={glyphStyle}>
              <Ionicons
                name={playing ? 'pause' : 'play'}
                size={sizes.iconLarge}
                color={colors.onAccent}
              />
            </Animated.View>
          </Press>
        )}
        <IconButton
          name="play-forward"
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
      <OutputRow />
      <Text style={styles.queueHeading}>{t('mobile-queue')}</Text>
    </View>
  );

  return (
    <SafeAreaView style={styles.screen} edges={['bottom']}>
      <View style={StyleSheet.absoluteFill} pointerEvents="none">
        {backdrop !== null && (
          <Image
            source={{ uri: backdrop }}
            style={styles.backdrop}
            contentFit="cover"
            blurRadius={BACKDROP_BLUR}
            transition={still ? 0 : durations.backdrop}
          />
        )}
        <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }]} />
        <LinearGradient
          colors={[colors.transparent, colors.background]}
          locations={[0.2, 0.75]}
          style={StyleSheet.absoluteFill}
        />
      </View>
      <Pressable
        style={styles.close}
        onPress={() => router.back()}
        hitSlop={spacing.md}
        accessibilityLabel={t('mobile-close')}
      >
        <View style={styles.grabber} />
      </Pressable>
      <FlatList
        data={queue?.items ?? []}
        scrollEnabled={!scrubbing}
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
                  onLongPress={() => !current && attempt(bridge.remove(index))}
                  heart={false}
                />
              </View>
              {!current && (
                <Press
                  scale={pressScale.button}
                  hitSlop={spacing.sm}
                  style={styles.remove}
                  onPress={() => attempt(bridge.remove(index))}
                  accessibilityLabel={t('mobile-remove')}
                >
                  <Ionicons name="close" size={sizes.icon} color={colors.textDim} />
                </Press>
              )}
            </View>
          );
        }}
      />
      <Toast quiet />
    </SafeAreaView>
  );
}

/** A transport button that springs under the finger and taps lightly. */
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
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <Press
      scale={pressScale.button}
      hitSlop={spacing.sm}
      onPress={() => {
        haptic.light();
        onPress();
      }}
      accessibilityLabel={label}
    >
      <Ionicons name={name} size={size} color={on ? colors.accent : colors.text} />
      {badge && <Text style={styles.badge}>{badge}</Text>}
    </Press>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  screen: { flex: 1, backgroundColor: colors.background },
  backdrop: { ...StyleSheet.absoluteFill, transform: [{ scale: BACKDROP_SCALE }] },
  close: { alignSelf: 'center', paddingVertical: spacing.md, paddingHorizontal: spacing.xl },
  grabber: {
    width: sizes.grabberWidth,
    height: sizes.grabberHeight,
    borderRadius: radii.pill,
    backgroundColor: colors.textDim,
  },
  header: { alignItems: 'center', gap: spacing.lg, paddingHorizontal: spacing.xl },
  art: {
    marginVertical: spacing.md,
    shadowColor: colors.overlay,
    shadowOpacity: 1,
    shadowRadius: spacing.xl,
    shadowOffset: { width: 0, height: spacing.lg },
  },
  titleRow: { alignSelf: 'stretch', flexDirection: 'row', alignItems: 'center', gap: spacing.md },
  titles: { flex: 1 },
  title: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '800',
    letterSpacing: -0.4,
  },
  artist: { color: colors.textDim, fontSize: typography.title, marginTop: spacing.xs / 2 },
  scrubber: { alignSelf: 'stretch' },
  times: { flexDirection: 'row', justifyContent: 'space-between' },
  time: { color: colors.textDim, fontSize: typography.caption, fontVariant: ['tabular-nums'] },
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
}));
