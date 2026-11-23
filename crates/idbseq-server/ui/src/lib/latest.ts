// Ignore late answers: when a view reloads (a new filter, a new page), only
// the newest request may update it, whatever order the responses arrive in.

export const STALE: unique symbol = Symbol('stale');

/** Returns `latest(task)`, which resolves to `STALE` once a newer task has started. */
export function createLatest() {
  let generation = 0;
  return async function latest<T>(task: () => Promise<T>): Promise<T | typeof STALE> {
    generation += 1;
    const mine = generation;
    const value = await task();
    return mine === generation ? value : STALE;
  };
}
