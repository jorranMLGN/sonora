import { Image } from 'expo-image';
import { View } from 'react-native';

import { type ArtworkShape, useReduceMotion } from '../settings';
import { makeStyles, useTheme } from '../styles';

/** How long a loaded image takes to fade in over its placeholder. */
const FADE_MS = 150;

/**
 * Artwork with a flat placeholder when there is no image. It is square with small corners unless
 * `round` makes it a circle or `shape` picks one of the player's shapes.
 */
export function Artwork({
  uri,
  size,
  round = false,
  shape,
}: {
  uri: string | null;
  size: number;
  round?: boolean;
  shape?: ArtworkShape;
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
    />
  ) : (
    <View style={[styles.image, box]} />
  );
}

const useStyles = makeStyles(({ colors }) => ({
  image: { backgroundColor: colors.placeholder },
}));
