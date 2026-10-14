import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { type ReactNode, useEffect } from 'react';
import { Pressable, Text, View } from 'react-native';
import Animated, { FadeInUp, FadeOutUp } from 'react-native-reanimated';
import { SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';

import { t } from '../i18n';
import { haptic, springs } from '../motion';
import { useStore } from '../store';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { MiniPlayer } from './MiniPlayer';

/** How long an error stays up before it goes by itself. */
const ERROR_SHOWN_MS = 6000;

/** Dismissible error banner fed by the store. */
export function ErrorBanner() {
  const error = useStore((s) => s.error);
  const clear = useStore((s) => s.clearError);
  const styles = useStyles();
  // A toast: it goes by itself after a while, or at once on a tap.
  useEffect(() => {
    if (!error) return;
    const timer = setTimeout(clear, ERROR_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [error, clear]);
  if (!error) return null;
  return (
    <Pressable style={styles.error} onPress={clear} accessibilityLabel={t('mobile-dismiss')}>
      <Text style={styles.errorText}>{error}</Text>
    </Pressable>
  );
}

/**
 * The error toast: it drops in from under the status bar over whatever is showing, taps an
 * error haptic, and goes by itself after a while or at once on a tap. The root layout draws one
 * over every page, and the player sheet its own, since a sheet covers the root; that one is
 * `quiet`, as the root's still taps and times the error while the sheet is up.
 */
export function Toast({ quiet = false }: { quiet?: boolean }) {
  const error = useStore((s) => s.error);
  const clear = useStore((s) => s.clearError);
  const insets = useSafeAreaInsets();
  const { colors } = useTheme();
  const styles = useStyles();
  useEffect(() => {
    if (!error || quiet) return;
    haptic.error();
    const timer = setTimeout(clear, ERROR_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [error, clear, quiet]);
  if (!error) return null;
  return (
    <Animated.View
      key={error}
      entering={FadeInUp.springify()
        .duration(springs.snappy.duration)
        .dampingRatio(springs.snappy.dampingRatio)}
      exiting={FadeOutUp.duration(springs.press.duration)}
      style={[styles.toast, { top: insets.top + spacing.xs }]}
    >
      <Pressable style={styles.toastBody} onPress={clear} accessibilityLabel={t('mobile-dismiss')}>
        <Ionicons name="alert-circle" size={sizes.icon} color={colors.danger} />
        <Text style={styles.toastText}>{error}</Text>
      </Pressable>
    </Animated.View>
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
  const { colors } = useTheme();
  const styles = useStyles();
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
      <View style={styles.body}>{children}</View>
      {miniPlayer && <MiniPlayer />}
    </SafeAreaView>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  screen: { flex: 1, backgroundColor: colors.background },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  title: {
    flex: 1,
    color: colors.text,
    fontSize: typography.display,
    fontWeight: '800',
    letterSpacing: -0.4,
  },
  body: { flex: 1 },
  error: { backgroundColor: colors.danger, padding: spacing.md },
  errorText: { color: colors.onAccent, fontSize: typography.caption },
  toast: { position: 'absolute', left: spacing.md, right: spacing.md },
  toastBody: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    padding: spacing.md,
    borderRadius: radii.large,
    backgroundColor: colors.surfaceRaised,
    borderWidth: sizes.hairline,
    borderColor: colors.border,
    shadowColor: colors.overlay,
    shadowOpacity: 1,
    shadowRadius: spacing.lg,
    shadowOffset: { width: 0, height: spacing.sm },
  },
  toastText: { flex: 1, color: colors.text, fontSize: typography.label, fontWeight: '500' },
}));
