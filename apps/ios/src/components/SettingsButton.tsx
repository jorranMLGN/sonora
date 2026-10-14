import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';

import { t } from '../i18n';
import { pressScale } from '../motion';
import { useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Press } from './Press';

/** The gear at the top right of every tab, opening Settings. */
export function SettingsButton() {
  const router = useRouter();
  const { colors } = useTheme();
  return (
    <Press
      scale={pressScale.button}
      hitSlop={spacing.md}
      onPress={() => router.push('/settings')}
      accessibilityLabel={t('mobile-settings')}
    >
      <Ionicons name="settings-outline" size={sizes.icon} color={colors.text} />
    </Press>
  );
}
