import { useLocalSearchParams } from 'expo-router';
import { FlatList } from 'react-native';

import { bridge } from '../../src/bridgeImpl';
import { DetailHeader, Loading, useCurrentId } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { TrackRow } from '../../src/components/TrackRow';
import { attempt } from '../../src/store';
import { useLoad } from '../../src/useLoad';

export default function PlaylistPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data } = useLoad(() => bridge.playlist(id), [id]);
  const currentId = useCurrentId();
  return (
    <Screen back miniPlayer>
      {!data ? (
        <Loading />
      ) : (
        <FlatList
          data={data.tracks}
          keyExtractor={(track) => track.id}
          ListHeaderComponent={
            <DetailHeader item={data.playlist} tracks={data.tracks} origin={`playlist:${id}`} />
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
                    `playlist:${id}`,
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
