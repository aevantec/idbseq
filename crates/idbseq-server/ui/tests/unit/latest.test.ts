import { describe, expect, test } from 'vitest';

import { STALE, createLatest } from '@/lib/latest';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

describe('createLatest', () => {
  test('returns the value of the only task', async () => {
    const latest = createLatest();

    await expect(latest(async () => 42)).resolves.toBe(42);
  });

  test('marks an older task STALE when a newer one started, whatever order they finish in', async () => {
    const latest = createLatest();
    const slow = deferred<string>();
    const fast = deferred<string>();

    const first = latest(() => slow.promise);
    const second = latest(() => fast.promise);
    fast.resolve('abc');
    slow.resolve('ab');

    await expect(second).resolves.toBe('abc');
    await expect(first).resolves.toBe(STALE);
  });

  test('keeps separate counters per view', async () => {
    const one = createLatest();
    const two = createLatest();

    const a = one(async () => 'a');
    const b = two(async () => 'b');

    await expect(a).resolves.toBe('a');
    await expect(b).resolves.toBe('b');
  });

  test('passes errors through', async () => {
    const latest = createLatest();

    await expect(latest(async () => Promise.reject(new Error('boom')))).rejects.toThrow('boom');
  });
});
