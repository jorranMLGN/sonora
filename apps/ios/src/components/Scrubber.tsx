import { useRef, useState } from 'react';
import { PanResponder, View } from 'react-native';

import { makeStyles } from '../styles';
import { sizes } from '../theme';

/** A draggable progress bar; reports the position once, when the finger lifts. */
export function Scrubber({
  value,
  max,
  onSeek,
}: {
  value: number;
  max: number;
  onSeek: (ms: number) => void;
}) {
  const styles = useStyles();
  const [width, setWidth] = useState(1);
  const [drag, setDrag] = useState<number | null>(null);
  const live = useRef({ width: 1, max: 1, onSeek });
  live.current = { width, max, onSeek };

  const at = (x: number) =>
    Math.max(0, Math.min(1, x / live.current.width)) * live.current.max;

  const responder = useRef(
    PanResponder.create({
      onStartShouldSetPanResponder: () => true,
      onMoveShouldSetPanResponder: () => true,
      onPanResponderGrant: (event) => setDrag(at(event.nativeEvent.locationX)),
      onPanResponderMove: (event) => setDrag(at(event.nativeEvent.locationX)),
      onPanResponderRelease: (event) => {
        live.current.onSeek(at(event.nativeEvent.locationX));
        setDrag(null);
      },
      onPanResponderTerminate: () => setDrag(null),
    }),
  ).current;

  const shown = drag ?? value;
  const fraction = max > 0 ? Math.min(1, shown / max) : 0;

  return (
    <View
      style={styles.hit}
      onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
      {...responder.panHandlers}
    >
      <View style={styles.track} pointerEvents="none">
        <View style={[styles.fill, { width: `${fraction * 100}%` }]} />
      </View>
      <View
        pointerEvents="none"
        style={[styles.thumb, { left: fraction * width - sizes.scrubberThumb / 2 }]}
      />
    </View>
  );
}

const useStyles = makeStyles(({ colors, radii }) => ({
  hit: { height: sizes.scrubberHit, justifyContent: 'center' },
  track: {
    height: sizes.scrubberTrack,
    borderRadius: radii.pill,
    backgroundColor: colors.surfaceRaised,
    overflow: 'hidden',
  },
  fill: { height: '100%', backgroundColor: colors.accent },
  thumb: {
    position: 'absolute',
    width: sizes.scrubberThumb,
    height: sizes.scrubberThumb,
    borderRadius: radii.pill,
    backgroundColor: colors.text,
  },
}));
