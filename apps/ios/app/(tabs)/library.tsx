import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ActionSheetIOS, Text, TextInput, View, useWindowDimensions } from 'react-native';
import Animated, { FadeIn } from 'react-native-reanimated';

import type { LibraryPart, Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { CHIP_ROW_HEIGHT, Chip, ChipRow } from '../../src/components/Chip';
import { EntityRow } from '../../src/components/EntityRow';
import { Page, framed } from '../../src/components/Page';
import { Press } from '../../src/components/Press';
import { SettingsButton } from '../../src/components/SettingsButton';
import { type Kind, Tile } from '../../src/components/Shelf';
import { SkeletonRows } from '../../src/components/Skeleton';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { durations, haptic, pressScale } from '../../src/motion';
import { CHOICES, type LibrarySort, useSettings, visible } from '../../src/settings';
import { attempt, markSaved, reason, useStore } from '../../src/store';
import { makeStyles, useTheme } from '../../src/styles';
import { brand, sizes, spacing } from '../../src/theme';

const PARTS: LibraryPart[] = ['tracks', 'albums', 'artists', 'playlists'];

const KINDS: Record<Exclude<LibraryPart, 'tracks'>, Kind> = {
  albums: 'album',
  artists: 'artist',
  playlists: 'playlist',
};

/** The two chip rows under the title, and the gap under them. */
const ACCESSORY_HEIGHT = CHIP_ROW_HEIGHT * 2 + spacing.sm;

/** The narrowest a grid cover gets before the grid drops a column. */
const GRID_MIN = sizes.tileArtwork;

/** `items` in the order `sort` asks for; `recent` keeps the service's own order. */
function sorted(items: Track[], sort: LibrarySort): Track[] {
  if (sort === 'recent') return items;
  const key = sort === 'title' ? (item: Track) => item.title : (item: Track) => item.subtitle;
  return [...items].sort((a, b) =>
    key(a).localeCompare(key(b), undefined, { sensitivity: 'base' }),
  );
}

/** Whether `item` matches what the listener typed into the filter, by title or subtitle. */
function matches(item: Track, query: string): boolean {
  const needle = query.toLocaleLowerCase();
  return (
    item.title.toLocaleLowerCase().includes(needle) ||
    item.subtitle.toLocaleLowerCase().includes(needle)
  );
}

/** Opens the sort menu and hands the chosen order to `choose`. */
function chooseSort(current: LibrarySort, choose: (sort: LibrarySort) => void) {
  const sorts = CHOICES.librarySort;
  ActionSheetIOS.showActionSheetWithOptions(
    {
      title: t('mobile-library-sort'),
      options: [...sorts.map((sort) => t(`mobile-library-sort-${sort}`)), t('mobile-cancel')],
      cancelButtonIndex: sorts.length,
    },
    (index) => {
      const sort = sorts[index];
      if (sort === undefined || sort === current) return;
      haptic.select();
      choose(sort);
    },
  );
}

export default function Library() {
  const router = useRouter();
  const providers = useStore((s) => s.providers);
  const theme = useTheme();
  const styles = useStyles();
  const { width } = useWindowDimensions();
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  // A guest session has no library, so only accounts get a chip.
  const usable = providers.filter((p) => p.session.kind === 'signedIn');
  const [chosen, setChosen] = useState<string | null>(null);
  // A chosen provider that signed out falls back to the first one still signed in.
  const slug = usable.find((p) => p.slug === chosen)?.slug ?? usable[0]?.slug ?? null;
  // Signing in to another account of the same provider lists the library again.
  const account = usable.find((p) => p.slug === slug)?.account ?? null;
  const opening = useSettings((s) => s.libraryTab);
  const [part, setPart] = useState<LibraryPart>(opening);
  // Choosing another part to open on in Settings switches to it at once.
  useEffect(() => setPart(opening), [opening]);
  const hideExplicit = useSettings((s) => s.hideExplicit);
  const layout = useSettings((s) => s.libraryLayout);
  const sort = useSettings((s) => s.librarySort);
  const setSetting = useSettings((s) => s.set);
  const [query, setQuery] = useState('');
  const [items, setItems] = useState<Track[]>([]);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const page = useRef(0);
  const generation = useRef(0);
  // Whether a page is on its way, so two callers never ask for the same one.
  const busy = useRef(false);

  /**
   * Loads the next page; page 0 starts the part over and replaces what is drawn. A page that
   * fails ends the listing there, so nothing asks again and again; a pull to refresh retries.
   */
  const fetchPage = useCallback(() => {
    if (slug === null || busy.current) return;
    const mine = generation.current;
    const first = page.current === 0;
    busy.current = true;
    setLoading(true);
    bridge
      .library(slug, part, page.current)
      .then((result) => {
        if (mine !== generation.current) return;
        markSaved(
          part,
          result.items.map((item) => item.id),
        );
        setItems((known) => (first ? result.items : [...known, ...result.items]));
        setTotal(result.total);
        setHasMore(result.hasMore);
        page.current += 1;
      })
      .catch((error: Error) => {
        if (mine !== generation.current) return;
        setHasMore(false);
        useStore.setState({ error: reason(error) });
      })
      .finally(() => {
        if (mine !== generation.current) return;
        busy.current = false;
        setLoading(false);
      });
  }, [slug, part, account]);

  /** Draws the part's last snapshot at once, then reads the first page from the network. */
  const restart = useCallback(
    (cached: boolean) => {
      generation.current += 1;
      page.current = 0;
      // The page an older generation still waits on will be dropped, so it no longer blocks.
      busy.current = false;
      setHasMore(false);
      if (slug !== null && cached) {
        const mine = generation.current;
        setItems([]);
        setTotal(0);
        bridge
          .cachedLibrary(slug, part)
          .then((snapshot) => {
            if (snapshot !== null && mine === generation.current && page.current === 0) {
              setItems(snapshot.items);
              setTotal(snapshot.total);
            }
          })
          .catch(() => undefined);
      }
      fetchPage();
    },
    [slug, part, fetchPage],
  );

  useEffect(() => restart(true), [restart]);
  // A new part or service starts with an empty filter.
  useEffect(() => setQuery(''), [slug, part]);

  // Filtering and sorting look at the whole part, so they read the rest of it in.
  const needsAll = query.trim() !== '' || sort !== 'recent';
  useEffect(() => {
    if (needsAll && hasMore && !loading) fetchPage();
  }, [needsAll, hasMore, loading, fetchPage]);

  const shown = useMemo(() => {
    const kept = part === 'tracks' ? visible(items, hideExplicit) : items;
    const needle = query.trim();
    const found = needle === '' ? kept : kept.filter((item) => matches(item, needle));
    return sorted(found, sort);
  }, [items, part, hideExplicit, query, sort]);

  const grid = part !== 'tracks' && layout === 'grid';
  const columns = Math.max(
    2,
    Math.floor((width - spacing.lg * 2 + spacing.md) / (GRID_MIN + spacing.md)),
  );
  const tile = (width - spacing.lg * 2 - spacing.md * (columns - 1)) / columns;
  const count = query.trim() !== '' ? shown.length : Math.max(total, items.length);

  const accessory = (
    <View>
      <ChipRow>
        {usable.map((p) => (
          <Chip
            key={p.slug}
            label={p.name}
            dot={brand(theme, p.slug)}
            on={p.slug === slug}
            onPress={() => setChosen(p.slug)}
          />
        ))}
      </ChipRow>
      <ChipRow>
        {PARTS.map((name) => (
          <Chip
            key={name}
            label={t(`mobile-library-${name}`)}
            on={name === part}
            onPress={() => setPart(name)}
          />
        ))}
      </ChipRow>
    </View>
  );

  const toolbar = (
    <View style={styles.toolbar}>
      <View style={styles.tools}>
        <Text style={styles.count}>
          {items.length > 0 ? t(`mobile-library-count-${part}`, { count }) : ''}
        </Text>
        <Press
          scale={pressScale.chip}
          hitSlop={spacing.sm}
          style={styles.tool}
          onPress={() => chooseSort(sort, (next) => setSetting('librarySort', next))}
          accessibilityLabel={t('mobile-library-sort')}
        >
          <Ionicons name="swap-vertical" size={sizes.iconSmall} color={theme.colors.accent} />
          <Text style={styles.toolText}>{t(`mobile-library-sort-${sort}`)}</Text>
        </Press>
        {part !== 'tracks' && (
          <Press
            scale={pressScale.button}
            hitSlop={spacing.sm}
            style={styles.tool}
            onPress={() => {
              haptic.select();
              setSetting('libraryLayout', grid ? 'list' : 'grid');
            }}
            accessibilityLabel={t(grid ? 'mobile-library-list' : 'mobile-library-grid')}
          >
            <Ionicons
              name={grid ? 'list' : 'grid-outline'}
              size={sizes.icon - spacing.xs}
              color={theme.colors.text}
            />
          </Press>
        )}
      </View>
      <View style={styles.field}>
        <Ionicons name="search" size={sizes.iconSmall} color={theme.colors.textDim} />
        <TextInput
          style={styles.input}
          value={query}
          onChangeText={setQuery}
          placeholder={t('mobile-library-filter')}
          placeholderTextColor={theme.colors.textDim}
          autoCorrect={false}
          autoCapitalize="none"
          clearButtonMode="while-editing"
          returnKeyType="done"
        />
      </View>
    </View>
  );

  const empty = loading ? (
    <SkeletonRows round={part === 'artists'} />
  ) : (
    <View style={styles.empty}>
      <Ionicons
        name={query.trim() !== '' ? 'search' : 'library-outline'}
        size={sizes.iconLarge}
        color={theme.colors.textDim}
      />
      <Text style={styles.emptyText}>
        {query.trim() !== ''
          ? t('mobile-library-no-match', { query: query.trim() })
          : t('mobile-library-empty')}
      </Text>
    </View>
  );

  return (
    <Page
      tabs
      large
      title={t('mobile-tab-library')}
      right={<SettingsButton />}
      accessory={slug !== null ? accessory : undefined}
      accessoryHeight={slug !== null ? ACCESSORY_HEIGHT : 0}
      resetKey={`${slug}:${part}:${grid ? 'grid' : 'list'}`}
    >
      {(frame) =>
        slug === null ? (
          <View style={[styles.signIn, { paddingTop: frame.top }]}>
            <Ionicons
              name="person-circle-outline"
              size={sizes.iconLarge * 2}
              color={theme.colors.textDim}
            />
            <Text style={styles.emptyText}>{t('mobile-library-sign-in')}</Text>
            <Press
              scale={pressScale.button}
              style={styles.button}
              onPress={() => router.push('/accounts')}
            >
              <Text style={styles.buttonText}>{t('mobile-accounts')}</Text>
            </Press>
          </View>
        ) : (
          <Animated.View
            key={`${slug}:${part}:${grid ? 'grid' : 'list'}`}
            entering={FadeIn.duration(durations.swap)}
            style={styles.fill}
          >
            <Animated.FlatList
              {...framed(frame)}
              data={shown}
              key={grid ? `grid-${columns}` : 'list'}
              numColumns={grid ? columns : 1}
              columnWrapperStyle={grid ? styles.gridRow : undefined}
              keyExtractor={(item, index) => `${index}:${item.id}`}
              keyboardDismissMode="on-drag"
              keyboardShouldPersistTaps="handled"
              refreshing={loading && page.current === 0 && items.length > 0}
              onRefresh={() => restart(false)}
              onEndReachedThreshold={0.6}
              onEndReached={() => hasMore && !loading && fetchPage()}
              ListHeaderComponent={toolbar}
              ListEmptyComponent={empty}
              renderItem={({ item, index }) => {
                if (part === 'tracks') {
                  return (
                    <TrackRow
                      track={item}
                      current={item.id === currentId}
                      onPress={() =>
                        attempt(
                          bridge.play(
                            shown.map((track) => track.id),
                            index,
                            `library:${slug}`,
                          ),
                        )
                      }
                    />
                  );
                }
                return grid ? (
                  <Tile item={item} kind={KINDS[part]} size={tile} />
                ) : (
                  <EntityRow item={item} kind={KINDS[part]} />
                );
              }}
            />
          </Animated.View>
        )
      }
    </Page>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  fill: { flex: 1 },
  toolbar: {
    paddingHorizontal: spacing.lg,
    paddingTop: spacing.sm,
    gap: spacing.md,
    marginBottom: spacing.sm,
  },
  tools: { flexDirection: 'row', alignItems: 'center', gap: spacing.lg },
  count: { flex: 1, color: colors.textDim, fontSize: typography.label, fontWeight: '500' },
  tool: { flexDirection: 'row', alignItems: 'center', gap: spacing.xs },
  toolText: { color: colors.text, fontSize: typography.label, fontWeight: '600' },
  field: {
    height: sizes.field,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
    paddingHorizontal: spacing.md,
    borderRadius: radii.medium,
    backgroundColor: colors.surface,
  },
  input: { flex: 1, height: '100%', color: colors.text, fontSize: typography.body },
  gridRow: { paddingHorizontal: spacing.lg, gap: spacing.md, marginBottom: spacing.lg },
  empty: {
    alignItems: 'center',
    gap: spacing.md,
    marginTop: spacing.xxl * 2,
    paddingHorizontal: spacing.xl,
  },
  emptyText: { color: colors.textDim, fontSize: typography.body, textAlign: 'center' },
  signIn: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: spacing.lg,
    paddingHorizontal: spacing.xl,
  },
  button: {
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.md,
    borderRadius: radii.pill,
    backgroundColor: colors.accent,
  },
  buttonText: { color: colors.onAccent, fontSize: typography.body, fontWeight: '700' },
}));
