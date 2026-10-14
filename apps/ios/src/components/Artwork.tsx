import { Ionicons } from '@expo/vector-icons';
import { Image } from 'expo-image';
import { LinearGradient } from 'expo-linear-gradient';
import { useState } from 'react';

import { durations } from '../motion';
import { type ArtworkShape, useReduceMotion } from '../settings';
import { makeStyles, useTheme } from '../styles';
import { artworkWash, sizes } from '../theme';

/** What a cover belongs to, which picks the glyph a missing one is drawn with. */
export type ArtworkKind = 'track' | 'album' | 'playlist' | 'artist';

type IconName = keyof typeof Ionicons.glyphMap;

const GLYPHS: Record<ArtworkKind, IconName> = {
  track: 'musical-note',
  album: 'disc',
  playlist: 'musical-notes',
  artist: 'person',
};

/**
 * Artwork that fades in over a flat placeholder. It is square with small corners unless `round`
 * makes it a circle or `shape` picks one of the player's shapes. With no image, or one that will
 * not load, it draws a gradient picked by `seed` (the item's title) with a glyph for its `kind`,
 * so a playlist without a cover still looks made rather than missing. `onError` hears a failed
 * load, so a caller can fall back to another url before the gradient shows.
 */
export function Artwork({
  uri,
  size,
  round = false,
  shape,
  kind = 'track',
  seed,
  onError,
}: {
  uri: string | null;
  size: number;
  round?: boolean;
  shape?: ArtworkShape;
  kind?: ArtworkKind;
  /** What picks the fallback's gradient; the url's absence makes the title a good one. */
  seed?: string;
  onError?: () => void;
}) {
  const theme = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  // The url that would not load, so the fallback draws until the url changes.
  const [broken, setBroken] = useState<string | null>(null);
  const circle = round || shape === 'circle';
  const corner = shape === 'rounded' ? theme.radii.large : theme.radii.small;
  const box = { width: size, height: size, borderRadius: circle ? size / 2 : corner };
  if (uri && uri !== broken) {
    return (
      <Image
        source={{ uri }}
        recyclingKey={uri}
        cachePolicy="memory-disk"
        style={[styles.image, box]}
        contentFit="cover"
        transition={still ? 0 : { duration: durations.fade, effect: 'cross-dissolve' }}
        onError={() => {
          if (onError) onError();
          else setBroken(uri);
        }}
      />
    );
  }
  const [from, to] = artworkWash(theme, seed ?? kind);
  return (
    <LinearGradient
      colors={[from, to]}
      start={{ x: 0, y: 0 }}
      end={{ x: 1, y: 1 }}
      style={[styles.fallback, box]}
    >
      <Ionicons
        name={GLYPHS[kind]}
        size={Math.round(size * sizes.glyphShare)}
        color={theme.colors.onArtwork}
      />
    </LinearGradient>
  );
}

const useStyles = makeStyles(({ colors }) => ({
  image: { backgroundColor: colors.placeholder },
  fallback: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden' },
}));
