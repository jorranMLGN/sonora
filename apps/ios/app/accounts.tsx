import { ScrollView } from 'react-native';

import { ProviderCard } from '../src/components/ProviderCard';
import { Screen } from '../src/components/Screen';
import { t } from '../src/i18n';
import { useStore } from '../src/store';
import { makeStyles } from '../src/styles';
import { spacing } from '../src/theme';

export default function Accounts() {
  const providers = useStore((s) => s.providers);
  const styles = useStyles();
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

const useStyles = makeStyles(() => ({
  content: { padding: spacing.lg, gap: spacing.md },
}));
