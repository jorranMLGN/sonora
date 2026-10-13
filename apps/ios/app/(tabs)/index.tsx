import { FlatList, Text, View } from 'react-native';

import type { Section } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { Loading } from '../../src/components/Detail';
import { Screen } from '../../src/components/Screen';
import { SettingsButton } from '../../src/components/SettingsButton';
import { Shelf } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { attempt, useStore } from '../../src/store';
import { makeStyles } from '../../src/styles';
import { spacing } from '../../src/theme';
import { useLoad } from '../../src/useLoad';

const TRACKS_SHOWN = 5;

/** A shelf's heading: the provider's own title, or the app's name for the two it leaves blank. */
function heading(section: Section): string {
  switch (section.kind) {
    case 'listenAgain':
      return t('mobile-home-listen-again');
    case 'quickPicks':
      return t('mobile-home-quick-picks');
    default:
      return section.title;
  }
}

export default function Home() {
  // Every provider whose session is up feeds Home, so a session coming up, going, or changing
  // from guest to an account (or to another account) loads it again.
  const live = useStore((s) =>
    s.providers
      .filter((p) => p.session.kind === 'signedIn' || p.session.kind === 'guest')
      .map((p) => `${p.slug}:${p.session.kind}:${p.account ?? ''}`)
      .join(','),
  );
  const { data, loading, reload } = useLoad(() => bridge.home(), [live]);
  const currentId = useStore((s) => s.player?.track?.id ?? null);
  const styles = useStyles();
  return (
    <Screen title={t('mobile-tab-home')} right={<SettingsButton />}>
      {loading && !data && <Loading />}
      <FlatList
        data={data ?? []}
        keyExtractor={(section) => section.id}
        refreshing={loading && data !== null}
        onRefresh={reload}
        ListEmptyComponent={
          !loading ? <Text style={styles.empty}>{t('mobile-home-empty')}</Text> : null
        }
        renderItem={({ item: section }) => {
          const title = heading(section);
          return (
            <View style={styles.section}>
              <Text style={styles.provider}>{section.provider}</Text>
              {section.tracks.length > 0 && (
                <>
                  <Text style={styles.heading}>{title}</Text>
                  {section.tracks.slice(0, TRACKS_SHOWN).map((track, index) => (
                    <TrackRow
                      key={`${index}:${track.id}`}
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
              <Shelf title={title} items={section.albums} kind="album" />
              <Shelf title={title} items={section.artists} kind="artist" />
              <Shelf title={title} items={section.playlists} kind="playlist" />
            </View>
          );
        }}
      />
    </Screen>
  );
}

const useStyles = makeStyles(({ colors, typography }) => ({
  section: { marginTop: spacing.md },
  provider: {
    color: colors.textDim,
    fontSize: typography.caption,
    marginHorizontal: spacing.lg,
    textTransform: 'uppercase',
  },
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    marginHorizontal: spacing.lg,
    marginTop: spacing.xs,
    marginBottom: spacing.md,
  },
  empty: { color: colors.textDim, textAlign: 'center', marginTop: spacing.xxl },
}));
