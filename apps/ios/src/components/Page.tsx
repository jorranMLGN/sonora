import { Ionicons } from '@expo/vector-icons';
import { BlurView } from 'expo-blur';
import { useRouter } from 'expo-router';
import { type ReactNode, useEffect } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import Animated, {
  Extrapolation,
  type SharedValue,
  interpolate,
  useAnimatedProps,
  useAnimatedScrollHandler,
  useAnimatedStyle,
  useDerivedValue,
  useSharedValue,
} from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { t } from '../i18n';
import { pressScale } from '../motion';
import { useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { MiniPlayer } from './MiniPlayer';
import { Press } from './Press';

/** How far past the point a bar's background starts to show it is fully solid. */
const FADE_SPAN = 16;
/** How much a large title grows when the page is pulled down past its top, as iOS does. */
const STRETCH = 0.08;
const STRETCH_PULL = 120;
/** How strongly a frosted bar blurs what passes under it. */
const GLASS_BLUR = 80;

/** What a page hands the list it draws: the room the bars take and the scroll it follows. */
export type Frame = {
  /** The height of everything over the list: the status bar, the bar and the header. */
  top: number;
  /** The height of everything under it: the mini player and the tab bar. */
  bottom: number;
  /** How far the list is scrolled from its top, 0 when it rests there. */
  scrollY: SharedValue<number>;
  onScroll: ReturnType<typeof useAnimatedScrollHandler>;
};

const AnimatedBlur = Animated.createAnimatedComponent(BlurView);

/**
 * A frosted surface, for the bars things scroll under. With `reveal` (0 to 1) it frosts over as
 * the value rises. The blur's intensity animates rather than its opacity, since a blur under a
 * fading parent draws wrong on iOS.
 */
export function Glass({
  reveal,
  edge = true,
}: {
  reveal?: SharedValue<number>;
  /** Whether a hairline marks the bottom edge, where rows pass under it. */
  edge?: boolean;
}) {
  const { colors } = useTheme();
  const styles = useStyles();
  const blur = useAnimatedProps(() => ({ intensity: GLASS_BLUR * (reveal ? reveal.value : 1) }));
  const tint = useAnimatedStyle(() => ({ opacity: reveal ? reveal.value : 1 }));
  return (
    <View style={StyleSheet.absoluteFill} pointerEvents="none">
      <AnimatedBlur
        tint="systemChromeMaterialDark"
        animatedProps={blur}
        style={StyleSheet.absoluteFill}
      />
      <Animated.View style={[StyleSheet.absoluteFill, { backgroundColor: colors.glass }, tint]} />
      {edge && <Animated.View style={[styles.hairline, tint]} />}
    </View>
  );
}

/**
 * How much room the floating chrome takes at the bottom of the screen: the tab bar on a tab,
 * the mini player while something plays, and the home indicator.
 */
export function useBottomChrome(tabs: boolean): number {
  const insets = useSafeAreaInsets();
  const playing = useStore((s) => s.player?.track != null);
  return (
    insets.bottom +
    (tabs ? sizes.tabBar : 0) +
    (playing ? sizes.miniPlayer + spacing.sm * 2 : 0) +
    spacing.md
  );
}

/**
 * The props a list needs to scroll under a page's bars: its insets, where it starts, and the
 * scroll handler that drives the header. The insets sit outside the content, so the pull to
 * refresh spinner shows just under the header rather than behind it.
 */
export function framed(frame: Frame) {
  return {
    onScroll: frame.onScroll,
    scrollEventThrottle: 16,
    contentInset: { top: frame.top, bottom: frame.bottom },
    contentOffset: { x: 0, y: -frame.top },
    scrollIndicatorInsets: { top: frame.top, bottom: frame.bottom },
    contentInsetAdjustmentBehavior: 'never' as const,
    automaticallyAdjustContentInsets: false,
  };
}

/**
 * A page whose list scrolls under frosted bars, the iOS way. With `large` the title opens big
 * above the list, scrolls up with it and settles small in the bar, which frosts over once rows
 * pass under it. Without, the bar stays clear until the list scrolls past `revealAt`, as on a
 * detail page whose artwork leads. `accessory`, such as a row of chips, rides under the title
 * and stays pinned under the bar; `accessoryHeight` must say how tall it is, since the list's
 * inset is set before anything is measured. `children` draws the list from the `Frame`, and
 * `framed` turns that into its props.
 */
export function Page({
  title,
  large = false,
  back = false,
  right,
  accessory,
  accessoryHeight = 0,
  revealAt = 0,
  tabs = false,
  resetKey,
  children,
}: {
  title?: string;
  large?: boolean;
  back?: boolean;
  right?: ReactNode;
  accessory?: ReactNode;
  accessoryHeight?: number;
  /** Without `large`, how far the list scrolls before the bar frosts and shows the title. */
  revealAt?: number;
  /** Whether the page is a tab, under which the tab bar floats; a stack page draws the mini player. */
  tabs?: boolean;
  /**
   * Whatever remounts the page's list, such as the chips that pick what it shows. A new list
   * starts at its top without telling the header, so a change of this puts the header back.
   */
  resetKey?: string;
  children: (frame: Frame) => ReactNode;
}) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const theme = useTheme();
  const styles = useStyles();
  const bottom = useBottomChrome(tabs);
  const titleHeight = large ? Math.round(theme.typography.hero * 1.25) + spacing.xs : 0;
  const bar = insets.top + sizes.topBar;
  const top = bar + titleHeight + accessoryHeight;
  const scrollY = useSharedValue(0);
  useEffect(() => {
    scrollY.value = 0;
  }, [resetKey, scrollY]);
  const onScroll = useAnimatedScrollHandler(
    (event) => {
      scrollY.value = event.contentOffset.y + top;
    },
    [top],
  );
  // Where the bar turns solid: once the large title has gone under it, or at `revealAt`.
  const solidAt = large ? titleHeight : revealAt;

  const header = useAnimatedStyle(() => ({
    transform: [{ translateY: -Math.min(scrollY.value, titleHeight) }],
  }));
  const largeTitle = useAnimatedStyle(() => ({
    opacity: interpolate(scrollY.value, [0, titleHeight * 0.7], [1, 0], Extrapolation.CLAMP),
    transform: [
      {
        scale: interpolate(
          scrollY.value,
          [-STRETCH_PULL, 0],
          [1 + STRETCH, 1],
          Extrapolation.CLAMP,
        ),
      },
    ],
  }));
  const solid = useDerivedValue(() =>
    interpolate(scrollY.value, [solidAt - FADE_SPAN, solidAt], [0, 1], Extrapolation.CLAMP),
  );
  const smallTitle = useAnimatedStyle(() => ({
    opacity: interpolate(
      scrollY.value,
      [solidAt - FADE_SPAN, solidAt + FADE_SPAN / 2],
      [0, 1],
      Extrapolation.CLAMP,
    ),
  }));

  return (
    <View style={styles.page}>
      {children({ top, bottom, scrollY, onScroll })}

      {(large || accessory) && (
        <Animated.View pointerEvents="box-none" style={[styles.header, { top: bar }, header]}>
          {large && (
            <Animated.View style={[styles.largeTitle, { height: titleHeight }, largeTitle]}>
              <Text numberOfLines={1} style={styles.hero}>
                {title}
              </Text>
            </Animated.View>
          )}
          {accessory && (
            <View style={{ height: accessoryHeight }}>
              <Glass reveal={solid} />
              {accessory}
            </View>
          )}
        </Animated.View>
      )}

      <View pointerEvents="box-none" style={[styles.bar, { height: bar }]}>
        <Glass reveal={solid} edge={!accessory} />
        <View style={[styles.barRow, { marginTop: insets.top }]}>
          <View style={styles.side}>
            {back && (
              <Press
                scale={pressScale.button}
                hitSlop={spacing.md}
                onPress={() => router.back()}
                accessibilityLabel={t('mobile-back')}
                style={styles.round}
              >
                <Ionicons name="chevron-back" size={sizes.icon} color={theme.colors.text} />
              </Press>
            )}
          </View>
          <Animated.Text numberOfLines={1} style={[styles.small, smallTitle]}>
            {title}
          </Animated.Text>
          <View style={[styles.side, styles.right]}>{right}</View>
        </View>
      </View>

      {!tabs && (
        <View
          pointerEvents="box-none"
          style={[styles.dock, { paddingBottom: insets.bottom + spacing.xs }]}
        >
          <MiniPlayer />
        </View>
      )}
    </View>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  page: { flex: 1, backgroundColor: colors.background },
  header: { position: 'absolute', left: 0, right: 0 },
  largeTitle: {
    justifyContent: 'flex-end',
    paddingHorizontal: spacing.lg,
    paddingBottom: spacing.xs,
    transformOrigin: 'left',
  },
  hero: {
    color: colors.text,
    fontSize: typography.hero,
    fontWeight: '800',
    letterSpacing: -0.6,
  },
  bar: { position: 'absolute', top: 0, left: 0, right: 0 },
  dock: { position: 'absolute', left: 0, right: 0, bottom: 0 },
  barRow: {
    height: sizes.topBar,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.sm,
  },
  side: { minWidth: sizes.topBar, flexDirection: 'row', alignItems: 'center', gap: spacing.sm },
  right: { justifyContent: 'flex-end', paddingRight: spacing.sm },
  small: {
    flex: 1,
    textAlign: 'center',
    color: colors.text,
    fontSize: typography.title,
    fontWeight: '600',
  },
  // A dark disc behind the back chevron, so it reads over bright artwork too.
  round: {
    width: sizes.topBar - spacing.sm,
    height: sizes.topBar - spacing.sm,
    borderRadius: radii.pill,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.scrim,
  },
  hairline: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
    height: StyleSheet.hairlineWidth,
    backgroundColor: colors.border,
  },
}));
