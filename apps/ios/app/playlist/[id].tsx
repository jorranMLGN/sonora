import { useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
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
import { attempt, useStore } from '../../src/store';
import { useLoad } from '../../src/useLoad';

export default function PlaylistPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, loading, reload } = useLoad(() => bridge.playlist(id), [id]);
  const currentId = useCurrentId();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [continuation, setContinuation] = useState<string | null>(null);
  const fetching = useRef(false);
  const shown = useVisible(tracks);

  // A fresh load replaces whatever the continuations had added.
  useEffect(() => {
    setTracks(data?.tracks ?? []);
    setContinuation(data?.continuation ?? null);
  }, [data]);

  const more = () => {
    if (continuation === null || fetching.current) return;
    fetching.current = true;
    bridge
      .playlistMore(id, continuation)
      .then((next) => {
        setTracks((known) => [...known, ...next.tracks]);
        setContinuation(next.continuation);
      })
      .catch((error: Error) => {
        setContinuation(null);
        useStore.setState({ error: error.message });
      })
      .finally(() => (fetching.current = false));
  };

  return (
    <Page
      back
      title={data?.playlist.title}
      revealAt={DETAIL_REVEAL}
      right={<SaveButton id={id} kind="playlists" />}
    >
      {(frame) =>
        !data ? (
          <Loading top={frame.top} />
        ) : (
          <Animated.FlatList
            {...framed(frame)}
            data={shown}
            keyExtractor={(track, index) => `${index}:${track.id}`}
            refreshing={loading}
            onRefresh={reload}
            onEndReachedThreshold={0.5}
            onEndReached={more}
            ListHeaderComponent={
              <DetailHeader
                item={data.playlist}
                kind="playlist"
                tracks={shown}
                origin={`playlist:${id}`}
                scrollY={frame.scrollY}
                top={frame.top}
                // The count is only worth saying once every page of the playlist is in.
                count={continuation === null ? shown.length : undefined}
              />
            }
            renderItem={({ item, index }) => (
              <TrackRow
                track={item}
                current={item.id === currentId}
                onPress={() =>
                  attempt(
                    bridge.play(
                      shown.map((track) => track.id),
                      index,
                      `playlist:${id}`,
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
