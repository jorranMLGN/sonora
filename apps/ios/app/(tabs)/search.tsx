import { Ionicons } from '@expo/vector-icons';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Text, TextInput, View } from 'react-native';
import Animated, { FadeIn } from 'react-native-reanimated';

import type { SearchResults, Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { Chip, ChipRow } from '../../src/components/Chip';
import { EntityRow } from '../../src/components/EntityRow';
import { Page, framed } from '../../src/components/Page';
import { Press } from '../../src/components/Press';
import { SettingsButton } from '../../src/components/SettingsButton';
import type { Kind } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { durations } from '../../src/motion';
import { preference, setPreference } from '../../src/preferences';
import { useSettings, visible } from '../../src/settings';
import { attempt, reason, useStore } from '../../src/store';
import { makeStyles, useTheme } from '../../src/styles';
import { sizes, spacing } from '../../src/theme';

/** How long typing must pause before the search runs by itself. */
const DEBOUNCE_MS = 450;
/** How many recent searches are kept, and the preference they are kept under. */
const RECENT_KEPT = 8;
const RECENT_KEY = 'search.recent';
/** The search field and the gap under it. */
const ACCESSORY_HEIGHT = sizes.field + spacing.sm;

/** Which kind of result the chips narrow the list to. */
type Filter = 'all' | 'tracks' | 'albums' | 'artists' | 'playlists';
const FILTERS: Filter[] = ['all', 'tracks', 'albums', 'artists', 'playlists'];

type Row =
  | { type: 'header'; key: string; title: string }
  | { type: 'track'; key: string; track: Track; index: number }
  | { type: 'entity'; key: string; item: Track; kind: Kind };

/** Flattens the result lists `filter` keeps into one virtualised list with headers. */
function rows(results: SearchResults, filter: Filter): Row[] {
  const out: Row[] = [];
  const titled = filter === 'all';
  const add = (part: Exclude<Filter, 'all' | 'tracks'>, kind: Kind) => {
    const list = results[part];
    if (list.length === 0 || (filter !== 'all' && filter !== part)) return;
    if (titled) out.push({ type: 'header', key: `h-${kind}`, title: t(`mobile-search-${part}`) });
    list.forEach((item) => out.push({ type: 'entity', key: `${kind}-${item.id}`, item, kind }));
  };
  if (results.tracks.length > 0 && (filter === 'all' || filter === 'tracks')) {
    if (titled) out.push({ type: 'header', key: 'h-tracks', title: t('mobile-search-tracks') });
    results.tracks.forEach((track, index) =>
      out.push({ type: 'track', key: `track-${track.id}`, track, index }),
    );
  }
  add('albums', 'album');
  add('artists', 'artist');
  add('playlists', 'playlist');
  return out;
}

/** The searches kept from earlier runs, newest first. */
function readRecent(): string[] {
  const raw = preference(RECENT_KEY);
  return raw === null ? [] : raw.split('\n').filter((entry) => entry !== '');
}

function writeRecent(recent: string[]): void {
  setPreference(RECENT_KEY, recent.length > 0 ? recent.join('\n') : null);
}

export default function Search() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResults | null>(null);
  const [searched, setSearched] = useState('');
  const [searching, setSearching] = useState(false);
  const [filter, setFilter] = useState<Filter>('all');
  const [recent, setRecent] = useState<string[]>(readRecent);
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  const hideExplicit = useSettings((s) => s.hideExplicit);
  const theme = useTheme();
  const styles = useStyles();
  const latest = useRef(0);
  // The term searched last, so the field settling on it does not search it again.
  const asked = useRef('');

  /** Keeps `term` at the top of the recent searches. */
  const remember = (term: string) => {
    if (term === '') return;
    const next = [term, ...recent.filter((entry) => entry !== term)].slice(0, RECENT_KEPT);
    setRecent(next);
    writeRecent(next);
  };

  /** Runs the search for `text`; an answer to an older search than the last is dropped. */
  const run = (text: string, keep: boolean) => {
    const term = text.trim();
    if (term === '') return;
    if (keep) remember(term);
    if (term === asked.current) return;
    asked.current = term;
    const mine = ++latest.current;
    setSearching(true);
    bridge
      .search(term)
      .then((found) => {
        if (mine !== latest.current) return;
        setResults(found);
        setSearched(term);
      })
      .catch((error: Error) => {
        if (mine !== latest.current) return;
        // A search that failed may be asked again.
        asked.current = '';
        useStore.setState({ error: reason(error) });
      })
      .finally(() => mine === latest.current && setSearching(false));
  };

  // Typing searches by itself once the fingers rest; clearing the field clears the results.
  useEffect(() => {
    const term = query.trim();
    if (term === '') {
      latest.current += 1;
      asked.current = '';
      setResults(null);
      setSearched('');
      setSearching(false);
      return;
    }
    if (term === asked.current) return;
    const timer = setTimeout(() => run(term, false), DEBOUNCE_MS);
    return () => clearTimeout(timer);
    // `run` reads the latest state each time; only the query starts it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const tracks = useMemo(
    () => visible(results?.tracks ?? [], hideExplicit),
    [results, hideExplicit],
  );
  const data = useMemo(
    () => (results ? rows({ ...results, tracks }, filter) : []),
    [results, tracks, filter],
  );

  const accessory = (
    <View style={styles.fieldRow}>
      <View style={styles.field}>
        <Ionicons name="search" size={sizes.iconSmall} color={theme.colors.textDim} />
        <TextInput
          style={styles.input}
          placeholder={t('mobile-search-placeholder')}
          placeholderTextColor={theme.colors.textDim}
          value={query}
          onChangeText={setQuery}
          onSubmitEditing={() => run(query, true)}
          returnKeyType="search"
          autoCorrect={false}
          autoCapitalize="none"
          clearButtonMode="while-editing"
        />
        {searching && <ActivityIndicator size="small" color={theme.colors.textDim} />}
      </View>
    </View>
  );

  const header = results ? (
    <View style={styles.filters}>
      <ChipRow>
        {FILTERS.map((name) => (
          <Chip
            key={name}
            label={t(name === 'all' ? 'mobile-search-all' : `mobile-search-${name}`)}
            on={name === filter}
            onPress={() => setFilter(name)}
          />
        ))}
      </ChipRow>
    </View>
  ) : null;

  const idle =
    recent.length > 0 ? (
      <Animated.View entering={FadeIn.duration(durations.fade)}>
        <View style={styles.recentHead}>
          <Text style={styles.heading}>{t('mobile-search-recent')}</Text>
          <Press
            hitSlop={spacing.sm}
            onPress={() => {
              setRecent([]);
              writeRecent([]);
            }}
          >
            <Text style={styles.clear}>{t('mobile-search-clear')}</Text>
          </Press>
        </View>
        {recent.map((entry) => (
          <Press
            key={entry}
            highlight
            style={styles.recent}
            onPress={() => {
              setQuery(entry);
              run(entry, true);
            }}
          >
            <Ionicons
              name="time-outline"
              size={sizes.iconSmall + spacing.xs}
              color={theme.colors.textDim}
            />
            <Text numberOfLines={1} style={styles.recentText}>
              {entry}
            </Text>
            <Ionicons
              name="arrow-up-outline"
              size={sizes.iconSmall}
              color={theme.colors.textDim}
              style={styles.arrow}
            />
          </Press>
        ))}
      </Animated.View>
    ) : (
      <View style={styles.empty}>
        <Ionicons name="search" size={sizes.iconLarge} color={theme.colors.textDim} />
        <Text style={styles.emptyText}>{t('mobile-search-start')}</Text>
      </View>
    );

  return (
    <Page
      tabs
      large
      title={t('mobile-tab-search')}
      right={<SettingsButton />}
      accessory={accessory}
      accessoryHeight={ACCESSORY_HEIGHT}
    >
      {(frame) => (
        <Animated.FlatList
          {...framed(frame)}
          data={data}
          keyExtractor={(row) => row.key}
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode="on-drag"
          ListHeaderComponent={header}
          ListEmptyComponent={
            results === null ? (
              idle
            ) : searching ? null : (
              <View style={styles.empty}>
                <Ionicons name="sad-outline" size={sizes.iconLarge} color={theme.colors.textDim} />
                <Text style={styles.emptyText}>{t('mobile-search-empty')}</Text>
              </View>
            )
          }
          renderItem={({ item: row }) => {
            switch (row.type) {
              case 'header':
                return <Text style={[styles.heading, styles.section]}>{row.title}</Text>;
              case 'track':
                return (
                  <TrackRow
                    track={row.track}
                    current={row.track.id === currentId}
                    onPress={() => {
                      remember(searched);
                      attempt(
                        bridge.play(
                          tracks.map((track) => track.id),
                          row.index,
                          `search:${searched}`,
                        ),
                      );
                    }}
                  />
                );
              case 'entity':
                return <EntityRow item={row.item} kind={row.kind} />;
            }
          }}
        />
      )}
    </Page>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  // The arrow that says a recent search fills the field, pointing up into it.
  arrow: { marginLeft: 'auto', transform: [{ rotate: '-45deg' }] },
  fieldRow: { paddingHorizontal: spacing.lg },
  field: {
    height: sizes.field,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
    paddingHorizontal: spacing.md,
    borderRadius: radii.medium,
    backgroundColor: colors.surfaceRaised,
  },
  input: { flex: 1, height: '100%', color: colors.text, fontSize: typography.body },
  filters: { paddingTop: spacing.xs, paddingBottom: spacing.xs },
  heading: {
    color: colors.text,
    fontSize: typography.title,
    fontWeight: '700',
    letterSpacing: -0.3,
  },
  section: { marginHorizontal: spacing.lg, marginTop: spacing.lg, marginBottom: spacing.xs },
  recentHead: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.lg,
    marginTop: spacing.md,
    marginBottom: spacing.xs,
  },
  clear: { color: colors.accent, fontSize: typography.label, fontWeight: '600' },
  recent: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  recentText: { flexShrink: 1, color: colors.text, fontSize: typography.body },
  empty: {
    alignItems: 'center',
    gap: spacing.md,
    marginTop: spacing.xxl * 2,
    paddingHorizontal: spacing.xl,
  },
  emptyText: { color: colors.textDim, fontSize: typography.body, textAlign: 'center' },
}));
