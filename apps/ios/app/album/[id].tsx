import { useLocalSearchParams } from 'expo-router';
import Animated from 'react-native-reanimated';

import type { Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import {
  DETAIL_REVEAL,
  DetailHeader,
  Loading,
  SaveButton,
  useCurrentId,
} from '../../src/components/Detail';
import { Page, framed } from '../../src/components/Page';
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
    <Page
      back
      title={data?.album.title}
      revealAt={DETAIL_REVEAL}
      right={<SaveButton id={id} kind="albums" />}
    >
      {(frame) =>
        !data ? (
          <Loading top={frame.top} />
        ) : (
          <Animated.FlatList
            {...framed(frame)}
            data={tracks}
            keyExtractor={(track, index) => `${index}:${track.id}`}
            refreshing={loading}
            onRefresh={reload}
            ListHeaderComponent={
              <DetailHeader
                item={data.album}
                kind="album"
                tracks={tracks}
                origin={`album:${id}`}
                scrollY={frame.scrollY}
                top={frame.top}
                count={tracks.length}
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
        )
      }
    </Page>
  );
}
