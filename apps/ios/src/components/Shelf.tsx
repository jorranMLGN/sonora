import { useRouter } from 'expo-router';
import { memo } from 'react';
import { FlatList, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { pressScale } from '../motion';
import { makeStyles } from '../styles';
import { sizes, spacing } from '../theme';
import { Artwork } from './Artwork';
import { Press } from './Press';

export type Kind = 'album' | 'playlist' | 'artist';

/** Navigates to the detail page of an album, playlist or artist. */
export function useOpen() {
  const router = useRouter();
  return (kind: Kind, id: string) => router.push({ pathname: `/${kind}/[id]`, params: { id } });
}

/**
 * A cover with its title and subtitle under it, for a shelf or the library's grid; artists are
 * round and centred. `size` is the cover's side, the shelf's tile size unless a grid sets it.
 */
export const Tile = memo(function Tile({
  item,
  kind,
  size = sizes.tileArtwork,
}: {
  item: Track;
  kind: Kind;
  size?: number;
}) {
  const open = useOpen();
  const styles = useStyles();
  const artist = kind === 'artist';
  return (
    <Press
      scale={pressScale.tile}
      style={[{ width: size }, artist && styles.centred]}
      onPress={() => open(kind, item.id)}
    >
      <View style={styles.cover}>
        <Artwork uri={item.artwork} size={size} round={artist} kind={kind} seed={item.title} />
      </View>
      <Text numberOfLines={1} style={[styles.title, artist && styles.centredText]}>
        {item.title}
      </Text>
      {item.subtitle !== '' && (
        <Text numberOfLines={1} style={[styles.subtitle, artist && styles.centredText]}>
          {item.subtitle}
        </Text>
      )}
    </Press>
  );
});

/**
 * A titled horizontal list of albums, playlists or artists. It snaps tile by tile, so a flick
 * always comes to rest on a cover's edge.
 */
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
        snapToInterval={sizes.tileArtwork + spacing.md}
        decelerationRate="fast"
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
    letterSpacing: -0.4,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.md,
  },
  content: { paddingHorizontal: spacing.lg, gap: spacing.md },
  cover: {
    shadowColor: colors.overlay,
    shadowOpacity: 1,
    shadowRadius: spacing.md,
    shadowOffset: { width: 0, height: spacing.xs },
  },
  centred: { alignItems: 'center' },
  centredText: { textAlign: 'center' },
  title: {
    color: colors.text,
    fontSize: typography.label,
    fontWeight: '600',
    marginTop: spacing.sm,
  },
  subtitle: { color: colors.textDim, fontSize: typography.caption, marginTop: spacing.xs / 2 },
}));
