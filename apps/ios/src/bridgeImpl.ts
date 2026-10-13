import native, { type SonoraCoreModule } from '../modules/sonora-core';
import type {
  AlbumDetail,
  ArtistDetail,
  Bridge,
  Page,
  PlaylistDetail,
  Section,
  Track,
  Unsubscribe,
} from './bridge';
import { t } from './i18n';
import { mockBridge } from './mockBridge';

/** A record standing in for an album, playlist or artist whose page the core cannot load yet. */
function placeholder(id: string): Track {
  return {
    id,
    title: '',
    subtitle: '',
    artwork: null,
    durationMs: 0,
    explicit: false,
    provider: id.split(':')[0] ?? '',
    playable: false,
  };
}

const emptyPage = (): Page => ({
  tracks: [],
  albums: [],
  artists: [],
  playlists: [],
  hasMore: false,
});

/**
 * The bridge over the Rust core. Calls the core does not have yet answer empty, so every screen
 * still renders.
 */
function nativeBridge(core: SonoraCoreModule): Bridge {
  return {
    // The core keeps its files under Application Support; Swift picks the folder.
    init: () => core.start(),
    providers: () => core.providers(),
    // TODO(milestone 3): sign_in / answer / sign_out on the core.
    signIn: () => Promise.reject(new Error(t('mobile-sign-in-unavailable'))),
    answer: () => Promise.resolve(),
    signOut: () => Promise.resolve(),
    // TODO(milestone 4): home, library, album, playlist, artist and set_saved on the core.
    home: () => Promise.resolve<Section[]>([]),
    search: (query) => core.search(query),
    library: () => Promise.resolve(emptyPage()),
    album: (id) => Promise.resolve<AlbumDetail>({ album: placeholder(id), tracks: [] }),
    playlist: (id) =>
      Promise.resolve<PlaylistDetail>({ playlist: placeholder(id), tracks: [] }),
    artist: (id) =>
      Promise.resolve<ArtistDetail>({ artist: placeholder(id), topTracks: [], albums: [] }),
    play: (ids, index, origin) => core.play(ids, index, origin),
    toggle: () => core.toggle(),
    next: () => core.next(),
    previous: () => core.previous(),
    seek: (positionMs) => core.seek(positionMs),
    setShuffle: (on) => core.setShuffle(on),
    cycleRepeat: () => core.cycleRepeat(),
    enqueueNext: (ids) => core.enqueueNext(ids),
    enqueueLast: (ids) => core.enqueueLast(ids),
    remove: (index) => core.remove(index),
    setSaved: () => Promise.resolve(),
    on: (event, handler): Unsubscribe => {
      const subscription = core.addListener(event, handler as never);
      return () => subscription.remove();
    },
  };
}

/** The bridge screens use: the native Rust core when the build carries it, the mock otherwise. */
export const bridge: Bridge = native === null ? mockBridge : nativeBridge(native);

/** True when the screens run on the mock instead of the Rust core. */
export const usingMock = native === null;
