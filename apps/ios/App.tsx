import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  FlatList,
  Image,
  Pressable,
  SafeAreaView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import SonoraCore, {
  Item,
  PlayerState,
  QueueSnapshot,
  SessionState,
} from './modules/sonora-core';

type Tab = 'search' | 'queue';

function clock(ms: number) {
  const whole = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

function describe(state: SessionState) {
  switch (state.kind) {
    case 'signedIn':
      return state.account;
    case 'failed':
      return 'failed';
    default:
      return state.kind;
  }
}

/** Runs a core call and shows its failure instead of throwing. */
function attempt(call: Promise<unknown>, onError: (message: string) => void) {
  call.catch((error: Error) => onError(error.message));
}

function Row({
  item,
  current,
  onPress,
  onLongPress,
}: {
  item: Item;
  current: boolean;
  onPress: () => void;
  onLongPress?: () => void;
}) {
  return (
    <Pressable
      style={[styles.row, current && styles.current]}
      onPress={onPress}
      onLongPress={onLongPress}
      disabled={!item.playable}
    >
      {item.artwork ? (
        <Image source={{ uri: item.artwork }} style={styles.art} />
      ) : (
        <View style={[styles.art, styles.blank]} />
      )}
      <View style={styles.meta}>
        <Text numberOfLines={1} style={[styles.title, !item.playable && styles.dim]}>
          {item.title}
        </Text>
        <Text numberOfLines={1} style={styles.dim}>
          {item.subtitle} · {clock(item.durationMs)} · {item.provider}
        </Text>
      </View>
    </Pressable>
  );
}

export default function App() {
  const [problem, setProblem] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Record<string, SessionState>>({});
  const [query, setQuery] = useState('');
  const [tracks, setTracks] = useState<Item[]>([]);
  const [searching, setSearching] = useState(false);
  const [player, setPlayer] = useState<PlayerState | null>(null);
  const [position, setPosition] = useState(0);
  const [queue, setQueue] = useState<QueueSnapshot | null>(null);
  const [tab, setTab] = useState<Tab>('search');

  useEffect(() => {
    const subscriptions = [
      SonoraCore.addListener('onState', (state) => {
        setPlayer(state);
        setPosition(state.positionMs);
      }),
      SonoraCore.addListener('onPosition', ({ positionMs }) => setPosition(positionMs)),
      SonoraCore.addListener('onQueue', setQueue),
      SonoraCore.addListener('onSession', ({ slug, state }) =>
        setSessions((known) => ({ ...known, [slug]: state })),
      ),
      SonoraCore.addListener('onError', ({ message }) => setProblem(message)),
    ];
    attempt(SonoraCore.start(), setProblem);
    return () => subscriptions.forEach((subscription) => subscription.remove());
  }, []);

  const search = () => {
    if (!query.trim()) return;
    setSearching(true);
    setProblem(null);
    SonoraCore.search(query.trim())
      .then((results) => setTracks(results.tracks))
      .catch((error: Error) => setProblem(error.message))
      .finally(() => setSearching(false));
  };

  const ready = Object.values(sessions).some(
    (state) => state.kind === 'guest' || state.kind === 'signedIn',
  );
  const playing = player?.status === 'playing' || player?.status === 'loading';

  return (
    <SafeAreaView style={styles.screen}>
      <Text style={styles.sessions}>
        {Object.entries(sessions)
          .map(([slug, state]) => `${slug}: ${describe(state)}`)
          .join(' · ')}
      </Text>
      <View style={styles.tabs}>
        {(['search', 'queue'] as Tab[]).map((name) => (
          <Pressable key={name} onPress={() => setTab(name)} style={styles.tab}>
            <Text style={[styles.tabText, tab === name && styles.tabActive]}>
              {name === 'search' ? 'Search' : `Queue (${queue?.items.length ?? 0})`}
            </Text>
          </Pressable>
        ))}
      </View>
      {problem && (
        <Pressable onPress={() => setProblem(null)}>
          <Text style={styles.problem}>{problem}</Text>
        </Pressable>
      )}
      {tab === 'search' ? (
        <>
          <TextInput
            style={styles.field}
            placeholder={ready ? 'Search' : 'Connecting…'}
            editable={ready}
            value={query}
            onChangeText={setQuery}
            onSubmitEditing={search}
            returnKeyType="search"
            autoCorrect={false}
          />
          {searching && <ActivityIndicator style={styles.spinner} />}
          <FlatList
            style={styles.list}
            data={tracks}
            keyExtractor={(item) => item.id}
            keyboardShouldPersistTaps="handled"
            renderItem={({ item, index }) => (
              <Row
                item={item}
                current={player?.track?.id === item.id}
                onPress={() =>
                  attempt(
                    SonoraCore.play(
                      tracks.map((track) => track.id),
                      index,
                      `search:${query.trim()}`,
                    ),
                    setProblem,
                  )
                }
                onLongPress={() => attempt(SonoraCore.enqueueLast([item.id]), setProblem)}
              />
            )}
          />
        </>
      ) : (
        <FlatList
          style={styles.list}
          data={queue?.items ?? []}
          keyExtractor={(item, index) => `${index}:${item.id}`}
          ListHeaderComponent={
            <Text style={styles.hint}>Long-press a track to remove it.</Text>
          }
          renderItem={({ item, index }) => (
            <Row
              item={item}
              current={queue?.current === index}
              onPress={() => {}}
              onLongPress={() => attempt(SonoraCore.remove(index), setProblem)}
            />
          )}
        />
      )}
      {player?.track && (
        <View style={styles.bar}>
          <View style={styles.meta}>
            <Text numberOfLines={1} style={styles.title}>
              {player.track.title}
            </Text>
            <Text numberOfLines={1} style={styles.dim}>
              {player.track.subtitle} · {clock(position)} / {clock(player.durationMs)}
            </Text>
          </View>
          <Pressable
            style={styles.small}
            onPress={() => attempt(SonoraCore.setShuffle(!player.shuffle), setProblem)}
          >
            <Text style={[styles.smallText, player.shuffle && styles.on]}>Shuf</Text>
          </Pressable>
          <Pressable
            style={styles.small}
            onPress={() => attempt(SonoraCore.cycleRepeat(), setProblem)}
          >
            <Text style={[styles.smallText, player.repeat !== 'off' && styles.on]}>
              {player.repeat === 'one' ? 'Rep1' : 'Rep'}
            </Text>
          </Pressable>
          <Pressable
            style={styles.small}
            onPress={() => attempt(SonoraCore.previous(), setProblem)}
          >
            <Text style={styles.smallText}>Prev</Text>
          </Pressable>
          {player.status === 'loading' ? (
            <ActivityIndicator style={styles.toggle} />
          ) : (
            <Pressable
              style={styles.toggle}
              onPress={() => attempt(SonoraCore.toggle(), setProblem)}
            >
              <Text style={styles.toggleText}>{playing ? 'Pause' : 'Play'}</Text>
            </Pressable>
          )}
          <Pressable style={styles.small} onPress={() => attempt(SonoraCore.next(), setProblem)}>
            <Text style={styles.smallText}>Next</Text>
          </Pressable>
        </View>
      )}
      <StatusBar style="auto" />
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#fff' },
  sessions: { textAlign: 'center', color: '#888', fontSize: 12, marginTop: 8 },
  tabs: { flexDirection: 'row', justifyContent: 'center', gap: 24, marginTop: 8 },
  tab: { paddingVertical: 6 },
  tabText: { fontSize: 16, color: '#888' },
  tabActive: { color: '#000', fontWeight: '600' },
  field: {
    margin: 12,
    padding: 10,
    borderRadius: 8,
    backgroundColor: '#f0f0f0',
    fontSize: 16,
  },
  problem: { color: '#c0392b', marginHorizontal: 12, marginVertical: 8 },
  hint: { color: '#888', marginHorizontal: 12, marginVertical: 8 },
  spinner: { marginBottom: 8 },
  list: { flex: 1 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 12,
    paddingVertical: 6,
    gap: 10,
  },
  current: { backgroundColor: '#fff3e6' },
  art: { width: 44, height: 44, borderRadius: 4 },
  blank: { backgroundColor: '#ddd' },
  meta: { flex: 1 },
  title: { fontSize: 15, fontWeight: '500' },
  dim: { color: '#888' },
  bar: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 12,
    gap: 8,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: '#ccc',
  },
  small: { paddingHorizontal: 4, paddingVertical: 8 },
  smallText: { color: '#555', fontWeight: '600' },
  on: { color: '#ff5500' },
  toggle: {
    minWidth: 64,
    alignItems: 'center',
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 16,
    backgroundColor: '#ff5500',
  },
  toggleText: { color: '#fff', fontWeight: '600' },
});
