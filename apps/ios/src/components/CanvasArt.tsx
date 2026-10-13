import { useEffect, useRef, useState } from 'react';
import { Animated, Easing, StyleSheet, View } from 'react-native';

import { CanvasView } from '../../modules/sonora-core';
import { largestArtwork } from '../artwork';
import type { Track } from '../bridge';
import type { ArtworkShape } from '../settings';
import { useTheme } from '../styles';
import { useCanvas } from '../useCanvas';
import { Artwork } from './Artwork';

/** How long the Canvas takes to fade in over the artwork, and back out of it. */
const FADE_MS = 600;

/**
 * The full player's artwork at its largest, with the track's Canvas faded in over it once the
 * video can draw. A track change fades the old Canvas out before the next one mounts, and a
 * Canvas that cannot play fades back to the artwork. The video runs only while `playing`, and is
 * cut to the same `shape` as the artwork.
 */
export function CanvasArt({
  track,
  size,
  playing,
  shape,
}: {
  track: Track | null;
  size: number;
  playing: boolean;
  shape?: ArtworkShape;
}) {
  const { radii } = useTheme();
  const corner =
    shape === 'circle' ? size / 2 : shape === 'rounded' ? radii.large : radii.small;
  const original = track?.artwork ?? null;
  const large = largestArtwork(original);
  // A large url its host refused, so the artwork falls back to the url the core gave.
  const [broken, setBroken] = useState<string | null>(null);
  const artwork = large !== null && large !== broken ? large : original;

  const url = useCanvas(track);
  // The Canvas mounted now, which lags `url` while the old one fades out.
  const [shown, setShown] = useState<string | null>(url);
  const fade = useRef(new Animated.Value(0)).current;
  const wanted = useRef(url);
  wanted.current = url;
  const ready = useRef<string | null>(null);

  const to = (value: number) =>
    Animated.timing(fade, {
      toValue: value,
      duration: FADE_MS,
      easing: Easing.inOut(Easing.quad),
      useNativeDriver: true,
    });

  useEffect(() => {
    if (url === shown) {
      if (shown === null || ready.current !== shown) return;
      const back = to(1);
      back.start();
      return () => back.stop();
    }
    if (shown === null) {
      setShown(url);
      return;
    }
    const out = to(0);
    out.start(({ finished }) => {
      if (!finished) return;
      ready.current = null;
      setShown(url);
    });
    return () => out.stop();
    // `to` only reads `fade`, which lives as long as the component.
  }, [url, shown]);

  return (
    <View style={{ width: size, height: size }}>
      <Artwork
        uri={artwork}
        size={size}
        shape={shape}
        onError={() => large !== null && large !== original && setBroken(large)}
      />
      {CanvasView !== null && shown !== null && (
        <Animated.View
          style={[StyleSheet.absoluteFill, styles.clip, { borderRadius: corner, opacity: fade }]}
        >
          <CanvasView
            key={shown}
            url={shown}
            playing={playing}
            style={StyleSheet.absoluteFill}
            onReady={() => {
              ready.current = shown;
              if (wanted.current === shown) to(1).start();
            }}
            onFailed={() => {
              ready.current = null;
              if (wanted.current === shown) to(0).start();
            }}
          />
        </Animated.View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  clip: { overflow: 'hidden', pointerEvents: 'none' },
});
