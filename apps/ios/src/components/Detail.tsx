import { Ionicons } from '@expo/vector-icons';
import { useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import type { Track } from '../bridge';
import { bridge } from '../bridgeImpl';
import { t } from '../i18n';
import { attempt, useStore } from '../store';
import { colors, radii, sizes, spacing, typography } from '../theme';
import { Artwork } from './Artwork';

/** Artwork, title, play / shuffle buttons and a save toggle, shared by the detail pages. */
export function DetailHeader({
  item,
  tracks,
  origin,
  round = false,
}: {
  item: Track;
  tracks: Track[];
  origin: string;
  round?: boolean;
}) {
  const [saved, setSaved] = useState(false);
  const play = (shuffle: boolean) => {
    if (tracks.length === 0) return;
    attempt(
      bridge.setShuffle(shuffle).then(() =>
        bridge.play(
          tracks.map((track) => track.id),
          shuffle ? Math.floor(Math.random() * tracks.length) : 0,
          origin,
        ),
      ),
    );
  };
  return (
    <View style={styles.header}>
      <Artwork uri={item.artwork} size={sizes.headerArtwork} round={round} />
      <Text style={styles.title}>{item.title}</Text>
      <Text style={styles.subtitle}>{item.subtitle}</Text>
      <View style={styles.actions}>
        <Pressable style={styles.primary} onPress={() => play(false)}>
          <Ionicons name="play" size={sizes.icon} color={colors.onAccent} />
          <Text style={styles.primaryText}>{t('mobile-play')}</Text>
        </Pressable>
        <Pressable style={styles.secondary} onPress={() => play(true)}>
          <Ionicons name="shuffle" size={sizes.icon} color={colors.text} />
          <Text style={styles.secondaryText}>{t('mobile-shuffle')}</Text>
        </Pressable>
        <Pressable
          style={styles.secondary}
          onPress={() => {
            attempt(bridge.setSaved(item.id, !saved));
            setSaved(!saved);
          }}
          accessibilityLabel={t(saved ? 'mobile-unsave' : 'mobile-save')}
        >
          <Ionicons
            name={saved ? 'heart' : 'heart-outline'}
            size={sizes.icon}
            color={saved ? colors.accent : colors.text}
          />
        </Pressable>
      </View>
    </View>
  );
}

/** Spinner shown while a detail page loads. */
export function Loading() {
  return <ActivityIndicator color={colors.textDim} style={styles.loading} />;
}

/** Whether a track is the one playing, for highlighting rows. */
export function useCurrentId(): string | null {
  return useStore((s) => s.player?.track?.id ?? null);
}

const styles = StyleSheet.create({
  header: { alignItems: 'center', gap: spacing.sm, padding: spacing.lg },
  title: {
    color: colors.text,
    fontSize: typography.heading,
    fontWeight: '700',
    textAlign: 'center',
    marginTop: spacing.md,
  },
  subtitle: { color: colors.textDim, fontSize: typography.body, textAlign: 'center' },
  actions: { flexDirection: 'row', gap: spacing.sm, marginTop: spacing.md },
  primary: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs,
    backgroundColor: colors.accent,
    borderRadius: radii.pill,
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.md,
  },
  primaryText: { color: colors.onAccent, fontWeight: '600', fontSize: typography.body },
  secondary: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs,
    backgroundColor: colors.surfaceRaised,
    borderRadius: radii.pill,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  secondaryText: { color: colors.text, fontWeight: '600', fontSize: typography.body },
  loading: { marginTop: spacing.xxl },
});
