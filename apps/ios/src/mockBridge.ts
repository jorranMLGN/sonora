import type {
  AlbumDetail,
  ArtistDetail,
  Bridge,
  BridgeEvents,
  LibraryPart,
  Page,
  PlayerState,
  PlaylistDetail,
  ProviderInfo,
  QueueSnapshot,
  Repeat,
  SearchResults,
  Section,
  SessionState,
  Track,
  Unsubscribe,
} from './bridge';

const TICK_MS = 500;
const PAGE_SIZE = 20;
const PAGES = 3;

type Handler = (payload: never) => void;
const handlers = new Map<keyof BridgeEvents, Set<Handler>>();

function emit<K extends keyof BridgeEvents>(event: K, payload: BridgeEvents[K]) {
  handlers.get(event)?.forEach((handler) => (handler as (p: BridgeEvents[K]) => void)(payload));
}

const NAMES = ['Aurora', 'Night Drive', 'Glass Hours', 'Paper Moons', 'Static', 'Low Tide', 'Neon Rain'];
const ARTISTS = ['The Lanterns', 'Mira Vale', 'Hollow Pines', 'Kito', 'Sundial Club'];

function art(seed: string): string {
  return `https://picsum.photos/seed/${encodeURIComponent(seed)}/400`;
}

function item(provider: string, kind: string, n: number, subtitle?: string): Track {
  return {
    id: `${provider}:${kind}${n}`,
    title: `${NAMES[n % NAMES.length]} ${kind === 'track' ? '' : n}`.trim(),
    subtitle: subtitle ?? ARTISTS[n % ARTISTS.length],
    artwork: art(`${provider}${kind}${n}`),
    durationMs: kind === 'track' ? 120_000 + ((n * 37_000) % 150_000) : 0,
    explicit: n % 5 === 0,
    provider,
    playable: true,
  };
}

function many(provider: string, kind: string, from: number, count: number): Track[] {
  return Array.from({ length: count }, (_, i) => item(provider, kind, from + i));
}

const catalog = new Map<string, Track>();
function remember(tracks: Track[]): Track[] {
  tracks.forEach((track) => catalog.set(track.id, track));
  return tracks;
}

const providerList: ProviderInfo[] = [
  { slug: 'spotify', name: 'Spotify', signedIn: false, account: null, session: { kind: 'signedOut' }, options: ['account'] },
  { slug: 'youtube', name: 'YouTube Music', signedIn: false, account: null, session: { kind: 'signedOut' }, options: ['guest', 'web'] },
  { slug: 'soundcloud', name: 'SoundCloud', signedIn: false, account: null, session: { kind: 'guest' }, options: ['guest', 'web', 'paste'] },
];

const saved = new Set<string>();

let player: PlayerState = {
  track: null,
  status: 'idle',
  positionMs: 0,
  durationMs: 0,
  shuffle: false,
  repeat: 'off',
};
let queue: QueueSnapshot = { items: [], current: null, origin: null, shuffle: false, repeat: 'off' };
let timer: ReturnType<typeof setInterval> | null = null;

function publishQueue() {
  queue = { ...queue, shuffle: player.shuffle, repeat: player.repeat };
  emit('onQueue', queue);
}

function publishState() {
  emit('onState', player);
}

function stopTimer() {
  if (timer !== null) clearInterval(timer);
  timer = null;
}

function startTimer() {
  stopTimer();
  timer = setInterval(() => {
    const positionMs = player.positionMs + TICK_MS;
    if (positionMs >= player.durationMs) {
      advance(true);
      return;
    }
    player = { ...player, positionMs };
    emit('onPosition', { positionMs });
  }, TICK_MS);
}

function load(index: number) {
  const track = queue.items[index];
  queue = { ...queue, current: index };
  player = {
    ...player,
    track,
    status: 'playing',
    positionMs: 0,
    durationMs: track.durationMs,
  };
  publishQueue();
  publishState();
  startTimer();
}

function advance(automatic: boolean) {
  const current = queue.current;
  if (current === null) return;
  if (automatic && player.repeat === 'one') return load(current);
  const next = current + 1;
  if (next < queue.items.length) return load(next);
  if (player.repeat === 'all' && queue.items.length > 0) return load(0);
  stopTimer();
  player = { ...player, status: 'paused', positionMs: 0 };
  publishState();
}

function setSession(slug: string, state: SessionState, account: string | null = null) {
  const info = providerList.find((provider) => provider.slug === slug);
  if (info) {
    info.session = state;
    info.signedIn = state.kind === 'signedIn';
    info.account = account;
  }
  emit('onSession', { slug, state });
}

function lookup(ids: string[]): Track[] {
  return ids.map((id) => catalog.get(id)).filter((track): track is Track => track !== undefined);
}

const empty = (): SearchResults => ({ tracks: [], albums: [], artists: [], playlists: [] });

/** A bridge with fake data and timers, for building the UI without the Rust core. */
export const mockBridge: Bridge = {
  async init() {
    providerList.forEach((provider) => emit('onSession', { slug: provider.slug, state: provider.session }));
    publishState();
    publishQueue();
  },
  async providers() {
    return providerList.map((provider) => ({ ...provider }));
  },
  async signIn(slug, option) {
    setSession(slug, { kind: 'connecting' });
    await delay(300);
    if (option === 'guest') {
      setSession(slug, { kind: 'guest' });
      return;
    }
    switch (option) {
      case 'account':
        emit('onPrompt', { slug, prompt: { kind: 'url', url: 'https://accounts.spotify.com/' } });
        setTimeout(() => setSession(slug, { kind: 'signedIn', account: 'mock@spotify' }, 'mock@spotify'), 3000);
        break;
      case 'paste':
        emit('onPrompt', { slug, prompt: { kind: 'secret', web: null } });
        break;
      default:
        emit('onPrompt', {
          slug,
          prompt: {
            kind: 'accounts',
            choices: [
              { id: 'a', name: 'Mock account', detail: 'mock@example.com' },
              { id: 'b', name: 'Other account', detail: null },
            ],
          },
        });
    }
  },
  async answer(slug, input) {
    await delay(300);
    setSession(slug, { kind: 'signedIn', account: input.length > 12 ? 'mock account' : input }, 'mock account');
  },
  async cancelSignIn(slug) {
    const info = providerList.find((provider) => provider.slug === slug);
    setSession(slug, slug === 'soundcloud' ? { kind: 'guest' } : { kind: 'signedOut' }, info?.account ?? null);
  },
  async signOut(slug) {
    setSession(slug, { kind: slug === 'soundcloud' ? 'guest' : 'signedOut' });
  },
  async home() {
    await delay(200);
    const sections: Section[] = [
      { id: 'recent', title: 'Jump back in', tracks: remember(many('soundcloud', 'track', 0, 8)), albums: [], artists: [], playlists: [] },
      { id: 'albums', title: 'New albums', tracks: [], albums: remember(many('spotify', 'album', 0, 8)), artists: [], playlists: [] },
      { id: 'artists', title: 'Artists for you', tracks: [], albums: [], artists: remember(many('youtube', 'artist', 0, 8)), playlists: [] },
      { id: 'lists', title: 'Made for you', tracks: [], albums: [], artists: [], playlists: remember(many('spotify', 'playlist', 0, 8)) },
    ];
    return sections;
  },
  async search(query): Promise<SearchResults> {
    await delay(300);
    if (query.trim().length === 0) return empty();
    return {
      tracks: remember(many('soundcloud', 'track', 10, 12).map((t) => ({ ...t, title: `${query} ${t.title}` }))),
      albums: remember(many('spotify', 'album', 10, 4)),
      artists: remember(many('youtube', 'artist', 10, 4)),
      playlists: remember(many('soundcloud', 'playlist', 10, 4)),
    };
  },
  async library(slug, part: LibraryPart, page): Promise<Page> {
    await delay(200);
    const kind = part.slice(0, -1);
    const list = remember(many(slug, kind, page * PAGE_SIZE, PAGE_SIZE));
    return { ...empty(), [part]: list, hasMore: page + 1 < PAGES };
  },
  async album(id): Promise<AlbumDetail> {
    await delay(200);
    const album = catalog.get(id) ?? item('spotify', 'album', 0);
    return { album, tracks: remember(many(album.provider, 'track', 20, 9)) };
  },
  async playlist(id): Promise<PlaylistDetail> {
    await delay(200);
    const playlist = catalog.get(id) ?? item('spotify', 'playlist', 0);
    return { playlist, tracks: remember(many(playlist.provider, 'track', 30, 14)) };
  },
  async artist(id): Promise<ArtistDetail> {
    await delay(200);
    const artist = catalog.get(id) ?? item('youtube', 'artist', 0);
    return {
      artist,
      topTracks: remember(many(artist.provider, 'track', 40, 6)),
      albums: remember(many(artist.provider, 'album', 40, 5)),
    };
  },
  async play(ids, index, origin) {
    const items = lookup(ids);
    if (items.length === 0) return;
    queue = { ...queue, items, origin };
    load(Math.min(index, items.length - 1));
  },
  async toggle() {
    if (player.track === null) return;
    switch (player.status) {
      case 'playing':
        stopTimer();
        player = { ...player, status: 'paused' };
        break;
      default:
        player = { ...player, status: 'playing' };
        startTimer();
    }
    publishState();
  },
  async next() {
    advance(false);
  },
  async previous() {
    const current = queue.current;
    if (current === null) return;
    if (player.positionMs > 3000 || current === 0) {
      player = { ...player, positionMs: 0 };
      publishState();
    } else {
      load(current - 1);
    }
  },
  async seek(positionMs) {
    player = { ...player, positionMs };
    publishState();
  },
  async setShuffle(on) {
    player = { ...player, shuffle: on };
    publishState();
    publishQueue();
  },
  async cycleRepeat() {
    const order: Repeat[] = ['off', 'all', 'one'];
    player = { ...player, repeat: order[(order.indexOf(player.repeat) + 1) % order.length] };
    publishState();
    publishQueue();
  },
  async enqueueNext(ids) {
    const at = (queue.current ?? -1) + 1;
    const items = [...queue.items];
    items.splice(at, 0, ...lookup(ids));
    queue = { ...queue, items };
    publishQueue();
  },
  async enqueueLast(ids) {
    queue = { ...queue, items: [...queue.items, ...lookup(ids)] };
    publishQueue();
  },
  async remove(index) {
    if (index === queue.current || index < 0 || index >= queue.items.length) return;
    const items = queue.items.filter((_, i) => i !== index);
    const current = queue.current !== null && index < queue.current ? queue.current - 1 : queue.current;
    queue = { ...queue, items, current };
    publishQueue();
  },
  async setSaved(id, on) {
    if (on) saved.add(id);
    else saved.delete(id);
  },
  on<K extends keyof BridgeEvents>(event: K, handler: (payload: BridgeEvents[K]) => void): Unsubscribe {
    const set = handlers.get(event) ?? new Set<Handler>();
    set.add(handler as Handler);
    handlers.set(event, set);
    return () => {
      set.delete(handler as Handler);
    };
  },
};

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
