import { Image } from 'expo-image';
import { View } from 'react-native';

import { makeStyles, useTheme } from '../styles';

/** Square artwork with a flat placeholder when there is no image. */
export function Artwork({
  uri,
  size,
  round = false,
}: {
  uri: string | null;
  size: number;
  round?: boolean;
}) {
  const { radii } = useTheme();
  const styles = useStyles();
  const box = { width: size, height: size, borderRadius: round ? size / 2 : radii.small };
  return uri ? (
    <Image source={{ uri }} style={[styles.image, box]} contentFit="cover" transition={150} />
  ) : (
    <View style={[styles.image, box]} />
  );
}

const useStyles = makeStyles(({ colors }) => ({
  image: { backgroundColor: colors.placeholder },
}));
