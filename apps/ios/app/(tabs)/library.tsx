import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { FlatList, Pressable, ScrollView, StyleSheet, Text } from 'react-native';

import type { LibraryPart, Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { EntityRow } from '../../src/components/EntityRow';
import { Screen } from '../../src/components/Screen';
import type { Kind } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { attempt, markSaved, useStore } from '../../src/store';
import { colors, radii, sizes, spacing, typography } from '../../src/theme';

const PARTS: LibraryPart[] = ['tracks', 'albums', 'artists', 'playlists'];

const KINDS: Record<Exclude<LibraryPart, 'tracks'>, Kind> = {
  albums: 'album',
  artists: 'artist',
  playlists: 'playlist',
};

export default function Library() {
  const router = useRouter();
  const providers = useStore((s) => s.providers);
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  // A guest session has no library, so only accounts get a chip.
  const usable = providers.filter((p) => p.session.kind === 'signedIn');
  const [chosen, setChosen] = useState<string | null>(null);
  const slug = chosen ?? usable[0]?.slug ?? null;
  const [part, setPart] = useState<LibraryPart>('tracks');
  const [items, setItems] = useState<Track[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const page = useRef(0);
  const generation = useRef(0);

  /** Loads the next page; page 0 starts the part over and replaces what is drawn. */
  const fetchPage = useCallback(() => {
    if (slug === null) return;
    const mine = generation.current;
    const first = page.current === 0;
    setLoading(true);
    bridge
      .library(slug, part, page.current)
      .then((result) => {
        if (mine !== generation.current) return;
        markSaved(part, result.items.map((item) => item.id));
        setItems((known) => (first ? result.items : [...known, ...result.items]));
        setHasMore(result.hasMore);
        page.current += 1;
      })
      .catch((error: Error) => useStore.setState({ error: error.message }))
      .finally(() => mine === generation.current && setLoading(false));
  }, [slug, part]);

  /** Draws the part's last snapshot at once, then reads the first page from the network. */
  const restart = useCallback(
    (cached: boolean) => {
      generation.current += 1;
      page.current = 0;
      setHasMore(false);
      if (slug !== null && cached) {
        const mine = generation.current;
        setItems([]);
        bridge
          .cachedLibrary(slug, part)
          .then((snapshot) => {
            if (snapshot !== null && mine === generation.current && page.current === 0) {
              setItems(snapshot.items);
            }
          })
          .catch(() => undefined);
      }
      fetchPage();
    },
    [slug, part, fetchPage],
  );

  useEffect(() => restart(true), [restart]);

  return (
    <Screen
      title={t('mobile-tab-library')}
      right={
        <Pressable
          hitSlop={spacing.md}
          onPress={() => router.push('/accounts')}
          accessibilityLabel={t('mobile-accounts')}
        >
          <Ionicons name="person-circle-outline" size={sizes.iconLarge} color={colors.text} />
        </Pressable>
      }
    >
      <ScrollView horizontal showsHorizontalScrollIndicator={false} style={styles.chips} contentContainerStyle={styles.chipsContent}>
        {usable.map((p) => (
          <Chip key={p.slug} label={p.name} on={p.slug === slug} onPress={() => setChosen(p.slug)} />
        ))}
      </ScrollView>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} style={styles.chips} contentContainerStyle={styles.chipsContent}>
        {PARTS.map((name) => (
          <Chip
            key={name}
            label={t(`mobile-library-${name}`)}
            on={name === part}
            onPress={() => setPart(name)}
          />
        ))}
      </ScrollView>
      {slug === null ? (
        <Text style={styles.empty}>{t('mobile-library-sign-in')}</Text>
      ) : (
        <FlatList
          data={items}
          keyExtractor={(item, index) => `${index}:${item.id}`}
          refreshing={loading && page.current === 0 && items.length > 0}
          onRefresh={() => restart(false)}
          onEndReachedThreshold={0.5}
          onEndReached={() => hasMore && !loading && fetchPage()}
          ListEmptyComponent={
            !loading ? <Text style={styles.empty}>{t('mobile-library-empty')}</Text> : null
          }
          renderItem={({ item, index }) =>
            part === 'tracks' ? (
              <TrackRow
                track={item}
                current={item.id === currentId}
                onPress={() =>
                  attempt(
                    bridge.play(
                      items.map((track) => track.id),
                      index,
                      `library:${slug}`,
                    ),
                  )
                }
              />
            ) : (
              <EntityRow item={item} kind={KINDS[part]} />
            )
          }
        />
      )}
    </Screen>
  );
}

function Chip({ label, on, onPress }: { label: string; on: boolean; onPress: () => void }) {
  return (
    <Pressable style={[styles.chip, on && styles.chipOn]} onPress={onPress}>
      <Text style={[styles.chipText, on && styles.chipTextOn]}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  chips: { flexGrow: 0, marginBottom: spacing.sm },
  chipsContent: { paddingHorizontal: spacing.lg, gap: spacing.sm },
  chip: {
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
  },
  chipOn: { backgroundColor: colors.accent },
  chipText: { color: colors.text, fontSize: typography.body },
  chipTextOn: { color: colors.onAccent, fontWeight: '600' },
  empty: { color: colors.textDim, textAlign: 'center', marginTop: spacing.xxl },
});
