import { ScrollView, StyleSheet } from 'react-native';

import { ProviderCard } from '../src/components/ProviderCard';
import { Screen } from '../src/components/Screen';
import { t } from '../src/i18n';
import { useStore } from '../src/store';
import { spacing } from '../src/theme';

export default function Accounts() {
  const providers = useStore((s) => s.providers);
  return (
    <Screen back title={t('mobile-accounts')} miniPlayer>
      <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled">
        {providers.map((provider) => (
          <ProviderCard key={provider.slug} provider={provider} />
        ))}
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing.lg, gap: spacing.md },
});
