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

/**
 * A shelf on Home, from one provider. `title` is the provider's own and is empty for the two
 * kinds the app names itself (`listenAgain`, `quickPicks`).
 */
export type Section = {
  id: string;
  title: string;
  /** The provider's display name. */
  provider: string;
  kind: 'listenAgain' | 'quickPicks' | 'shelf';
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

/** One page of a library part; `total` is the part's length when the provider says it. */
export type Page = { items: Track[]; total: number; hasMore: boolean };

/** A library part, and the kind of item a saved toggle is about. */
export type LibraryPart = 'tracks' | 'albums' | 'artists' | 'playlists';

/** Every id in the signed-in providers' libraries, by kind. */
export type SavedIds = Record<LibraryPart, string[]>;

export type AlbumDetail = { album: Album; tracks: Track[] };
/** A playlist page; `continuation`, when set, fetches more tracks through `playlistMore`. */
export type PlaylistDetail = { playlist: Playlist; tracks: Track[]; continuation: string | null };
export type PlaylistMore = { tracks: Track[]; continuation: string | null };
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

/** A cookie sign-in the native side runs in its own browser. */
export type WebSignIn = {
  url: string;
  landing: string;
  domain: string;
  proof: string[];
  agent: string | null;
};

export type PromptChoice = { id: string; name: string; detail: string | null };

export type Prompt =
  | { kind: 'url'; url: string }
  | { kind: 'code'; code: string; url: string }
  | { kind: 'secret'; web: WebSignIn | null }
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
  /**
   * Signs in and settles when that is done. A provider's refusal rejects with an error whose
   * `code` is `ERR_SIGN_IN_<PROBLEM>`; a sign-in called off resolves.
   */
  signIn(slug: string, option: SignInOption): Promise<void>;
  /** Answers the prompt the sign-in of `slug` waits on. */
  answer(slug: string, input: string): Promise<void>;
  /** Calls the sign-in of `slug` off and puts its session back. */
  cancelSignIn(slug: string): Promise<void>;
  /** Drops the account, its tracks in the queue and its snapshots. */
  signOut(slug: string): Promise<void>;
  home(): Promise<Section[]>;
  search(query: string): Promise<SearchResults>;
  /** Page `page` of a library part, from 0; page 0 reads the part afresh. */
  library(slug: string, part: LibraryPart, page: number): Promise<Page>;
  /** The first page of a part as it was last listed, to draw before the network answers. */
  cachedLibrary(slug: string, part: LibraryPart): Promise<Page | null>;
  /** Every id in the signed-in providers' libraries, for the hearts. */
  savedIds(): Promise<SavedIds>;
  album(id: string): Promise<AlbumDetail>;
  playlist(id: string): Promise<PlaylistDetail>;
  playlistMore(id: string, continuation: string): Promise<PlaylistMore>;
  artist(id: string): Promise<ArtistDetail>;
  /**
   * The url of the Canvas of the track `id`, the short looping MP4 Spotify shows while it
   * plays, or null when it has none. Only Spotify tracks have one.
   */
  canvas(id: string): Promise<string | null>;
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
  /** Puts the item `id` of kind `kind` into its provider's library, or takes it out. */
  setSaved(id: string, kind: LibraryPart, saved: boolean): Promise<void>;
  /** Subscribes to a core event; returns the function that removes the subscription. */
  on<K extends keyof BridgeEvents>(
    event: K,
    handler: (payload: BridgeEvents[K]) => void,
  ): Unsubscribe;
}
