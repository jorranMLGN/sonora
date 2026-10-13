import { useState } from 'react';
import { ActivityIndicator, FlatList, Text, TextInput } from 'react-native';

import type { SearchResults, Track } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { EntityRow } from '../../src/components/EntityRow';
import { Screen } from '../../src/components/Screen';
import { SettingsButton } from '../../src/components/SettingsButton';
import type { Kind } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { attempt, useStore } from '../../src/store';
import { makeStyles, useTheme } from '../../src/styles';
import { spacing } from '../../src/theme';

type Row =
  | { type: 'header'; key: string; title: string }
  | { type: 'track'; key: string; track: Track; index: number }
  | { type: 'entity'; key: string; item: Track; kind: Kind };

/** Flattens the four result lists into one virtualised list with headers. */
function rows(results: SearchResults): Row[] {
  const out: Row[] = [];
  const add = (title: string, list: Track[], kind: Kind) => {
    if (list.length === 0) return;
    out.push({ type: 'header', key: `h-${kind}`, title });
    list.forEach((item) => out.push({ type: 'entity', key: `${kind}-${item.id}`, item, kind }));
  };
  if (results.tracks.length > 0) {
    out.push({ type: 'header', key: 'h-tracks', title: t('mobile-search-tracks') });
    results.tracks.forEach((track, index) =>
      out.push({ type: 'track', key: `track-${track.id}`, track, index }),
    );
  }
  add(t('mobile-search-albums'), results.albums, 'album');
  add(t('mobile-search-artists'), results.artists, 'artist');
  add(t('mobile-search-playlists'), results.playlists, 'playlist');
  return out;
}

export default function Search() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResults | null>(null);
  const [searching, setSearching] = useState(false);
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  const { colors } = useTheme();
  const styles = useStyles();

  const submit = () => {
    const text = query.trim();
    if (!text) return;
    setSearching(true);
    bridge
      .search(text)
      .then(setResults)
      .catch((error: Error) => useStore.setState({ error: error.message }))
      .finally(() => setSearching(false));
  };

  return (
    <Screen title={t('mobile-tab-search')} right={<SettingsButton />}>
      <TextInput
        style={styles.field}
        placeholder={t('mobile-search-placeholder')}
        placeholderTextColor={colors.textDim}
        value={query}
        onChangeText={setQuery}
        onSubmitEditing={submit}
        returnKeyType="search"
        autoCorrect={false}
        autoCapitalize="none"
        clearButtonMode="while-editing"
      />
      {searching && <ActivityIndicator color={colors.textDim} />}
      <FlatList
        data={results ? rows(results) : []}
        keyExtractor={(row) => row.key}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="on-drag"
        ListEmptyComponent={
          results && !searching ? <Text style={styles.empty}>{t('mobile-search-empty')}</Text> : null
        }
        renderItem={({ item: row }) => {
          switch (row.type) {
            case 'header':
              return <Text style={styles.heading}>{row.title}</Text>;
            case 'track':
              return (
                <TrackRow
                  track={row.track}
                  current={row.track.id === currentId}
                  onPress={() =>
                    attempt(
                      bridge.play(
                        (results?.tracks ?? []).map((track) => track.id),
                        row.index,
                        `search:${query.trim()}`,
                      ),
                    )
                  }
                />
              );
            case 'entity':
              return <EntityRow item={row.item} kind={row.kind} />;
          }
        }}
      />
    </Screen>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  field: {
    marginHorizontal: spacing.lg,
    marginBottom: spacing.md,
    padding: spacing.md,
    borderRadius: radii.medium,
    backgroundColor: colors.surfaceRaised,
    color: colors.text,
    fontSize: typography.title,
  },
  heading: {
    color: colors.text,
    fontSize: typography.title,
    fontWeight: '700',
    marginHorizontal: spacing.lg,
    marginTop: spacing.lg,
    marginBottom: spacing.xs,
  },
  empty: { color: colors.textDim, textAlign: 'center', marginTop: spacing.xxl },
}));
