import { useLocalSearchParams } from 'expo-router';
import { Text, View } from 'react-native';
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
import { Shelf } from '../../src/components/Shelf';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { useVisible } from '../../src/settings';
import { attempt } from '../../src/store';
import { makeStyles } from '../../src/styles';
import { spacing } from '../../src/theme';
import { useLoad } from '../../src/useLoad';

/** The tracks of a page still loading, the same array each time so a memo holds. */
const NONE: Track[] = [];

export default function ArtistPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, loading, reload } = useLoad(() => bridge.artist(id), [id]);
  const currentId = useCurrentId();
  const topTracks = useVisible(data?.topTracks ?? NONE);
  const styles = useStyles();
  return (
    <Page
      back
      title={data?.artist.title}
      revealAt={DETAIL_REVEAL}
      right={<SaveButton id={id} kind="artists" />}
    >
      {(frame) =>
        !data ? (
          <Loading top={frame.top} />
        ) : (
          <Animated.FlatList
            {...framed(frame)}
            data={topTracks}
            keyExtractor={(track, index) => `${index}:${track.id}`}
            refreshing={loading}
            onRefresh={reload}
            ListHeaderComponent={
              <View>
                <DetailHeader
                  item={data.artist}
                  kind="artist"
                  tracks={topTracks}
                  origin={`artist:${id}`}
                  scrollY={frame.scrollY}
                  top={frame.top}
                />
                {topTracks.length > 0 && (
                  <Text style={styles.heading}>{t('mobile-artist-top-tracks')}</Text>
                )}
              </View>
            }
            ListFooterComponent={
              <View style={styles.footer}>
                <Shelf title={t('mobile-artist-albums')} items={data.albums} kind="album" />
              </View>
            }
            renderItem={({ item, index }) => (
              <TrackRow
                track={item}
                current={item.id === currentId}
                onPress={() =>
                  attempt(
                    bridge.play(
                      topTracks.map((track) => track.id),
                      index,
                      `artist:${id}`,
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

const useStyles = makeStyles(({ colors, typography }) => ({
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    letterSpacing: -0.4,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.xs,
  },
  footer: { marginTop: spacing.xl },
}));
