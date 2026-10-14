import { NativeModule, requireNativeView, requireOptionalNativeModule } from 'expo';
import type { ComponentType } from 'react';
import type { ViewProps } from 'react-native';

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

/** A library part, also the kind a saved toggle is about. */
export type Part = 'tracks' | 'albums' | 'artists' | 'playlists';

/** One page of a library part; `total` is the part's length when the provider says it. */
export type LibraryPage = { items: Item[]; total: number; hasMore: boolean };

/** One shelf on Home, from one provider. `title` is empty for the two the app names. */
export type HomeSection = {
  id: string;
  title: string;
  provider: string;
  kind: 'listenAgain' | 'quickPicks' | 'shelf';
  tracks: Item[];
  albums: Item[];
  artists: Item[];
  playlists: Item[];
};

export type AlbumDetail = { album: Item; tracks: Item[] };
export type PlaylistDetail = { playlist: Item; tracks: Item[]; continuation: string | null };
export type PlaylistMore = { tracks: Item[]; continuation: string | null };
export type ArtistDetail = { artist: Item; topTracks: Item[]; albums: Item[] };
export type SavedIds = { tracks: string[]; albums: string[]; artists: string[]; playlists: string[] };

type Events = {
  onState(state: PlayerState): void;
  onPosition(event: { positionMs: number }): void;
  onQueue(queue: QueueSnapshot): void;
  onSession(event: { slug: string; state: SessionState }): void;
  onPrompt(event: { slug: string; prompt: Prompt }): void;
  onError(event: { message: string }): void;
  /** A warning or error from the core's log; for the log, never for the listener. */
  onLog(event: { level: string; target: string; message: string }): void;
};

export declare class SonoraCoreModule extends NativeModule<Events> {
  /** Starts the core, which answers at once with the queue, the player and the sessions. */
  start(): Promise<void>;
  /** A switch the app keeps for itself in the user defaults; false until it is set. */
  flag(key: string): boolean;
  setFlag(key: string, on: boolean): void;
  /** A setting the app keeps for itself in the user defaults, as a string; null until it is set. */
  preference(key: string): string | null;
  /** Stores a setting; null removes it, so it reads as unset again. */
  setPreference(key: string, value: string | null): void;
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
  /** Every signed-in provider's home, woven a shelf at a time. */
  home(): Promise<HomeSection[]>;
  /** Page `page` of a library part; page 0 reads it afresh. */
  library(slug: string, part: Part, page: number): Promise<LibraryPage>;
  /** The first page of a part as last listed, or null before the first listing. */
  cachedLibrary(slug: string, part: Part): Promise<LibraryPage | null>;
  /** Every id in the signed-in providers' libraries, by kind. */
  savedIds(): Promise<SavedIds>;
  setSaved(id: string, kind: Part, saved: boolean): Promise<void>;
  album(id: string): Promise<AlbumDetail>;
  playlist(id: string): Promise<PlaylistDetail>;
  playlistMore(id: string, continuation: string): Promise<PlaylistMore>;
  artist(id: string): Promise<ArtistDetail>;
  /** The url of a track's Canvas, the looping MP4 Spotify shows while it plays, or null. */
  canvas(id: string): Promise<string | null>;
  playerState(): Promise<PlayerState>;
  queue(): Promise<QueueSnapshot>;
}

/** The native module, or null where the build does not carry it (Expo Go, the web). */
const core = requireOptionalNativeModule<SonoraCoreModule>('SonoraCore');
export default core;

/**
 * A track's Canvas: `url` muted, looping and filling the view. `onReady` fires once the first
 * frame can be drawn, `onFailed` when the video cannot play. `playing` defaults to true.
 */
export type CanvasViewProps = ViewProps & {
  url: string;
  playing?: boolean;
  onReady?: () => void;
  onFailed?: () => void;
};

/** The native Canvas view, or null where the build does not carry the module. */
export const CanvasView: ComponentType<CanvasViewProps> | null =
  core === null ? null : requireNativeView<CanvasViewProps>('SonoraCore');
