import { Pressable, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { makeStyles } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { type Kind, useOpen } from './Shelf';

/** A list row for an album, playlist or artist; opens its page. */
export function EntityRow({ item, kind }: { item: Track; kind: Kind }) {
  const open = useOpen();
  const styles = useStyles();
  return (
    <Pressable style={styles.row} onPress={() => open(kind, item.id)}>
      <Artwork uri={item.artwork} size={sizes.rowArtwork} round={kind === 'artist'} />
      <View style={styles.meta}>
        <Text numberOfLines={1} style={styles.title}>
          {item.title}
        </Text>
        <Text numberOfLines={1} style={styles.subtitle}>
          {item.subtitle}
        </Text>
      </View>
    </Pressable>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    gap: spacing.md,
  },
  meta: { flex: 1 },
  title: { color: colors.text, fontSize: typography.body, fontWeight: '500' },
  subtitle: { color: colors.textDim, fontSize: typography.caption },
}));
