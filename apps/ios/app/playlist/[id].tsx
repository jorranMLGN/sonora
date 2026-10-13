import { useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { FlatList } from 'react-native';

import type { Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { DetailHeader, Loading, useCurrentId } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
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
    <Screen back miniPlayer>
      {!data ? (
        <Loading />
      ) : (
        <FlatList
          data={shown}
          keyExtractor={(track, index) => `${index}:${track.id}`}
          refreshing={loading}
          onRefresh={reload}
          onEndReachedThreshold={0.5}
          onEndReached={more}
          ListHeaderComponent={
            <DetailHeader
              item={data.playlist}
              kind="playlists"
              tracks={shown}
              origin={`playlist:${id}`}
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
      )}
    </Screen>
  );
}
