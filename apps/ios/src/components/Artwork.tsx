import { Image } from 'expo-image';
import { View } from 'react-native';

import { type ArtworkShape, useReduceMotion } from '../settings';
import { makeStyles, useTheme } from '../styles';

/** How long a loaded image takes to fade in over its placeholder. */
const FADE_MS = 150;

/**
 * Artwork with a flat placeholder when there is no image. It is square with small corners unless
 * `round` makes it a circle or `shape` picks one of the player's shapes. `onError` hears when
 * the image cannot load, so a caller can fall back to another url.
 */
export function Artwork({
  uri,
  size,
  round = false,
  shape,
  onError,
}: {
  uri: string | null;
  size: number;
  round?: boolean;
  shape?: ArtworkShape;
  onError?: () => void;
}) {
  const { radii } = useTheme();
  const styles = useStyles();
  const still = useReduceMotion();
  const circle = round || shape === 'circle';
  const corner = shape === 'rounded' ? radii.large : radii.small;
  const box = { width: size, height: size, borderRadius: circle ? size / 2 : corner };
  return uri ? (
    <Image
      source={{ uri }}
      style={[styles.image, box]}
      contentFit="cover"
      transition={still ? 0 : FADE_MS}
      onError={onError}
    />
  ) : (
    <View style={[styles.image, box]} />
  );
}

const useStyles = makeStyles(({ colors }) => ({
  image: { backgroundColor: colors.placeholder },
}));
