import { Ionicons } from '@expo/vector-icons';
import { memo } from 'react';
import { Text, View } from 'react-native';

import type { Track } from '../bridge';
import { makeStyles, useTheme } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { Press } from './Press';
import { ProviderBadge } from './ProviderBadge';
import { type Kind, useOpen } from './Shelf';

/** A list row for an album, playlist or artist, with a chevron; a tap opens its page. */
export const EntityRow = memo(function EntityRow({ item, kind }: { item: Track; kind: Kind }) {
  const open = useOpen();
  const { colors } = useTheme();
  const styles = useStyles();
  return (
    <Press highlight style={styles.row} onPress={() => open(kind, item.id)}>
      <Artwork
        uri={item.artwork}
        size={sizes.rowArtwork}
        round={kind === 'artist'}
        kind={kind}
        seed={item.title}
      />
      <View style={styles.meta}>
        <Text numberOfLines={1} style={styles.title}>
          {item.title}
        </Text>
        {item.subtitle !== '' && (
          <Text numberOfLines={1} style={styles.subtitle}>
            {item.subtitle}
          </Text>
        )}
        <ProviderBadge slug={item.provider} />
      </View>
      <Ionicons name="chevron-forward" size={sizes.iconSmall} color={colors.border} />
    </Press>
  );
});

const useStyles = makeStyles(({ colors, typography }) => ({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    gap: spacing.md,
  },
  meta: { flex: 1, gap: spacing.xs / 2 },
  title: { color: colors.text, fontSize: typography.row, fontWeight: '500', letterSpacing: -0.2 },
  subtitle: { color: colors.textDim, fontSize: typography.label },
}));
