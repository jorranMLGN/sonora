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

import SonoraCore, { Hit, NowPlaying } from './modules/sonora-core';

function clock(seconds: number) {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

export default function App() {
  const [ready, setReady] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<Hit[]>([]);
  const [searching, setSearching] = useState(false);
  const [now, setNow] = useState<NowPlaying | null>(null);

  const connect = () => {
    setProblem(null);
    SonoraCore.init()
      .then(() => setReady(true))
      .catch((error: Error) => setProblem(error.message));
  };

  useEffect(connect, []);

  useEffect(() => {
    if (!ready) return;
    const timer = setInterval(() => {
      try {
        setNow(SonoraCore.nowPlaying());
      } catch {}
    }, 500);
    return () => clearInterval(timer);
  }, [ready]);

  const search = () => {
    if (!query.trim()) return;
    setSearching(true);
    setProblem(null);
    SonoraCore.search(query.trim())
      .then(setHits)
      .catch((error: Error) => setProblem(error.message))
      .finally(() => setSearching(false));
  };

  const play = (hit: Hit) => {
    SonoraCore.play(hit.id).catch((error: Error) => setProblem(error.message));
  };

  return (
    <SafeAreaView style={styles.screen}>
      <Text style={styles.version}>{SonoraCore.version()} · SoundCloud guest</Text>
      <TextInput
        style={styles.field}
        placeholder={ready ? 'Search SoundCloud' : 'Connecting…'}
        editable={ready}
        value={query}
        onChangeText={setQuery}
        onSubmitEditing={search}
        returnKeyType="search"
        autoCorrect={false}
      />
      {problem && (
        <Pressable onPress={ready ? undefined : connect}>
          <Text style={styles.problem}>
            {problem}
            {ready ? '' : ' (tap to retry)'}
          </Text>
        </Pressable>
      )}
      {searching && <ActivityIndicator style={styles.spinner} />}
      <FlatList
        style={styles.list}
        data={hits}
        keyExtractor={(hit) => hit.id}
        keyboardShouldPersistTaps="handled"
        renderItem={({ item }) => (
          <Pressable
            style={[styles.row, now?.id === item.id && styles.current]}
            onPress={() => play(item)}
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
                {item.artist} · {clock(item.duration)}
              </Text>
            </View>
          </Pressable>
        )}
      />
      {now?.id && (
        <View style={styles.bar}>
          <View style={styles.meta}>
            <Text numberOfLines={1} style={styles.title}>
              {now.title ?? now.id}
            </Text>
            <Text numberOfLines={1} style={styles.dim}>
              {now.error ?? `${now.artist ?? ''} · ${clock(now.position)} / ${clock(now.duration)}`}
            </Text>
          </View>
          {now.loading ? (
            <ActivityIndicator />
          ) : (
            <Pressable
              style={styles.toggle}
              onPress={() => {
                try {
                  SonoraCore.toggle();
                } catch (error) {
                  setProblem((error as Error).message);
                }
              }}
            >
              <Text style={styles.toggleText}>{now.playing ? 'Pause' : 'Play'}</Text>
            </Pressable>
          )}
        </View>
      )}
      <StatusBar style="auto" />
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#fff' },
  version: { textAlign: 'center', color: '#888', fontSize: 12, marginTop: 8 },
  field: {
    margin: 12,
    padding: 10,
    borderRadius: 8,
    backgroundColor: '#f0f0f0',
    fontSize: 16,
  },
  problem: { color: '#c0392b', marginHorizontal: 12, marginBottom: 8 },
  spinner: { marginBottom: 8 },
  list: { flex: 1 },
  row: { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 12, paddingVertical: 6, gap: 10 },
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
    gap: 12,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: '#ccc',
  },
  toggle: { paddingHorizontal: 16, paddingVertical: 8, borderRadius: 16, backgroundColor: '#ff5500' },
  toggleText: { color: '#fff', fontWeight: '600' },
});
