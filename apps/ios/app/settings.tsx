import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { build } from '../src/build';
import { Screen } from '../src/components/Screen';
import { t } from '../src/i18n';
import { useStore } from '../src/store';
import { colors, radii, sizes, spacing, typography } from '../src/theme';

type IconName = keyof typeof Ionicons.glyphMap;

/** One row of Settings: an icon, a label, an optional line under it, and a chevron. */
function Row({
  icon,
  label,
  detail,
  onPress,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  onPress: () => void;
}) {
  return (
    <Pressable style={({ pressed }) => [styles.row, pressed && styles.pressed]} onPress={onPress}>
      <Ionicons name={icon} size={sizes.icon} color={colors.accent} />
      <View style={styles.rowText}>
        <Text style={styles.label}>{label}</Text>
        {detail && <Text style={styles.detail}>{detail}</Text>}
      </View>
      <Ionicons name="chevron-forward" size={sizes.icon * 0.75} color={colors.textDim} />
    </Pressable>
  );
}

/** The lines that say which build this is, or that it is a development build. */
function buildLines(): string[] {
  if (!build.version && !build.number) return [t('mobile-build-dev')];
  const lines: string[] = [];
  if (build.version) lines.push(t('mobile-build-version', { version: build.version }));
  if (build.number) {
    const number = t('mobile-build-number', { number: build.number });
    lines.push(build.commit ? `${number} · ${build.commit}` : number);
  }
  return lines;
}

export default function Settings() {
  const router = useRouter();
  const providers = useStore((s) => s.providers);
  const signedIn = providers.filter((p) => p.signedIn).map((p) => p.name);
  return (
    <Screen back title={t('mobile-settings')} miniPlayer>
      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.group}>
          <Row
            icon="person-circle-outline"
            label={t('mobile-settings-accounts')}
            detail={signedIn.length > 0 ? signedIn.join(', ') : t('mobile-settings-accounts-detail')}
            onPress={() => router.push('/accounts')}
          />
          <View style={styles.divider} />
          <Row
            icon="sparkles-outline"
            label={t('mobile-settings-intro')}
            onPress={() => router.push('/intro')}
          />
        </View>
        <Text style={styles.heading}>{t('mobile-settings-about')}</Text>
        <View style={styles.about}>
          <Text style={styles.name}>Sonora</Text>
          {buildLines().map((line) => (
            <Text key={line} selectable style={styles.detail}>
              {line}
            </Text>
          ))}
        </View>
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing.lg, gap: spacing.md },
  group: { backgroundColor: colors.surface, borderRadius: radii.large, overflow: 'hidden' },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  pressed: { backgroundColor: colors.surfaceRaised },
  rowText: { flex: 1, gap: spacing.xs / 2 },
  label: { color: colors.text, fontSize: typography.body, fontWeight: '600' },
  detail: { color: colors.textDim, fontSize: typography.caption },
  divider: {
    height: sizes.hairline,
    backgroundColor: colors.border,
    marginLeft: spacing.lg + sizes.icon + spacing.md,
  },
  heading: {
    color: colors.textDim,
    fontSize: typography.caption,
    fontWeight: '600',
    textTransform: 'uppercase',
    marginTop: spacing.md,
    marginLeft: spacing.xs,
  },
  about: {
    backgroundColor: colors.surface,
    borderRadius: radii.large,
    padding: spacing.lg,
    gap: spacing.xs,
  },
  name: { color: colors.text, fontSize: typography.title, fontWeight: '700' },
});
