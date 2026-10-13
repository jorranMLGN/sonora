import { Text } from 'react-native';

import { useSettings } from '../settings';
import { useStore } from '../store';
import { makeStyles } from '../styles';
import { spacing } from '../theme';

/** The name of the service an item comes from, as a small tag; only drawn when Settings asks. */
export function ProviderBadge({ slug }: { slug: string }) {
  const styles = useStyles();
  const shown = useSettings((s) => s.providerBadges);
  const name = useStore((s) => s.providers.find((p) => p.slug === slug)?.name ?? slug);
  if (!shown || slug === '') return null;
  return (
    <Text numberOfLines={1} style={styles.badge}>
      {name}
    </Text>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  badge: {
    alignSelf: 'flex-start',
    overflow: 'hidden',
    color: colors.accent,
    backgroundColor: colors.accentSoft,
    borderRadius: radii.small,
    paddingHorizontal: spacing.xs,
    fontSize: typography.tag,
    fontWeight: '700',
    marginTop: spacing.xs / 2,
  },
}));
