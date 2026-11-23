import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  IdbSeqClient,
  IdbSeqNetworkError,
  InternalError,
  RateLimitedError,
  SequenceExhaustedError,
} from '../src/index.js';
import { fail, MockServer, ok, okRaw } from './helpers/mock-server.js';

const FAST_RETRY = { baseDelayMs: 1, maxDelayMs: 5 };
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

let server: MockServer;
let client: IdbSeqClient;

beforeAll(async () => {
  server = await MockServer.start();
});

afterAll(async () => {
  await server.close();
});

beforeEach(() => {
  server.reset();
  client = new IdbSeqClient({ url: server.url, apiKey: 'idb_k1_secret', retry: FAST_RETRY });
});

describe('generateId', () => {
  it('posts to the next endpoint with the key and an idempotency key', async () => {
    // Arrange
    server.reply(ok({ id: 1044 }));

    // Act
    const id = await client.generateId('orders');

    // Assert
    expect(id).toBe(1044);
    const request = server.lastRequest;
    expect(request.method).toBe('POST');
    expect(request.path).toBe('/v1/collections/orders/next');
    expect(request.query.has('count')).toBe(false);
    expect(request.headers.authorization).toBe('Bearer idb_k1_secret');
    expect(request.headers['idempotency-key']).toMatch(UUID);
  });

  it('returns a bigint for IDs beyond Number.MAX_SAFE_INTEGER', async () => {
    server.reply(okRaw('{"id":9223372036854775807}'));

    await expect(client.generateId('orders')).resolves.toBe(9223372036854775807n);
  });

  it('url-encodes the collection name', async () => {
    server.reply(ok({ id: 1 }));

    await client.generateId('eu/orders');

    expect(server.lastRequest.path).toBe('/v1/collections/eu%2Forders/next');
  });

  it('uses a caller-supplied idempotency key', async () => {
    server.reply(ok({ id: 1 }));

    await client.generateId('orders', { idempotencyKey: 'order-77' });

    expect(server.lastRequest.headers['idempotency-key']).toBe('order-77');
  });

  it('retries 5xx and network failures with the same idempotency key', async () => {
    // Arrange
    server.reply(fail(500, 'INTERNAL'), { drop: true }, ok({ id: 7 }));

    // Act
    const id = await client.generateId('orders');

    // Assert
    expect(id).toBe(7);
    expect(server.requests).toHaveLength(3);
    const keys = new Set(server.requests.map((r) => r.headers['idempotency-key']));
    expect(keys.size).toBe(1);
  });

  it('uses a new idempotency key for each logical call', async () => {
    server.reply(ok({ id: 1 }), ok({ id: 2 }));

    await client.generateId('orders');
    await client.generateId('orders');

    const [first, second] = server.requests;
    expect(first?.headers['idempotency-key']).not.toBe(second?.headers['idempotency-key']);
  });

  it('honours Retry-After on 429', async () => {
    // Arrange
    server.reply(fail(429, 'RATE_LIMITED', 'slow down', null, { 'retry-after': '1' }), ok({ id: 3 }));
    const started = Date.now();

    // Act
    const id = await client.generateId('orders');

    // Assert
    expect(id).toBe(3);
    expect(Date.now() - started).toBeGreaterThanOrEqual(900);
  });

  it('gives up with RateLimitedError when Retry-After is too long', async () => {
    server.reply(fail(429, 'RATE_LIMITED', 'slow down', null, { 'retry-after': '3600' }));

    const error = await client.generateId('orders').catch((e: unknown) => e);

    expect(error).toBeInstanceOf(RateLimitedError);
    expect((error as RateLimitedError).retryAfterSeconds).toBe(3600);
    expect(server.requests).toHaveLength(1);
  });

  it('stops after maxAttempts and throws the last error', async () => {
    // Arrange
    const limited = new IdbSeqClient({
      url: server.url,
      apiKey: 'k',
      retry: { ...FAST_RETRY, maxAttempts: 2 },
    });
    server.reply(fail(500, 'INTERNAL', 'first'), fail(503, 'INTERNAL', 'second'));

    // Act
    const error = await limited.generateId('orders').catch((e: unknown) => e);

    // Assert
    expect(error).toBeInstanceOf(InternalError);
    expect((error as InternalError).message).toBe('second');
    expect((error as InternalError).status).toBe(503);
    expect(server.requests).toHaveLength(2);
  });

  it('throws IdbSeqNetworkError when the server never answers', async () => {
    const noRetry = new IdbSeqClient({ url: server.url, apiKey: 'k', retry: false });
    server.reply({ drop: true });

    await expect(noRetry.generateId('orders')).rejects.toBeInstanceOf(IdbSeqNetworkError);
    expect(server.requests).toHaveLength(1);
  });

  it('does not retry SEQUENCE_EXHAUSTED', async () => {
    server.reply(fail(409, 'SEQUENCE_EXHAUSTED', 'exhausted'));

    await expect(client.generateId('orders')).rejects.toBeInstanceOf(SequenceExhaustedError);
    expect(server.requests).toHaveLength(1);
  });

  it('rejects an empty collection name before sending', async () => {
    await expect(client.generateId('')).rejects.toBeInstanceOf(TypeError);
    expect(server.requests).toHaveLength(0);
  });
});

describe('generateIds', () => {
  it('requests a batch and returns its bounds', async () => {
    // Arrange
    server.reply(okRaw('{"first":9223372036854775706,"last":9223372036854775805,"count":100,"step":1}'));

    // Act
    const batch = await client.generateIds('orders', 100);

    // Assert
    expect(batch).toEqual({
      first: 9223372036854775706n,
      last: 9223372036854775805n,
      count: 100,
      step: 1,
    });
    expect(server.lastRequest.query.get('count')).toBe('100');
    expect(server.lastRequest.headers['idempotency-key']).toMatch(UUID);
  });

  it('retries with the same idempotency key', async () => {
    server.reply({ drop: true }, ok({ first: 1, last: 2, count: 2, step: 1 }));

    await client.generateIds('orders', 2, { idempotencyKey: 'batch-1' });

    expect(server.requests.map((r) => r.headers['idempotency-key'])).toEqual(['batch-1', 'batch-1']);
  });

  it.each([0, -1, 1.5, Number.NaN])('rejects count %s before sending', async (count) => {
    await expect(client.generateIds('orders', count)).rejects.toBeInstanceOf(RangeError);
    expect(server.requests).toHaveLength(0);
  });
});

describe('lastId', () => {
  it('returns the last and next IDs', async () => {
    server.reply(ok({ last_id: 1144, next_id: 1145 }));

    await expect(client.lastId('orders')).resolves.toEqual({ lastId: 1144, nextId: 1145 });
    expect(server.lastRequest.method).toBe('GET');
    expect(server.lastRequest.path).toBe('/v1/collections/orders/last');
  });

  it('passes nulls through before the first ID and when exhausted', async () => {
    server.reply(ok({ last_id: null, next_id: 1 }), okRaw('{"last_id":9223372036854775807,"next_id":null}'));

    await expect(client.lastId('a')).resolves.toEqual({ lastId: null, nextId: 1 });
    await expect(client.lastId('b')).resolves.toEqual({ lastId: 9223372036854775807n, nextId: null });
  });

  it('is not retried', async () => {
    server.reply(fail(500, 'INTERNAL'));

    await expect(client.lastId('orders')).rejects.toBeInstanceOf(InternalError);
    expect(server.requests).toHaveLength(1);
  });
});
