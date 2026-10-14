import { Ionicons } from '@expo/vector-icons';
import { Redirect } from 'expo-router';
import { TabList, TabSlot, TabTrigger, Tabs } from 'expo-router/ui';
import { forwardRef, useEffect } from 'react';
import { Pressable, type PressableProps, StyleSheet, Text, View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { MiniPlayer } from '../../src/components/MiniPlayer';
import { Glass } from '../../src/components/Page';
import { t } from '../../src/i18n';
import { haptic, springs } from '../../src/motion';
import { flag } from '../../src/preferences';
import { useReduceMotion } from '../../src/settings';
import { makeStyles, useTheme } from '../../src/styles';
import { sizes, spacing } from '../../src/theme';

type IconName = keyof typeof Ionicons.glyphMap;

/** How far a tab's icon dips when it is chosen, before it springs back. */
const CHOSEN_DIP = 0.82;

type ButtonProps = PressableProps & {
  isFocused?: boolean;
  icon: IconName;
  /** The outlined glyph drawn while the tab is not the one showing. */
  idle: IconName;
  label: string;
};

/** A tab: the filled glyph in the accent while it shows, which dips and springs when chosen. */
const TabButton = forwardRef<View, ButtonProps>(function TabButton(
  { isFocused = false, icon, idle, label, onPress, ...rest },
  ref,
) {
  const { colors } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const scale = useSharedValue(1);
  const tint = isFocused ? colors.accent : colors.textDim;
  useEffect(() => {
    if (!isFocused || still) return;
    scale.value = withSequence(
      withTiming(CHOSEN_DIP, { duration: 90 }),
      withSpring(1, springs.bouncy),
    );
  }, [isFocused, still, scale]);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));
  return (
    <Pressable
      ref={ref}
      {...rest}
      onPress={(event) => {
        if (!isFocused) haptic.select();
        onPress?.(event);
      }}
      style={styles.button}
      accessibilityRole="tab"
      accessibilityState={{ selected: isFocused }}
    >
      <Animated.View style={animated}>
        <Ionicons name={isFocused ? icon : idle} size={sizes.icon} color={tint} />
      </Animated.View>
      <Text style={[styles.label, { color: tint }]}>{label}</Text>
    </Pressable>
  );
});

/**
 * The tabs. The page runs under the chrome to the bottom of the screen; over it float the mini
 * player and the frosted tab bar, so rows scroll under both the way they do in Apple's apps.
 */
export default function TabsLayout() {
  const insets = useSafeAreaInsets();
  const styles = useStyles();
  // A first launch walks through the intro before anything else.
  if (!flag('introSeen')) return <Redirect href="/intro" />;
  return (
    <Tabs style={styles.root}>
      <TabSlot style={styles.slot} />
      <View
        pointerEvents="box-none"
        style={[styles.dock, { bottom: insets.bottom + sizes.tabBar + spacing.xs }]}
      >
        <MiniPlayer />
      </View>
      {/* The triggers must be the TabList's own children, and the list a child of Tabs. */}
      <TabList style={[styles.bar, { paddingBottom: insets.bottom }]}>
        <Glass edge={false} />
        <View style={styles.hairline} />
        <TabTrigger name="home" href="/" asChild>
          <TabButton icon="home" idle="home-outline" label={t('mobile-tab-home')} />
        </TabTrigger>
        <TabTrigger name="search" href="/search" asChild>
          <TabButton icon="search" idle="search-outline" label={t('mobile-tab-search')} />
        </TabTrigger>
        <TabTrigger name="library" href="/library" asChild>
          <TabButton icon="library" idle="library-outline" label={t('mobile-tab-library')} />
        </TabTrigger>
      </TabList>
    </Tabs>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  root: { flex: 1, backgroundColor: colors.background },
  slot: { flex: 1 },
  dock: { position: 'absolute', left: 0, right: 0 },
  bar: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
    overflow: 'hidden',
    backgroundColor: colors.transparent,
  },
  hairline: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    height: StyleSheet.hairlineWidth,
    backgroundColor: colors.border,
  },
  button: {
    flex: 1,
    height: sizes.tabBar,
    alignItems: 'center',
    justifyContent: 'center',
    gap: spacing.xs / 2,
  },
  label: { fontSize: typography.tag, fontWeight: '600' },
}));
