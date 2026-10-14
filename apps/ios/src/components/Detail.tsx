import { Ionicons } from '@expo/vector-icons';
import { Image } from 'expo-image';
import { LinearGradient } from 'expo-linear-gradient';
import { useEffect, useRef } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import Animated, {
  Extrapolation,
  type SharedValue,
  interpolate,
  useAnimatedStyle,
  useSharedValue,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';

import { largestArtwork } from '../artwork';
import type { LibraryPart, Track } from '../bridge';
import { bridge } from '../bridgeImpl';
import { t } from '../i18n';
import { durations, haptic, pressScale, springs } from '../motion';
import { useReduceMotion } from '../settings';
import { attempt, toggleSaved, useSaved, useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { brand, sizes, spacing } from '../theme';
import { Artwork, type ArtworkKind } from './Artwork';
import { Press } from './Press';

/** How far a detail page scrolls before its bar frosts and shows the title. */
export const DETAIL_REVEAL = sizes.headerArtwork + spacing.xxl * 2;
/** How far above the page the blurred backdrop reaches, so a pull down never shows its edge. */
const BACKDROP_OVERHANG = 400;
/** How soft the backdrop is. */
const BACKDROP_BLUR = 60;
/** How small and faint the cover gets as it scrolls away. */
const COVER_SHRINK = 0.82;
const COVER_FADE = 0.2;
/** How big a save toggle swells when it fills. */
const SAVE_SWELL = 1.3;

/**
 * The save toggle in a detail page's bar, filled while the item is in its library. Filling it
 * swells it with a spring and taps a success haptic.
 */
export function SaveButton({ id, kind }: { id: string; kind: LibraryPart }) {
  const saved = useSaved(kind, id);
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const scale = useSharedValue(1);
  const was = useRef(saved);
  useEffect(() => {
    if (saved && !was.current && !still) {
      scale.value = withSequence(
        withTiming(SAVE_SWELL, { duration: 120 }),
        withSpring(1, springs.bouncy),
      );
    }
    was.current = saved;
  }, [saved, still, scale]);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));
  return (
    <Press
      scale={pressScale.button}
      hitSlop={spacing.sm}
      style={styles.round}
      onPress={() => {
        if (!saved) haptic.success();
        toggleSaved(kind, id);
      }}
      accessibilityLabel={t(saved ? 'mobile-unsave' : 'mobile-save')}
    >
      <Animated.View style={animated}>
        <Ionicons
          name={saved ? 'heart' : 'heart-outline'}
          size={sizes.icon}
          color={saved ? colors.accent : colors.text}
        />
      </Animated.View>
    </Press>
  );
}

/**
 * The head of an album, playlist or artist page: the item's own artwork blurred behind
 * everything, its cover, title, a line of what it holds and where it is from, and Play and
 * Shuffle. The cover shrinks and fades as it scrolls up under the bar. `top` is the page's
 * header height, which the backdrop reaches up behind.
 */
export function DetailHeader({
  item,
  kind,
  tracks,
  origin,
  scrollY,
  top,
  count,
}: {
  item: Track;
  kind: ArtworkKind;
  tracks: Track[];
  origin: string;
  scrollY: SharedValue<number>;
  top: number;
  /** How many tracks the item holds, when that is worth saying. */
  count?: number;
}) {
  const theme = useTheme();
  const styles = useStyles();
  const provider = useStore((s) => s.providers.find((p) => p.slug === item.provider)?.name ?? '');
  const round = kind === 'artist';
  const play = (shuffle: boolean) => {
    if (tracks.length === 0) return;
    haptic.light();
    attempt(
      bridge.setShuffle(shuffle).then(() =>
        bridge.play(
          tracks.map((track) => track.id),
          shuffle ? Math.floor(Math.random() * tracks.length) : 0,
          origin,
        ),
      ),
    );
  };
  const cover = useAnimatedStyle(() => ({
    opacity: interpolate(scrollY.value, [0, DETAIL_REVEAL], [1, COVER_FADE], Extrapolation.CLAMP),
    transform: [
      {
        scale: interpolate(
          scrollY.value,
          [0, DETAIL_REVEAL],
          [1, COVER_SHRINK],
          Extrapolation.CLAMP,
        ),
      },
      {
        translateY: interpolate(
          scrollY.value,
          [0, DETAIL_REVEAL],
          [0, DETAIL_REVEAL * 0.35],
          Extrapolation.CLAMP,
        ),
      },
    ],
  }));
  const backdrop = largestArtwork(item.artwork);
  const meta = [
    count !== undefined && count > 0 ? t('mobile-track-count', { count }) : '',
    provider,
  ]
    .filter((part) => part !== '')
    .join(' · ');
  return (
    <View style={styles.header}>
      <View pointerEvents="none" style={[styles.backdrop, { top: -(top + BACKDROP_OVERHANG) }]}>
        {backdrop !== null && (
          <Image
            source={{ uri: backdrop }}
            style={StyleSheet.absoluteFill}
            contentFit="cover"
            blurRadius={BACKDROP_BLUR}
            transition={durations.backdrop}
          />
        )}
        <View style={[StyleSheet.absoluteFill, { backgroundColor: theme.colors.scrim }]} />
        <LinearGradient
          colors={[theme.colors.transparent, theme.colors.background]}
          locations={[0.45, 1]}
          style={StyleSheet.absoluteFill}
        />
      </View>
      <Animated.View style={[styles.cover, round && styles.coverRound, cover]}>
        <Artwork
          uri={item.artwork}
          size={sizes.headerArtwork}
          round={round}
          kind={kind}
          seed={item.title}
        />
      </Animated.View>
      <Text style={styles.title} numberOfLines={2}>
        {item.title}
      </Text>
      {item.subtitle !== '' && (
        <Text style={styles.subtitle} numberOfLines={1}>
          {item.subtitle}
        </Text>
      )}
      {meta !== '' && (
        <View style={styles.metaRow}>
          {provider !== '' && (
            <View style={[styles.dot, { backgroundColor: brand(theme, item.provider) }]} />
          )}
          <Text style={styles.meta}>{meta}</Text>
        </View>
      )}
      <View style={styles.actions}>
        <Press
          scale={pressScale.tile}
          style={[styles.action, styles.primary]}
          onPress={() => play(false)}
          disabled={tracks.length === 0}
        >
          <Ionicons name="play" size={sizes.icon - spacing.xs} color={theme.colors.onAccent} />
          <Text style={styles.primaryText}>{t('mobile-play')}</Text>
        </Press>
        <Press
          scale={pressScale.tile}
          style={[styles.action, styles.secondary]}
          onPress={() => play(true)}
          disabled={tracks.length === 0}
        >
          <Ionicons name="shuffle" size={sizes.icon - spacing.xs} color={theme.colors.accent} />
          <Text style={styles.secondaryText}>{t('mobile-shuffle')}</Text>
        </Press>
      </View>
    </View>
  );
}

/** Spinner shown while a detail page loads, under the page's bar. */
export function Loading({ top = 0 }: { top?: number }) {
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <ActivityIndicator
      color={colors.textDim}
      style={[styles.loading, { marginTop: top + spacing.xxl }]}
    />
  );
}

/** Whether a track is the one playing, for highlighting rows. */
export function useCurrentId(): string | null {
  return useStore((s) => s.player?.track?.id ?? null);
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  header: { alignItems: 'center', paddingHorizontal: spacing.lg, paddingBottom: spacing.lg },
  backdrop: { position: 'absolute', left: 0, right: 0, bottom: 0, overflow: 'hidden' },
  cover: {
    marginTop: spacing.sm,
    borderRadius: radii.small,
    shadowColor: colors.overlay,
    shadowOpacity: 1,
    shadowRadius: spacing.xl,
    shadowOffset: { width: 0, height: spacing.md },
  },
  coverRound: { borderRadius: sizes.headerArtwork / 2 },
  title: {
    color: colors.text,
    fontSize: typography.heading + 2,
    fontWeight: '800',
    letterSpacing: -0.5,
    textAlign: 'center',
    marginTop: spacing.xl,
  },
  subtitle: {
    color: colors.accent,
    fontSize: typography.title,
    fontWeight: '600',
    textAlign: 'center',
    marginTop: spacing.xs,
  },
  metaRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs + spacing.xs / 2,
    marginTop: spacing.xs,
  },
  dot: {
    width: sizes.dot - spacing.xs / 2,
    height: sizes.dot - spacing.xs / 2,
    borderRadius: radii.pill,
  },
  meta: { color: colors.textDim, fontSize: typography.label },
  actions: { alignSelf: 'stretch', flexDirection: 'row', gap: spacing.md, marginTop: spacing.lg },
  action: {
    flex: 1,
    height: sizes.button - spacing.xs,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: spacing.sm,
    borderRadius: radii.medium,
  },
  primary: { backgroundColor: colors.accent },
  secondary: { backgroundColor: colors.surfaceRaised },
  primaryText: { color: colors.onAccent, fontWeight: '700', fontSize: typography.body },
  secondaryText: { color: colors.accent, fontWeight: '700', fontSize: typography.body },
  round: {
    width: sizes.topBar - spacing.sm,
    height: sizes.topBar - spacing.sm,
    borderRadius: radii.pill,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.scrim,
  },
  loading: { alignSelf: 'center' },
}));
