import { useLocalSearchParams } from 'expo-router';
import { FlatList } from 'react-native';

import { bridge } from '../../src/bridgeImpl';
import { DetailHeader, Loading, useCurrentId } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { TrackRow } from '../../src/components/TrackRow';
import { attempt } from '../../src/store';
import { useLoad } from '../../src/useLoad';

export default function AlbumPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, loading, reload } = useLoad(() => bridge.album(id), [id]);
  const currentId = useCurrentId();
  return (
    <Screen back miniPlayer>
      {!data ? (
        <Loading />
      ) : (
        <FlatList
          data={data.tracks}
          keyExtractor={(track, index) => `${index}:${track.id}`}
          refreshing={loading}
          onRefresh={reload}
          ListHeaderComponent={
            <DetailHeader
              item={data.album}
              kind="albums"
              tracks={data.tracks}
              origin={`album:${id}`}
            />
          }
          renderItem={({ item, index }) => (
            <TrackRow
              track={item}
              current={item.id === currentId}
              onPress={() =>
                attempt(
                  bridge.play(
                    data.tracks.map((track) => track.id),
                    index,
                    `album:${id}`,
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
