import { Image } from 'expo-image';
import { StyleSheet, View } from 'react-native';

import { colors, radii } from '../theme';

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
  const box = { width: size, height: size, borderRadius: round ? size / 2 : radii.small };
  return uri ? (
    <Image source={{ uri }} style={[styles.image, box]} contentFit="cover" transition={150} />
  ) : (
    <View style={[styles.image, box]} />
  );
}

const styles = StyleSheet.create({
  image: { backgroundColor: colors.placeholder },
});
