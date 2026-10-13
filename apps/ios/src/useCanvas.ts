import { useEffect, useState } from 'react';
import { AccessibilityInfo } from 'react-native';

import { CanvasView } from '../modules/sonora-core';
import type { Track } from './bridge';
import { bridge } from './bridgeImpl';

/** What each track answered, its Canvas url or null for none, kept for the run. */
const known = new Map<string, string | null>();
/** Lookups under way, so the player opened twice asks once. */
const pending = new Map<string, Promise<string | null>>();

/** Only Spotify has Canvases, and only a build with the native view can play them. */
function wanted(track: Track | null): string | null {
  return track?.provider === 'spotify' && CanvasView !== null ? track.id : null;
}

/**
 * Asks the core for the Canvas of `id` once. A failed lookup counts as no Canvas for now but
 * is not remembered, so the next time the track plays asks again.
 */
function lookup(id: string): Promise<string | null> {
  const asked = pending.get(id);
  if (asked) return asked;
  const ask = bridge
    .canvas(id)
    .then((url) => {
      known.set(id, url);
      return url;
    })
    .catch((error: Error) => {
      console.warn(`canvas: cannot look up ${id}: ${error.message}`);
      return null;
    })
    .finally(() => pending.delete(id));
  pending.set(id, ask);
  return ask;
}

/**
 * The Canvas url of `track`, or null while it is looked up, when it has none, for a provider
 * without Canvases and while the system asks for reduced motion. It never delays playback.
 */
export function useCanvas(track: Track | null): string | null {
  const id = wanted(track);
  const still = useReduceMotion();
  const [url, setUrl] = useState<string | null>(() => (id ? (known.get(id) ?? null) : null));
  useEffect(() => {
    if (id === null) {
      setUrl(null);
      return;
    }
    if (known.has(id)) {
      setUrl(known.get(id) ?? null);
      return;
    }
    setUrl(null);
    let live = true;
    lookup(id).then((found) => live && setUrl(found));
    return () => {
      live = false;
    };
  }, [id]);
  return still ? null : url;
}

/** Whether the system asks for reduced motion, following the setting as it changes. */
function useReduceMotion(): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    let live = true;
    AccessibilityInfo.isReduceMotionEnabled().then((value) => live && setOn(value));
    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', setOn);
    return () => {
      live = false;
      subscription.remove();
    };
  }, []);
  return on;
}
