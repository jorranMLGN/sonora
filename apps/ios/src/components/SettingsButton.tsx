import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { Pressable } from 'react-native';

import { t } from '../i18n';
import { colors, sizes, spacing } from '../theme';

/** The gear at the top right of every tab, opening Settings. */
export function SettingsButton() {
  const router = useRouter();
  return (
    <Pressable
      hitSlop={spacing.md}
      onPress={() => router.push('/settings')}
      accessibilityLabel={t('mobile-settings')}
    >
      <Ionicons name="settings-outline" size={sizes.icon} color={colors.text} />
    </Pressable>
  );
}
