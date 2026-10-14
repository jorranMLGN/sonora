import { Ionicons } from '@expo/vector-icons';
import { useMemo } from 'react';
import { Text, View } from 'react-native';
import Animated from 'react-native-reanimated';

import type { Section } from '../../src/bridge';
import { bridge } from '../../src/bridgeImpl';
import { Page, framed } from '../../src/components/Page';
import { SettingsButton } from '../../src/components/SettingsButton';
import { Shelf } from '../../src/components/Shelf';
import { SkeletonShelves } from '../../src/components/Skeleton';
import { TrackRow } from '../../src/components/TrackRow';
import { t } from '../../src/i18n';
import { useSettings, visible } from '../../src/settings';
import { attempt, useStore } from '../../src/store';
import { makeStyles, useTheme } from '../../src/styles';
import { brand, sizes, spacing } from '../../src/theme';
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
  const theme = useTheme();
  const providers = useStore((s) => s.providers);
  const hidden = useSettings((s) => s.homeHidden);
  const hideExplicit = useSettings((s) => s.hideExplicit);
  // A section names its provider by display name; the setting keeps slugs.
  const sections = useMemo(
    () =>
      (data ?? [])
        .filter((section) => {
          const provider = providers.find(
            (p) => p.name === section.provider || p.slug === section.provider,
          );
          return !hidden.includes(provider?.slug ?? section.provider);
        })
        .map((section) => ({ ...section, tracks: visible(section.tracks, hideExplicit) }))
        .filter(
          (section) =>
            section.tracks.length +
              section.albums.length +
              section.artists.length +
              section.playlists.length >
            0,
        ),
    [data, providers, hidden, hideExplicit],
  );
  // A section names its provider by display name; its dot wants the slug's colour.
  const slugOf = (name: string) =>
    providers.find((p) => p.name === name || p.slug === name)?.slug ?? name;
  return (
    <Page tabs large title={t('mobile-tab-home')} right={<SettingsButton />}>
      {(frame) => (
        <Animated.FlatList
          {...framed(frame)}
          data={sections}
          keyExtractor={(section) => section.id}
          refreshing={loading && data !== null}
          onRefresh={reload}
          ListEmptyComponent={
            loading ? (
              <SkeletonShelves />
            ) : (
              <View style={styles.empty}>
                <Ionicons
                  name="musical-notes-outline"
                  size={sizes.iconLarge}
                  color={theme.colors.textDim}
                />
                <Text style={styles.emptyText}>{t('mobile-home-empty')}</Text>
              </View>
            )
          }
          renderItem={({ item: section }) => {
            const title = heading(section);
            return (
              <View style={styles.section}>
                <View style={styles.source}>
                  <View
                    style={[
                      styles.dot,
                      { backgroundColor: brand(theme, slugOf(section.provider)) },
                    ]}
                  />
                  <Text style={styles.provider}>{section.provider}</Text>
                </View>
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
      )}
    </Page>
  );
}

const useStyles = makeStyles(({ colors, radii, typography }) => ({
  section: { marginTop: spacing.lg },
  source: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs + spacing.xs / 2,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.xs,
  },
  dot: {
    width: sizes.dot - spacing.xs / 2,
    height: sizes.dot - spacing.xs / 2,
    borderRadius: radii.pill,
  },
  provider: {
    color: colors.textDim,
    fontSize: typography.caption,
    fontWeight: '600',
    letterSpacing: 0.6,
    textTransform: 'uppercase',
  },
  heading: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    letterSpacing: -0.4,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.sm,
  },
  empty: { alignItems: 'center', gap: spacing.md, marginTop: spacing.xxl * 2 },
  emptyText: { color: colors.textDim, fontSize: typography.body, textAlign: 'center' },
}));
