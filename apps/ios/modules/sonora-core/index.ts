import { NativeModule, requireOptionalNativeModule } from 'expo';

/** A track, album, playlist or artist. Its id carries its provider, as `soundcloud:123`. */
export type Item = {
  id: string;
  title: string;
  subtitle: string;
  /** The album of a track; empty for anything else, or when unknown. */
  album: string;
  artwork: string | null;
  durationMs: number;
  explicit: boolean;
  provider: string;
  playable: boolean;
};

export type SearchResults = {
  tracks: Item[];
  albums: Item[];
  artists: Item[];
  playlists: Item[];
};

export type Status = 'idle' | 'loading' | 'playing' | 'paused';
export type Repeat = 'off' | 'all' | 'one';

export type PlayerState = {
  track: Item | null;
  status: Status;
  positionMs: number;
  durationMs: number;
  shuffle: boolean;
  repeat: Repeat;
};

export type QueueSnapshot = {
  items: Item[];
  current: number | null;
  origin: string | null;
  shuffle: boolean;
  repeat: Repeat;
};

export type SessionState =
  | { kind: 'signedOut' }
  | { kind: 'connecting' }
  | { kind: 'guest' }
  | { kind: 'signedIn'; account: string }
  | { kind: 'expired' }
  | { kind: 'failed'; message: string; problem: string | null };

/** How to sign in: the provider's account page, as a guest, in the app's browser, or by pasting. */
export type SignInOption = 'account' | 'guest' | 'web' | 'paste';

export type ProviderInfo = {
  slug: string;
  name: string;
  signedIn: boolean;
  account: string | null;
  session: SessionState;
  options: SignInOption[];
};

/** A cookie sign-in Swift runs in its own browser. */
export type WebSignIn = {
  url: string;
  landing: string;
  domain: string;
  proof: string[];
  agent: string | null;
};

export type Prompt =
  | { kind: 'url'; url: string }
  | { kind: 'code'; code: string; url: string }
  | { kind: 'secret'; web: WebSignIn | null }
  | { kind: 'accounts'; choices: { id: string; name: string; detail: string | null }[] };

type Events = {
  onState(state: PlayerState): void;
  onPosition(event: { positionMs: number }): void;
  onQueue(queue: QueueSnapshot): void;
  onSession(event: { slug: string; state: SessionState }): void;
  onPrompt(event: { slug: string; prompt: Prompt }): void;
  onError(event: { message: string }): void;
};

export declare class SonoraCoreModule extends NativeModule<Events> {
  /** Starts the core, which answers at once with the queue, the player and the sessions. */
  start(): Promise<void>;
  providers(): Promise<ProviderInfo[]>;
  /**
   * Signs in and settles when that is done. A provider's refusal rejects with the code
   * `ERR_SIGN_IN_<PROBLEM>`; a sign-in called off resolves.
   */
  signIn(slug: string, option: SignInOption): Promise<void>;
  /** Answers the prompt the sign-in of `slug` waits on. */
  answer(slug: string, input: string): Promise<void>;
  cancelSignIn(slug: string): Promise<void>;
  signOut(slug: string): Promise<void>;
  search(query: string): Promise<SearchResults>;
  /** Replaces the queue with `ids` and plays the one at `index`. */
  play(ids: string[], index: number, origin: string | null): Promise<void>;
  toggle(): Promise<void>;
  next(): Promise<void>;
  previous(): Promise<void>;
  seek(positionMs: number): Promise<void>;
  setShuffle(on: boolean): Promise<void>;
  cycleRepeat(): Promise<void>;
  enqueueNext(ids: string[]): Promise<void>;
  enqueueLast(ids: string[]): Promise<void>;
  /** Takes the entry at `index` of the queue out; the current track cannot be removed. */
  remove(index: number): Promise<void>;
  playerState(): Promise<PlayerState>;
  queue(): Promise<QueueSnapshot>;
}

/** The native module, or null where the build does not carry it (Expo Go, the web). */
export default requireOptionalNativeModule<SonoraCoreModule>('SonoraCore');
