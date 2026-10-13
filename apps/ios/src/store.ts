import { create } from 'zustand';

import type {
  LibraryPart,
  PlayerState,
  Prompt,
  ProviderInfo,
  QueueSnapshot,
  SavedIds,
  SessionState,
  SignInOption,
} from './bridge';
import { bridge } from './bridgeImpl';
import { t } from './i18n';

type StoreState = {
  providers: ProviderInfo[];
  sessions: Record<string, SessionState>;
  prompts: Record<string, Prompt | undefined>;
  /** The providers a sign-in started here is still running for. */
  signingIn: Record<string, boolean>;
  player: PlayerState | null;
  positionMs: number;
  queue: QueueSnapshot | null;
  error: string | null;
  /** What is known to be in a library, keyed by `savedKey`. */
  saved: Record<string, boolean>;
  clearError(): void;
  clearPrompt(slug: string): void;
};

/** The one store. Bridge events feed it; screens read it and call bridge functions. */
export const useStore = create<StoreState>((set) => ({
  providers: [],
  sessions: {},
  prompts: {},
  signingIn: {},
  player: null,
  positionMs: 0,
  queue: null,
  error: null,
  saved: {},
  clearError: () => set({ error: null }),
  clearPrompt: (slug) => set((s) => ({ prompts: { ...s.prompts, [slug]: undefined } })),
}));

let started = false;

/** Subscribes the store to the bridge events and starts the core; safe to call twice. */
export function startStore(): void {
  if (started) return;
  started = true;
  const apply = useStore.setState;
  bridge.on('onState', (player) => apply({ player, positionMs: player.positionMs }));
  bridge.on('onPosition', ({ positionMs }) => apply({ positionMs }));
  bridge.on('onQueue', (queue) => apply({ queue }));
  bridge.on('onSession', ({ slug, state }) => {
    if (state.kind === 'signedIn') loadSaved();
    if (state.kind === 'signedOut') {
      dropSaved(slug);
      loadSaved();
    }
    apply((s) => ({
      sessions: { ...s.sessions, [slug]: state },
      providers: s.providers.map((p) =>
        p.slug === slug
          ? {
              ...p,
              session: state,
              signedIn: state.kind === 'signedIn',
              account: state.kind === 'signedIn' ? state.account : null,
            }
          : p,
      ),
      prompts: state.kind === 'connecting' ? s.prompts : { ...s.prompts, [slug]: undefined },
    }));
  });
  bridge.on('onPrompt', ({ slug, prompt }) =>
    apply((s) => ({ prompts: { ...s.prompts, [slug]: prompt } })),
  );
  bridge.on('onError', ({ message }) => apply({ error: message }));
  bridge
    .init('')
    .then(() => bridge.providers())
    .then((providers) => apply({ providers }))
    .catch((error: Error) => apply({ error: error.message }));
}

/** Runs a bridge call and puts its failure into the store instead of throwing. */
export function attempt(call: Promise<unknown>): void {
  call.catch((error: Error) => useStore.setState({ error: reason(error) }));
}

/**
 * Signs in to `provider` the way `option` says and keeps track of it until it settles. A refusal
 * is worded with the desktop's `login-problem-*` keys.
 */
export function signIn(provider: ProviderInfo, option: SignInOption): void {
  const mark = (on: boolean) =>
    useStore.setState((s) => ({ signingIn: { ...s.signingIn, [provider.slug]: on } }));
  mark(true);
  bridge
    .signIn(provider.slug, option)
    .catch((error: Error & { code?: string }) => {
      const code = error.code ?? '';
      // The toast words the problem; the core's own message goes to the log, for whoever
      // follows a sign-in from the dev server.
      console.warn(`sign-in to ${provider.slug} failed (${code || 'no code'}): ${error.message}`);
      const message = code.startsWith(SIGN_IN)
        ? t(`login-problem-${code.slice(SIGN_IN.length).toLowerCase()}`, { provider: provider.name })
        : reason(error);
      useStore.setState({ error: message });
    })
    .finally(() => mark(false));
}

/** What a rejected native call that failed with `ERR_SIGN_IN_<PROBLEM>` starts its code with. */
const SIGN_IN = 'ERR_SIGN_IN_';

/** The core's own words for a failure, without the native module's "Calling … has failed" lead. */
export function reason(error: Error): string {
  const parts = error.message.split('Caused by: ');
  return parts[parts.length - 1].trim();
}

/** The key of an item in the store's `saved` map; ids of different kinds may collide. */
export function savedKey(kind: LibraryPart, id: string): string {
  return `${kind}|${id}`;
}

/** Whether the item is known to be in its provider's library. */
export function useSaved(kind: LibraryPart, id: string): boolean {
  return useStore((s) => s.saved[savedKey(kind, id)] === true);
}

/** Records items a library listing handed out, which are saved by definition. */
export function markSaved(kind: LibraryPart, ids: string[]): void {
  useStore.setState((s) => {
    const saved = { ...s.saved };
    ids.forEach((id) => (saved[savedKey(kind, id)] = true));
    return { saved };
  });
}

/** Forgets the hearts of the provider `slug`, whose ids all start with `slug:`. */
function dropSaved(slug: string): void {
  useStore.setState((s) => {
    const saved: Record<string, boolean> = {};
    Object.entries(s.saved).forEach(([key, on]) => {
      if (!key.slice(key.indexOf('|') + 1).startsWith(`${slug}:`)) saved[key] = on;
    });
    return { saved };
  });
}

/** Asks the core for every saved id and replaces what the store knew. */
export function loadSaved(): void {
  bridge
    .savedIds()
    .then((ids: SavedIds) => {
      const saved: Record<string, boolean> = {};
      (Object.keys(ids) as LibraryPart[]).forEach((kind) =>
        ids[kind].forEach((id) => (saved[savedKey(kind, id)] = true)),
      );
      useStore.setState({ saved });
    })
    .catch((error: Error) => useStore.setState({ error: reason(error) }));
}

/** Flips the heart at once and tells the provider; a refusal flips it back. */
export function toggleSaved(kind: LibraryPart, id: string): void {
  const key = savedKey(kind, id);
  const next = useStore.getState().saved[key] !== true;
  const put = (on: boolean) => useStore.setState((s) => ({ saved: { ...s.saved, [key]: on } }));
  put(next);
  bridge.setSaved(id, kind, next).catch((error: Error) => {
    put(!next);
    useStore.setState({ error: reason(error) });
  });
}
