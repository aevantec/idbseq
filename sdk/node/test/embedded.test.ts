import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  CollectionExistsError,
  CollectionNotFoundError,
  DatabaseBusyError,
  DatabaseClosedError,
  DatabaseUnavailableError,
  IdbSeqDatabase,
  SequenceExhaustedError,
  ValidationFailedError,
  type Sequences,
} from '../src/index.js';

let dir: string;
let path: string;
const open: IdbSeqDatabase[] = [];

async function openDb(options: Parameters<typeof IdbSeqDatabase.open>[1] = {}): Promise<IdbSeqDatabase> {
  const db = await IdbSeqDatabase.open(path, options);
  open.push(db);
  return db;
}

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'idbseq-embedded-'));
  path = join(dir, 'app.idbseq');
});

afterEach(async () => {
  await Promise.all(open.splice(0).map((db) => db.close()));
  rmSync(dir, { recursive: true, force: true });
});

describe('IDs', () => {
  it('issues increasing IDs, contiguous batches and the last ID', async () => {
    // Arrange
    const db = await openDb();
    await db.createCollection({ name: 'invoices', start: 1000 });

    // Act
    const first = await db.generateId('invoices');
    const batch = await db.generateIds('invoices', 3);
    const last = await db.lastId('invoices');

    // Assert
    expect(first).toBe(1000);
    expect(batch).toEqual({ first: 1001, last: 1003, count: 3, step: 1 });
    expect(last).toEqual({ lastId: 1003, nextId: 1004 });
  });

  it('returns bigint above Number.MAX_SAFE_INTEGER', async () => {
    const db = await openDb();
    await db.createCollection({ name: 'big', start: 9007199254740992n });

    expect(await db.generateId('big')).toBe(9007199254740992n);
  });

  it('replays a repeated idempotency key across reopening', async () => {
    // Arrange
    const db = await openDb();
    await db.createCollection({ name: 'orders' });
    const first = await db.generateIds('orders', 2, { idempotencyKey: 'checkout-1' });
    await db.close();

    // Act
    const again = await (await openDb()).generateIds('orders', 2, { idempotencyKey: 'checkout-1' });

    // Assert
    expect(again).toEqual(first);
  });

  it('never repeats an ID across concurrent calls', async () => {
    // Arrange
    const db = await openDb();
    await db.createCollection({ name: 'strict' });
    await db.createCollection({ name: 'fast', durability: { mode: 'block', size: 50 } });

    // Act
    const ids = await Promise.all(
      Array.from({ length: 200 }, (_, i) => db.generateId(i % 2 === 0 ? 'strict' : 'fast')),
    );

    // Assert
    expect(new Set(ids.map((id, i) => `${i % 2}:${id}`)).size).toBe(200);
  });

  it('rejects a count below 1 before calling the module', async () => {
    const db = await openDb();

    await expect(db.generateIds('x', 0)).rejects.toThrow(RangeError);
  });
});

describe('collections', () => {
  it('creates, lists, updates, resets and deletes like the server', async () => {
    // Arrange
    const db = await openDb();
    for (const name of ['a.one', 'a.two', 'b.three']) {
      await db.createCollection({ name });
    }
    await db.generateIds('a.one', 5);

    // Act
    const page = await db.listCollections({ prefix: 'a.', limit: 1 });
    const updated = await db.updateCollection('a.one', { max: 100 });
    const reset = await db.resetCollection('a.one', { value: 2, allowRewind: true });
    const deleted = await db.deleteCollection('b.three');
    const fetched = await db.getCollection('a.one');

    // Assert
    expect(page.items.map((c) => c.name)).toEqual(['a.one']);
    expect(page).toMatchObject({ total: 2, limit: 1, nextCursor: 'a.one' });
    expect(updated.max).toBe(100);
    expect(reset).toEqual({ previousLastId: 5, lastId: 1, nextId: 2 });
    expect(deleted.name).toBe('b.three');
    expect(fetched).toMatchObject({ name: 'a.one', step: 1, protected: false, durability: { mode: 'strict' } });
    expect(fetched.createdAt).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/);
  });
});

describe('errors', () => {
  it('throws the same error classes as the HTTP client', async () => {
    // Arrange
    const db = await openDb();
    await db.createCollection({ name: 'tiny', max: 1 });
    await db.generateId('tiny');

    // Act + Assert
    await expect(db.getCollection('nope')).rejects.toBeInstanceOf(CollectionNotFoundError);
    await expect(db.createCollection({ name: 'tiny' })).rejects.toBeInstanceOf(CollectionExistsError);
    await expect(db.generateId('tiny')).rejects.toBeInstanceOf(SequenceExhaustedError);
    const invalid = await db.createCollection({ name: 'Bad Name' }).catch((error: unknown) => error);
    expect(invalid).toBeInstanceOf(ValidationFailedError);
    expect(invalid).toMatchObject({
      code: 'VALIDATION_FAILED',
      status: 0,
      details: { fields: { name: expect.any(String) } },
    });
  });

  it('reports a missing folder and a held file', async () => {
    await openDb();

    await expect(IdbSeqDatabase.open(join(dir, 'missing', 'x.idbseq'))).rejects.toBeInstanceOf(
      DatabaseUnavailableError,
    );
    await expect(IdbSeqDatabase.open(path, { busyTimeoutMs: 20 })).rejects.toBeInstanceOf(DatabaseBusyError);
  });

  it('refuses calls after close, and closing twice is fine', async () => {
    const db = await openDb();
    await db.createCollection({ name: 'x' });

    await db.close();
    await db.close();

    await expect(db.generateId('x')).rejects.toBeInstanceOf(DatabaseClosedError);
  });

  it('checks its options', async () => {
    await expect(IdbSeqDatabase.open('')).rejects.toThrow(TypeError);
    await expect(IdbSeqDatabase.open(path, { busyTimeoutMs: -1 })).rejects.toThrow(RangeError);
    await expect(IdbSeqDatabase.open(path, { busyTimeoutMs: 1.5 })).rejects.toThrow(RangeError);
  });
});

describe('modes', () => {
  it('lets go of the file after close, so another handle can open it', async () => {
    const first = await openDb();
    await first.createCollection({ name: 'x' });
    await first.close();

    const second = await openDb();

    expect(await second.generateId('x')).toBe(1);
  });

  it('shares the file between handles in shared mode', async () => {
    // Arrange
    const a = await openDb({ shared: true });
    const b = await openDb({ shared: true });
    await a.createCollection({ name: 'x', durability: { mode: 'block', size: 100 } });

    // Act
    const ids = [await a.generateId('x'), await b.generateId('x'), await a.generateId('x')];

    // Assert: shared mode writes block collections like strict ones, so no gaps.
    expect(ids).toEqual([1, 2, 3]);
  });

  it('works with code written for the HTTP client', async () => {
    const reserve = (sequences: Sequences): Promise<unknown> => sequences.generateId('x');
    const db = await openDb();
    await db.createCollection({ name: 'x' });

    expect(await reserve(db)).toBe(1);
  });
});

describe('the audit fixes of 2026-11-18', () => {
  it('close() waits for running calls, then the file can be opened at once', async () => {
    // Arrange
    const db = await IdbSeqDatabase.open(path);
    await db.createCollection({ name: 'x' });
    const running = Array.from({ length: 20 }, () => db.generateId('x'));

    // Act
    await db.close();
    const reopened = await openDb({ busyTimeoutMs: 0 });

    // Assert: every call made before close() finished, none was cut off.
    expect((await Promise.all(running)).sort((a, b) => Number(a) - Number(b))).toEqual(
      Array.from({ length: 20 }, (_, i) => i + 1),
    );
    expect(await reopened.generateId('x')).toBe(21);
  });

  it('calls waiting for a busy file leave Node’s thread pool free', async () => {
    // Arrange: a shared handle, then an exclusive one that holds the file.
    const shared = await openDb({ shared: true, busyTimeoutMs: 1500 });
    const holder = await openDb();
    await holder.createCollection({ name: 'x' });
    const waiting = Array.from({ length: 6 }, () => shared.lastId('x').catch((error: unknown) => error));

    // Act: Node's pool (4 threads) serves file reads; they must not queue behind
    // the waits. (Another file: Windows locks the database file while it is open.)
    const other = join(dir, 'other.txt');
    writeFileSync(other, 'x');
    const started = performance.now();
    await readFile(other);
    const elapsed = performance.now() - started;

    // Assert
    expect(elapsed).toBeLessThan(500);
    for (const result of await Promise.all(waiting)) expect(result).toBeInstanceOf(DatabaseBusyError);
  });

  it('a shared file deleted while open is reported, never replaced by an empty one', async () => {
    const db = await openDb({ shared: true });
    await db.createCollection({ name: 'x' });
    expect(await db.generateId('x')).toBe(1);

    rmSync(path);

    await expect(db.createCollection({ name: 'x' })).rejects.toBeInstanceOf(DatabaseUnavailableError);
    expect(existsSync(path)).toBe(false);
  });

  it('idempotency keys follow the same rules as on a server', async () => {
    const db = await openDb();
    await db.createCollection({ name: 'x' });

    for (const key of ['', 'x'.repeat(256), 'ü ü']) {
      await expect(db.generateId('x', { idempotencyKey: key })).rejects.toBeInstanceOf(ValidationFailedError);
    }
    expect(await db.generateId('x', { idempotencyKey: 'k1' })).toBe(await db.generateId('x', { idempotencyKey: 'k1' }));
  });
});
