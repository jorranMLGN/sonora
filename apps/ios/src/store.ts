import { create } from 'zustand';

import type { PlayerState, Prompt, ProviderInfo, QueueSnapshot, SessionState } from './bridge';
import { bridge } from './bridgeImpl';

type StoreState = {
  providers: ProviderInfo[];
  sessions: Record<string, SessionState>;
  prompts: Record<string, Prompt | undefined>;
  player: PlayerState | null;
  positionMs: number;
  queue: QueueSnapshot | null;
  error: string | null;
  clearError(): void;
  clearPrompt(slug: string): void;
};

/** The one store. Bridge events feed it; screens read it and call bridge functions. */
export const useStore = create<StoreState>((set) => ({
  providers: [],
  sessions: {},
  prompts: {},
  player: null,
  positionMs: 0,
  queue: null,
  error: null,
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
  bridge.on('onSession', ({ slug, state }) =>
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
    })),
  );
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
  call.catch((error: Error) => useStore.setState({ error: error.message }));
}
