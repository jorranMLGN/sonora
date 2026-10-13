import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { t } from '../i18n';
import { useStore } from '../store';
import { colors, sizes, spacing, typography } from '../theme';
import { MiniPlayer } from './MiniPlayer';

/** Dismissible error banner fed by the store. */
export function ErrorBanner() {
  const error = useStore((s) => s.error);
  const clear = useStore((s) => s.clearError);
  if (!error) return null;
  return (
    <Pressable style={styles.error} onPress={clear} accessibilityLabel={t('mobile-dismiss')}>
      <Text style={styles.errorText}>{error}</Text>
    </Pressable>
  );
}

/**
 * Page frame: safe area, an optional back header with a title, the error banner, and the mini
 * player at the bottom for stack pages that sit above the tab bar.
 */
export function Screen({
  title,
  back = false,
  miniPlayer = false,
  right,
  children,
}: {
  title?: string;
  back?: boolean;
  right?: ReactNode;
  miniPlayer?: boolean;
  children: ReactNode;
}) {
  const router = useRouter();
  return (
    <SafeAreaView style={styles.screen} edges={miniPlayer ? ['top', 'bottom'] : ['top']}>
      {(back || title) && (
        <View style={styles.header}>
          {back && (
            <Pressable
              hitSlop={spacing.md}
              onPress={() => router.back()}
              accessibilityLabel={t('mobile-back')}
            >
              <Ionicons name="chevron-back" size={sizes.icon} color={colors.text} />
            </Pressable>
          )}
          {title && (
            <Text numberOfLines={1} style={styles.title}>
              {title}
            </Text>
          )}
          {right}
        </View>
      )}
      <ErrorBanner />
      <View style={styles.body}>{children}</View>
      {miniPlayer && <MiniPlayer />}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  title: { flex: 1, color: colors.text, fontSize: typography.display, fontWeight: '700' },
  body: { flex: 1 },
  error: { backgroundColor: colors.danger, padding: spacing.md },
  errorText: { color: colors.onAccent, fontSize: typography.caption },
});
