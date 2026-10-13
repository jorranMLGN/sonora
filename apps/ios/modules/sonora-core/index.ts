import { requireNativeModule } from 'expo';

type Native = {
  version(): string;
  playTone(seconds: number): number;
  init(): Promise<string>;
  search(query: string): Promise<string>;
  play(id: string): Promise<string>;
  toggle(): string;
  nowPlaying(): string;
};

export type Hit = {
  id: string;
  title: string;
  artist: string;
  artwork: string | null;
  duration: number;
  playable: boolean;
};

export type NowPlaying = {
  id: string | null;
  title: string | null;
  artist: string | null;
  artwork: string | null;
  playing: boolean;
  loading: boolean;
  position: number;
  duration: number;
  error: string | null;
};

const native = requireNativeModule<Native>('SonoraCore');

/** Parses an answer from the core, throwing its `error` when it carries one. */
function parse<T>(text: string): T {
  const value = JSON.parse(text);
  // A failure is an object holding `error` and nothing else.
  if (value && !Array.isArray(value) && Object.keys(value).length === 1 && value.error) {
    throw new Error(value.error);
  }
  return value as T;
}

export default {
  version: () => native.version(),
  playTone: (seconds: number) => native.playTone(seconds),
  init: async () => parse<{ ok: boolean }>(await native.init()),
  search: async (query: string) => parse<Hit[]>(await native.search(query)),
  play: async (id: string) => parse<{ ok: boolean }>(await native.play(id)),
  toggle: () => parse<{ playing: boolean }>(native.toggle()),
  nowPlaying: () => parse<NowPlaying>(native.nowPlaying()),
};
