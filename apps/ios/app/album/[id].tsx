import { useLocalSearchParams } from 'expo-router';
import { FlatList } from 'react-native';

import type { Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { DetailHeader, Loading, useCurrentId } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { TrackRow } from '../../src/components/TrackRow';
import { useVisible } from '../../src/settings';
import { attempt } from '../../src/store';
import { useLoad } from '../../src/useLoad';

/** The tracks of a page still loading, the same array each time so a memo holds. */
const NONE: Track[] = [];

export default function AlbumPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, loading, reload } = useLoad(() => bridge.album(id), [id]);
  const currentId = useCurrentId();
  const tracks = useVisible(data?.tracks ?? NONE);
  return (
    <Screen back miniPlayer>
      {!data ? (
        <Loading />
      ) : (
        <FlatList
          data={tracks}
          keyExtractor={(track, index) => `${index}:${track.id}`}
          refreshing={loading}
          onRefresh={reload}
          ListHeaderComponent={
            <DetailHeader
              item={data.album}
              kind="albums"
              tracks={tracks}
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
                    tracks.map((track) => track.id),
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
