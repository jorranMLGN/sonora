import { FlatList, StyleSheet, Text } from 'react-native';

import { bridge } from '../../src/bridgeImpl';
import { Loading } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { Shelf } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { attempt, useStore } from '../../src/store';
import { colors, spacing, typography } from '../../src/theme';
import { useLoad } from '../../src/useLoad';

const TRACKS_SHOWN = 5;

export default function Home() {
  const { data, loading, reload } = useLoad(() => bridge.home(), []);
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  return (
    <Screen title={t('mobile-tab-home')}>
      {loading && !data && <Loading />}
      <FlatList
        data={data ?? []}
        keyExtractor={(section) => section.id}
        refreshing={loading && data !== null}
        onRefresh={reload}
        ListEmptyComponent={
          !loading ? <Text style={styles.empty}>{t('mobile-home-empty')}</Text> : null
        }
        renderItem={({ item: section }) => (
          <>
            {section.tracks.length > 0 && (
              <>
                <Text style={styles.heading}>{section.title}</Text>
                {section.tracks.slice(0, TRACKS_SHOWN).map((track, index) => (
                  <TrackRow
                    key={track.id}
                    track={track}
                    current={track.id === currentId}
                    onPress={() =>
                      attempt(
                        bridge.play(
                          section.tracks.map((entry) => entry.id),
                          index,
                          `home:${section.id}`,
                        ),
                      )
                    }
                  />
                ))}
              </>
            )}
            <Shelf title={section.title} items={section.albums} kind="album" />
            <Shelf title={section.title} items={section.artists} kind="artist" />
            <Shelf title={section.title} items={section.playlists} kind="playlist" />
          </>
        )}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    margin: spacing.lg,
  },
  empty: { color: colors.textDim, textAlign: 'center', marginTop: spacing.xxl },
});
