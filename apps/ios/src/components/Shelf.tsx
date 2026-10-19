import { useRouter } from 'expo-router';
import { FlatList, Pressable, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { makeStyles } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';

export type Kind = 'album' | 'playlist' | 'artist';

/** Navigates to the detail page of an album, playlist or artist. */
export function useOpen() {
  const router = useRouter();
  return (kind: Kind, id: string) => router.push({ pathname: `/${kind}/[id]`, params: { id } });
}

/** A tile in a horizontal shelf; artists are round. */
export function Tile({ item, kind }: { item: Track; kind: Kind }) {
  const open = useOpen();
  const styles = useStyles();
  return (
    <Pressable style={styles.tile} onPress={() => open(kind, item.id)}>
      <Artwork uri={item.artwork} size={sizes.tileArtwork} round={kind === 'artist'} />
      <Text numberOfLines={1} style={styles.title}>
        {item.title}
      </Text>
      <Text numberOfLines={1} style={styles.subtitle}>
        {item.subtitle}
      </Text>
    </Pressable>
  );
}

/** A titled horizontal list of albums, playlists or artists. */
export function Shelf({ title, items, kind }: { title: string; items: Track[]; kind: Kind }) {
  const styles = useStyles();
  if (items.length === 0) return null;
  return (
    <View style={styles.shelf}>
      <Text style={styles.heading}>{title}</Text>
      <FlatList
        horizontal
        showsHorizontalScrollIndicator={false}
        data={items}
        keyExtractor={(item) => item.id}
        contentContainerStyle={styles.content}
        renderItem={({ item }) => <Tile item={item} kind={kind} />}
      />
    </View>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  shelf: { marginBottom: spacing.xl },
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    marginHorizontal: spacing.lg,
    marginBottom: spacing.md,
  },
  content: { paddingHorizontal: spacing.lg, gap: spacing.md },
  tile: { width: sizes.tileArtwork },
  title: { color: colors.text, fontSize: typography.body, marginTop: spacing.sm },
  subtitle: { color: colors.textDim, fontSize: typography.caption },
}));
