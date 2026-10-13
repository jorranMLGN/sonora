import { useCallback, useEffect, useState } from 'react';

import { useStore } from './store';

/** Loads something from the bridge, reporting failures through the store; `reload` retries. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const run = useCallback(load, deps);

  const reload = useCallback(() => {
    setLoading(true);
    run()
      .then(setData)
      .catch((error: Error) => useStore.setState({ error: error.message }))
      .finally(() => setLoading(false));
  }, [run]);

  useEffect(reload, [reload]);
  return { data, loading, reload };
}
