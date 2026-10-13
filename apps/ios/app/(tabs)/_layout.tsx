import { Ionicons } from '@expo/vector-icons';
import { Redirect } from 'expo-router';
import { TabList, TabSlot, TabTrigger, Tabs } from 'expo-router/ui';
import { forwardRef } from 'react';
import { Pressable, type PressableProps, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { MiniPlayer } from '../../src/components/MiniPlayer';
import { t } from '../../src/i18n';
import { flag } from '../../src/preferences';
import { colors, sizes, spacing, typography } from '../../src/theme';

type IconName = keyof typeof Ionicons.glyphMap;

type ButtonProps = PressableProps & { isFocused?: boolean; icon: IconName; label: string };

const TabButton = forwardRef<View, ButtonProps>(function TabButton(
  { isFocused, icon, label, ...rest },
  ref,
) {
  const tint = isFocused ? colors.accent : colors.textDim;
  return (
    <Pressable ref={ref} {...rest} style={styles.button}>
      <Ionicons name={icon} size={sizes.icon} color={tint} />
      <Text style={[styles.label, { color: tint }]}>{label}</Text>
    </Pressable>
  );
});

export default function TabsLayout() {
  const insets = useSafeAreaInsets();
  // A first launch walks through the intro before anything else.
  if (!flag('introSeen')) return <Redirect href="/intro" />;
  return (
    <Tabs style={styles.root}>
      <TabSlot style={styles.slot} />
      <MiniPlayer />
      <TabList style={[styles.bar, { paddingBottom: insets.bottom }]}>
        <TabTrigger name="home" href="/" asChild>
          <TabButton icon="home" label={t('mobile-tab-home')} />
        </TabTrigger>
        <TabTrigger name="search" href="/search" asChild>
          <TabButton icon="search" label={t('mobile-tab-search')} />
        </TabTrigger>
        <TabTrigger name="library" href="/library" asChild>
          <TabButton icon="library" label={t('mobile-tab-library')} />
        </TabTrigger>
      </TabList>
    </Tabs>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: colors.background },
  slot: { flex: 1 },
  bar: {
    backgroundColor: colors.surface,
    borderTopWidth: sizes.hairline,
    borderTopColor: colors.border,
  },
  button: {
    flex: 1,
    height: sizes.tabBar,
    alignItems: 'center',
    justifyContent: 'center',
    gap: spacing.xs / 2,
  },
  label: { fontSize: typography.caption },
});
