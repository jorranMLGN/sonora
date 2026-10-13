import { useRouter } from 'expo-router';
import { Alert, ScrollView, Text, View } from 'react-native';

import { build } from '../src/build';
import { Screen } from '../src/components/Screen';
import { ChoiceRow, Group, Row, SwatchRow, SwitchRow } from '../src/components/SettingsRows';
import { t } from '../src/i18n';
import { CHOICES, type SettingKey, type Settings as Values, useSettings } from '../src/settings';
import { useStore } from '../src/store';
import { makeStyles } from '../src/styles';
import { accents, spacing } from '../src/theme';

/** The Fluent key of each choice's label, where it is not `mobile-settings-<setting>-<value>`. */
const LABELS: Record<string, string> = {
  'corners-default': 'mobile-settings-default',
  'textSize-default': 'mobile-settings-default',
  'textSize-small': 'mobile-settings-text-size-small',
  'textSize-large': 'mobile-settings-text-size-large',
  'libraryTab-tracks': 'mobile-library-tracks',
  'libraryTab-albums': 'mobile-library-albums',
  'libraryTab-artists': 'mobile-library-artists',
  'libraryTab-playlists': 'mobile-library-playlists',
};

type Choice = keyof typeof CHOICES;

/** The segments for a setting with fixed choices, labelled in the current language. */
function options<K extends Choice>(key: K): { value: Values[K]; label: string }[] {
  return (CHOICES[key] as readonly unknown[] as readonly Values[K][]).map((value) => ({
    value,
    label: t(LABELS[`${key}-${value}`] ?? `mobile-settings-${key}-${value}`),
  }));
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

/** Asks before putting every setting back to its default. */
function confirmReset(reset: () => void) {
  Alert.alert(t('mobile-settings-reset-title'), t('mobile-settings-reset-detail'), [
    { text: t('mobile-cancel'), style: 'cancel' },
    { text: t('mobile-settings-reset-confirm'), style: 'destructive', onPress: reset },
  ]);
}

export default function Settings() {
  const router = useRouter();
  const styles = useStyles();
  const providers = useStore((s) => s.providers);
  const settings = useSettings();
  const set = <K extends SettingKey>(key: K) => (value: Values[K]) => settings.set(key, value);
  const signedIn = providers.filter((p) => p.signedIn).map((p) => p.name);
  const showOnHome = (slug: string, on: boolean) =>
    settings.set(
      'homeHidden',
      on
        ? settings.homeHidden.filter((hidden) => hidden !== slug)
        : [...settings.homeHidden, slug],
    );
  return (
    <Screen back title={t('mobile-settings')} miniPlayer>
      <ScrollView contentContainerStyle={styles.content}>
        <Group>
          <Row
            icon="person-circle-outline"
            label={t('mobile-settings-accounts')}
            detail={signedIn.length > 0 ? signedIn.join(', ') : t('mobile-settings-accounts-detail')}
            onPress={() => router.push('/accounts')}
          />
        </Group>

        <Group title={t('mobile-settings-appearance')}>
          <SwatchRow
            icon="color-palette-outline"
            label={t('mobile-settings-accent')}
            swatches={accents.map((accent) => ({
              color: accent.color,
              label: t(`mobile-accent-${accent.name}`),
            }))}
            value={settings.accent}
            onChange={set('accent')}
          />
          <ChoiceRow
            icon="moon-outline"
            label={t('mobile-settings-background')}
            options={options('background')}
            value={settings.background}
            onChange={set('background')}
          />
          <ChoiceRow
            icon="square-outline"
            label={t('mobile-settings-corners')}
            options={options('corners')}
            value={settings.corners}
            onChange={set('corners')}
          />
          <ChoiceRow
            icon="text-outline"
            label={t('mobile-settings-text-size')}
            options={options('textSize')}
            value={settings.textSize}
            onChange={set('textSize')}
          />
          <ChoiceRow
            icon="disc-outline"
            label={t('mobile-settings-artwork')}
            options={options('artwork')}
            value={settings.artwork}
            onChange={set('artwork')}
          />
          <SwitchRow
            icon="pause-circle-outline"
            label={t('mobile-settings-reduce-motion')}
            detail={t(
              settings.systemReduceMotion
                ? 'mobile-settings-reduce-motion-system'
                : 'mobile-settings-reduce-motion-detail',
            )}
            value={settings.reduceMotion}
            locked={settings.systemReduceMotion}
            onChange={set('reduceMotion')}
          />
        </Group>

        <Group title={t('mobile-settings-content')}>
          <SwitchRow
            icon="eye-off-outline"
            label={t('mobile-settings-hide-explicit')}
            detail={t('mobile-settings-hide-explicit-detail')}
            value={settings.hideExplicit}
            onChange={set('hideExplicit')}
          />
          <SwitchRow
            icon="pricetag-outline"
            label={t('mobile-settings-badges')}
            detail={t('mobile-settings-badges-detail')}
            value={settings.providerBadges}
            onChange={set('providerBadges')}
          />
        </Group>

        <Group title={t('mobile-settings-home')}>
          {providers.map((provider) => (
            <SwitchRow
              key={provider.slug}
              icon="albums-outline"
              label={provider.name}
              detail={t('mobile-settings-home-shelves')}
              value={!settings.homeHidden.includes(provider.slug)}
              onChange={(on) => showOnHome(provider.slug, on)}
            />
          ))}
          <ChoiceRow
            icon="library-outline"
            label={t('mobile-settings-library-tab')}
            options={options('libraryTab')}
            value={settings.libraryTab}
            onChange={set('libraryTab')}
          />
        </Group>

        <Group title={t('mobile-settings-advanced')}>
          <Row
            icon="sparkles-outline"
            label={t('mobile-settings-intro')}
            onPress={() => router.push('/intro')}
          />
          <Row
            icon="refresh-outline"
            label={t('mobile-settings-reset')}
            danger
            onPress={() => confirmReset(settings.reset)}
          />
        </Group>

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

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  content: { padding: spacing.lg, gap: spacing.md },
  detail: { color: colors.textDim, fontSize: typography.caption },
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
}));
