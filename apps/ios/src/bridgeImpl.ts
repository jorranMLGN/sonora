import native, { type SonoraCoreModule } from '../modules/sonora-core';
import type { Bridge, Unsubscribe } from './bridge';
import { mockBridge } from './mockBridge';

/** The bridge over the Rust core. */
function nativeBridge(core: SonoraCoreModule): Bridge {
  return {
    // The core keeps its files under Application Support; Swift picks the folder.
    init: () => core.start(),
    providers: () => core.providers(),
    signIn: (slug, option) => core.signIn(slug, option),
    answer: (slug, input) => core.answer(slug, input),
    cancelSignIn: (slug) => core.cancelSignIn(slug),
    signOut: (slug) => core.signOut(slug),
    home: () => core.home(),
    search: (query) => core.search(query),
    library: (slug, part, page) => core.library(slug, part, page),
    cachedLibrary: (slug, part) => core.cachedLibrary(slug, part),
    savedIds: () => core.savedIds(),
    album: (id) => core.album(id),
    playlist: (id) => core.playlist(id),
    playlistMore: (id, continuation) => core.playlistMore(id, continuation),
    artist: (id) => core.artist(id),
    canvas: (id) => core.canvas(id),
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
    setSaved: (id, kind, saved) => core.setSaved(id, kind, saved),
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
