import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { useEffect } from 'react';
import { ActivityIndicator, StyleSheet, Text, View, useWindowDimensions } from 'react-native';
import { Directions, Gesture, GestureDetector } from 'react-native-gesture-handler';
import Animated, {
  interpolate,
  useAnimatedStyle,
  useSharedValue,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

import { bridge } from '../bridgeImpl';
import { t } from '../i18n';
import { haptic, pressScale, springs } from '../motion';
import { useReduceMotion } from '../settings';
import { attempt, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { OutputButton } from './Output';
import { Glass } from './Page';
import { Press } from './Press';

/** How far a finger must move before the strip takes the drag as a swipe, not a tap. */
const SWIPE_SLOP = 14;
/** How much of the strip's width a swipe must cover, or how fast it must fly, to skip. */
const SKIP_SHARE = 0.28;
const SKIP_VELOCITY = 650;
/** How long the old track slides out after a skip, and how far the next one slides in from. */
const OUT_MS = 140;
const IN_SHARE = 0.35;
/** How much the play glyph shrinks before it springs back when it changes. */
const GLYPH_DIP = 0.7;

/**
 * The strip floating over the bottom of the screen while something plays: artwork, title, a
 * progress line, play / pause and next. A tap or an upward flick opens the player, and a swipe
 * sideways skips to the next or the previous track, the strip's content sliding with the
 * finger. It sits on frosted glass, like the tab bar under it.
 */
export function MiniPlayer() {
  const router = useRouter();
  const player = useStore((s) => s.player);
  const positionMs = useStore((s) => s.positionMs);
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const { width } = useWindowDimensions();
  const shift = useSharedValue(0);
  const glyph = useSharedValue(1);
  const playing = player?.status === 'playing';

  // The play glyph dips and springs back whenever it turns into the other one.
  useEffect(() => {
    if (still) return;
    glyph.value = withSequence(
      withTiming(GLYPH_DIP, { duration: 90 }),
      withSpring(1, springs.bouncy),
    );
  }, [playing, still, glyph]);

  const open = () => router.push('/player');
  const skip = (direction: number) => {
    haptic.light();
    attempt(direction < 0 ? bridge.next() : bridge.previous());
  };

  const swipe = Gesture.Pan()
    .activeOffsetX([-SWIPE_SLOP, SWIPE_SLOP])
    .failOffsetY([-SWIPE_SLOP, SWIPE_SLOP])
    .onUpdate((event) => {
      shift.value = event.translationX;
    })
    .onEnd((event) => {
      const far = Math.abs(event.translationX) > width * SKIP_SHARE;
      const fast = Math.abs(event.velocityX) > SKIP_VELOCITY;
      if (!far && !fast) {
        shift.value = withSpring(0, springs.snappy);
        return;
      }
      const direction = Math.sign(event.translationX || event.velocityX);
      shift.value = withTiming(direction * width, { duration: OUT_MS }, (finished) => {
        if (!finished) return;
        shift.value = -direction * width * IN_SHARE;
        shift.value = withSpring(0, springs.snappy);
      });
      scheduleOnRN(skip, direction);
    });
  const flick = Gesture.Fling()
    .direction(Directions.UP)
    .onEnd(() => {
      scheduleOnRN(open);
    });
  const gestures = Gesture.Race(swipe, flick);

  const content = useAnimatedStyle(() => ({
    transform: [{ translateX: shift.value }],
    opacity: interpolate(Math.abs(shift.value), [0, width * 0.5], [1, 0.2]),
  }));
  const glyphStyle = useAnimatedStyle(() => ({ transform: [{ scale: glyph.value }] }));

  if (!player?.track) return null;
  const track = player.track;
  const fraction = player.durationMs > 0 ? Math.min(1, positionMs / player.durationMs) : 0;
  return (
    <GestureDetector gesture={gestures}>
      <View style={styles.card}>
        <Glass edge={false} />
        <Press
          style={styles.body}
          scale={pressScale.row}
          onPress={open}
          accessibilityLabel={t('mobile-open-player')}
        >
          <Animated.View style={[styles.now, content]}>
            <View style={styles.art}>
              <Artwork uri={track.artwork} size={sizes.miniArtwork} seed={track.title} />
            </View>
            <View style={styles.meta}>
              <Text numberOfLines={1} style={styles.title}>
                {track.title}
              </Text>
              <Text numberOfLines={1} style={styles.subtitle}>
                {track.subtitle}
              </Text>
            </View>
          </Animated.View>
          {player.status === 'loading' ? (
            <View style={styles.button}>
              <ActivityIndicator color={colors.text} />
            </View>
          ) : (
            <Press
              hitSlop={spacing.sm}
              scale={pressScale.button}
              style={styles.button}
              onPress={() => {
                haptic.light();
                attempt(bridge.toggle());
              }}
              accessibilityLabel={t(playing ? 'mobile-pause' : 'mobile-play')}
            >
              <Animated.View style={glyphStyle}>
                <Ionicons
                  name={playing ? 'pause' : 'play'}
                  size={sizes.icon + 2}
                  color={colors.text}
                />
              </Animated.View>
            </Press>
          )}
          <OutputButton />
          <Press
            hitSlop={spacing.sm}
            scale={pressScale.button}
            style={styles.button}
            onPress={() => skip(-1)}
            accessibilityLabel={t('mobile-next')}
          >
            <Ionicons name="play-forward" size={sizes.icon} color={colors.text} />
          </Press>
        </Press>
        <View style={styles.track}>
          <View style={[styles.progress, { width: `${fraction * 100}%` }]} />
        </View>
      </View>
    </GestureDetector>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  card: {
    height: sizes.miniPlayer,
    marginHorizontal: spacing.sm,
    marginBottom: spacing.sm,
    borderRadius: radii.large,
    overflow: 'hidden',
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: colors.border,
  },
  body: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
    paddingLeft: spacing.sm,
    paddingRight: spacing.md,
  },
  now: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: spacing.md },
  art: {
    shadowColor: colors.overlay,
    shadowOpacity: 1,
    shadowRadius: spacing.sm,
    shadowOffset: { width: 0, height: spacing.xs / 2 },
  },
  meta: { flex: 1 },
  title: { color: colors.text, fontSize: typography.body, fontWeight: '600' },
  subtitle: { color: colors.textDim, fontSize: typography.label },
  button: {
    width: sizes.icon + spacing.md,
    height: sizes.icon + spacing.md,
    alignItems: 'center',
    justifyContent: 'center',
  },
  track: {
    position: 'absolute',
    left: spacing.md,
    right: spacing.md,
    bottom: 0,
    height: sizes.hairline * 2,
    borderRadius: radii.pill,
    backgroundColor: colors.border,
    overflow: 'hidden',
  },
  progress: { height: '100%', backgroundColor: colors.accent },
}));
