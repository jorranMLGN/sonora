/**
 * The JS face of the Rust core (`crates/mobile`), mirroring the spec's "Bridge surface". The
 * native module and the mock both implement `Bridge`; screens never import either directly,
 * they import `bridge` from `./bridgeImpl`.
 */

/** A track. Its id carries its provider, as `soundcloud:123`. */
export type Track = {
  id: string;
  title: string;
  subtitle: string;
  /** The album of a track, when the source knows it. */
  album?: string;
  artwork: string | null;
  durationMs: number;
  explicit: boolean;
  provider: string;
  playable: boolean;
};

/** An album, flat like a track (`subtitle` is the artist, `durationMs` is 0 when unknown). */
export type Album = Track;
/** An artist (`subtitle` may be a follower count text). */
export type Artist = Track;
/** A playlist (`subtitle` is the owner). */
export type Playlist = Track;

/** A titled shelf on Home. */
export type Section = {
  id: string;
  title: string;
  tracks: Track[];
  albums: Album[];
  artists: Artist[];
  playlists: Playlist[];
};

export type SearchResults = {
  tracks: Track[];
  albums: Album[];
  artists: Artist[];
  playlists: Playlist[];
};

/** One page of a library part; only the list for the requested part is filled. */
export type Page = SearchResults & { hasMore: boolean };

export type LibraryPart = 'tracks' | 'albums' | 'artists' | 'playlists';

export type AlbumDetail = { album: Album; tracks: Track[] };
export type PlaylistDetail = { playlist: Playlist; tracks: Track[] };
export type ArtistDetail = { artist: Artist; topTracks: Track[]; albums: Album[] };

export type Status = 'idle' | 'loading' | 'playing' | 'paused';
export type Repeat = 'off' | 'all' | 'one';

export type PlayerState = {
  track: Track | null;
  status: Status;
  positionMs: number;
  durationMs: number;
  shuffle: boolean;
  repeat: Repeat;
};

export type QueueSnapshot = {
  items: Track[];
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
  | { kind: 'failed'; message: string };

export type ProviderInfo = {
  slug: string;
  name: string;
  signedIn: boolean;
  account: string | null;
  session: SessionState;
};

export type PromptChoice = { id: string; name: string; detail: string | null };

export type Prompt =
  | { kind: 'url'; url: string }
  | { kind: 'code'; code: string; url: string }
  | { kind: 'secret' }
  | { kind: 'accounts'; choices: PromptChoice[] };

/** Event payloads, keyed by the native event names. */
export type BridgeEvents = {
  onState: PlayerState;
  onPosition: { positionMs: number };
  onQueue: QueueSnapshot;
  onSession: { slug: string; state: SessionState };
  onPrompt: { slug: string; prompt: Prompt };
  onError: { message: string };
};

export type Unsubscribe = () => void;

export interface Bridge {
  /** Starts the core; it answers at once with the queue, the player and the sessions. */
  init(dataDir: string): Promise<void>;
  providers(): Promise<ProviderInfo[]>;
  signIn(slug: string, option: string | null): Promise<void>;
  answer(slug: string, input: string): Promise<void>;
  signOut(slug: string): Promise<void>;
  home(): Promise<Section[]>;
  search(query: string): Promise<SearchResults>;
  library(slug: string, part: LibraryPart, page: number): Promise<Page>;
  album(id: string): Promise<AlbumDetail>;
  playlist(id: string): Promise<PlaylistDetail>;
  artist(id: string): Promise<ArtistDetail>;
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
  /** Takes the entry at `index` out of the queue; the current track cannot be removed. */
  remove(index: number): Promise<void>;
  setSaved(id: string, saved: boolean): Promise<void>;
  /** Subscribes to a core event; returns the function that removes the subscription. */
  on<K extends keyof BridgeEvents>(
    event: K,
    handler: (payload: BridgeEvents[K]) => void,
  ): Unsubscribe;
}
