import { useLocalSearchParams } from 'expo-router';
import { FlatList, Text, View } from 'react-native';

import { bridge } from '../../src/bridgeImpl';
import { DetailHeader, Loading, useCurrentId } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { Shelf } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { attempt } from '../../src/store';
import { makeStyles } from '../../src/styles';
import { spacing } from '../../src/theme';
import { useLoad } from '../../src/useLoad';

export default function ArtistPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, loading, reload } = useLoad(() => bridge.artist(id), [id]);
  const currentId = useCurrentId();
  const styles = useStyles();
  return (
    <Screen back miniPlayer>
      {!data ? (
        <Loading />
      ) : (
        <FlatList
          data={data.topTracks}
          keyExtractor={(track, index) => `${index}:${track.id}`}
          refreshing={loading}
          onRefresh={reload}
          ListHeaderComponent={
            <View>
              <DetailHeader
                item={data.artist}
                kind="artists"
                tracks={data.topTracks}
                origin={`artist:${id}`}
                round
              />
              <Text style={styles.heading}>{t('mobile-artist-top-tracks')}</Text>
            </View>
          }
          ListFooterComponent={
            <View style={styles.footer}>
              <Shelf title={t('mobile-artist-albums')} items={data.albums} kind="album" />
            </View>
          }
          renderItem={({ item, index }) => (
            <TrackRow
              track={item}
              current={item.id === currentId}
              onPress={() =>
                attempt(
                  bridge.play(
                    data.topTracks.map((track) => track.id),
                    index,
                    `artist:${id}`,
                  ),
                )
              }
            />
          )}
        />
      )}
    </Screen>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    marginHorizontal: spacing.lg,
  },
  footer: { marginTop: spacing.xl },
}));
