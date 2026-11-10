import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  CollectionExistsError,
  CollectionNotFoundError,
  IdbseqClient,
  ProtectedCollectionError,
  ValidationFailedError,
} from '../src/index.js';
import { fail, MockServer, ok, okRaw } from './helpers/mock-server.js';

const WIRE_COLLECTION = {
  name: 'orders',
  start: 1,
  step: 1,
  max: 1000,
  protected: false,
  durability: { mode: 'strict' },
  created_at: '2026-11-10T08:15:30Z',
  last_id: 1043,
  next_id: 1044,
};

const SDK_COLLECTION = {
  name: 'orders',
  start: 1,
  step: 1,
  max: 1000,
  protected: false,
  durability: { mode: 'strict' },
  createdAt: '2026-11-10T08:15:30Z',
  lastId: 1043,
  nextId: 1044,
};

let server: MockServer;
let client: IdbseqClient;

beforeAll(async () => {
  server = await MockServer.start();
});

afterAll(async () => {
  await server.close();
});

beforeEach(() => {
  server.reset();
  client = new IdbseqClient({ url: `${server.url}/`, apiKey: 'k' });
});

describe('listCollections', () => {
  it('returns items, total and the next cursor', async () => {
    // Arrange
    server.reply(ok([WIRE_COLLECTION], { total: 120, limit: 1, next_cursor: 'orders' }));

    // Act
    const page = await client.listCollections({ limit: 1, cursor: 'a', prefix: 'or' });

    // Assert
    expect(page).toEqual({ items: [SDK_COLLECTION], total: 120, limit: 1, nextCursor: 'orders' });
    const request = server.lastRequest;
    expect(request.method).toBe('GET');
    expect(request.path).toBe('/v1/collections');
    expect(Object.fromEntries(request.query)).toEqual({ limit: '1', cursor: 'a', prefix: 'or' });
  });

  it('sends no query parameters by default and handles the last page', async () => {
    server.reply(ok([], { total: 0, limit: 50, next_cursor: null }));

    const page = await client.listCollections();

    expect(page).toEqual({ items: [], total: 0, limit: 50, nextCursor: null });
    expect([...server.lastRequest.query.keys()]).toEqual([]);
  });

  it('decodes block durability and 64-bit maxima', async () => {
    server.reply(
      okRaw(
        '[{"name":"big","start":1,"step":1,"max":9223372036854775807,"protected":true,' +
          '"durability":{"mode":"block","size":1000},"created_at":"2026-11-10T08:15:30Z",' +
          '"last_id":null,"next_id":1}]',
      ),
    );

    const page = await client.listCollections();

    expect(page.items[0]?.max).toBe(9223372036854775807n);
    expect(page.items[0]?.durability).toEqual({ mode: 'block', size: 1000 });
    expect(page.total).toBe(1);
    expect(page.nextCursor).toBeNull();
  });
});

describe('getCollection', () => {
  it('returns the collection in camelCase', async () => {
    server.reply(ok(WIRE_COLLECTION));

    await expect(client.getCollection('orders')).resolves.toEqual(SDK_COLLECTION);
    expect(server.lastRequest.path).toBe('/v1/collections/orders');
  });

  it('throws CollectionNotFoundError for an unknown name', async () => {
    server.reply(fail(404, 'COLLECTION_NOT_FOUND', 'no collection `nope`'));

    await expect(client.getCollection('nope')).rejects.toBeInstanceOf(CollectionNotFoundError);
  });
});

describe('createCollection', () => {
  it('posts the spec in snake_case, including bigint maxima', async () => {
    // Arrange
    server.reply({ ...ok(WIRE_COLLECTION), status: 201 });

    // Act
    const created = await client.createCollection({
      name: 'orders',
      start: 1,
      step: 1,
      max: 9223372036854775807n,
      protected: true,
      durability: { mode: 'block', size: 100 },
    });

    // Assert
    expect(created).toEqual(SDK_COLLECTION);
    const request = server.lastRequest;
    expect(request.method).toBe('POST');
    expect(request.path).toBe('/v1/collections');
    expect(request.headers['content-type']).toBe('application/json');
    expect(request.body).toBe(
      '{"name":"orders","start":1,"step":1,"max":9223372036854775807,"protected":true,' +
        '"durability":{"mode":"block","size":100}}',
    );
  });

  it('omits fields that are not given', async () => {
    server.reply(ok(WIRE_COLLECTION));

    await client.createCollection({ name: 'orders' });

    expect(server.lastRequest.body).toBe('{"name":"orders"}');
  });

  it('throws CollectionExistsError when the name is taken', async () => {
    server.reply(fail(409, 'COLLECTION_EXISTS'));

    await expect(client.createCollection({ name: 'orders' })).rejects.toBeInstanceOf(CollectionExistsError);
  });

  it('exposes validation details', async () => {
    server.reply(fail(400, 'VALIDATION_FAILED', 'invalid', { fields: { name: 'bad characters' } }));

    const error = await client.createCollection({ name: 'Bad!' }).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(ValidationFailedError);
    expect((error as ValidationFailedError).details).toEqual({ fields: { name: 'bad characters' } });
    expect((error as ValidationFailedError).requestId).toBe('req-test-1');
  });
});

describe('updateCollection', () => {
  it('patches only the given fields', async () => {
    server.reply(ok(WIRE_COLLECTION));

    await client.updateCollection('orders', { max: 5000, protected: false });

    const request = server.lastRequest;
    expect(request.method).toBe('PATCH');
    expect(request.path).toBe('/v1/collections/orders');
    expect(request.body).toBe('{"max":5000,"protected":false}');
  });
});

describe('resetCollection', () => {
  it('sends confirm with the collection name and maps the result', async () => {
    // Arrange
    server.reply(ok({ previous_last_id: 1144, last_id: null, next_id: 1 }));

    // Act
    const result = await client.resetCollection('orders');

    // Assert
    expect(result).toEqual({ previousLastId: 1144, lastId: null, nextId: 1 });
    expect(server.lastRequest.path).toBe('/v1/collections/orders/reset');
    expect(server.lastRequest.body).toBe('{"confirm":"orders"}');
  });

  it('sends value and allow_rewind when given', async () => {
    server.reply(ok({ previous_last_id: 50, last_id: 9, next_id: 10 }));

    await client.resetCollection('orders', { value: 10, allowRewind: true });

    expect(server.lastRequest.body).toBe('{"confirm":"orders","value":10,"allow_rewind":true}');
  });

  it('throws ProtectedCollectionError for keys on protected collections', async () => {
    server.reply(fail(403, 'PROTECTED_COLLECTION'));

    await expect(client.resetCollection('tickets')).rejects.toBeInstanceOf(ProtectedCollectionError);
  });
});

describe('deleteCollection', () => {
  it('sends confirm and returns the collection as it was', async () => {
    server.reply(ok(WIRE_COLLECTION));

    const deleted = await client.deleteCollection('orders');

    expect(deleted).toEqual(SDK_COLLECTION);
    expect(server.lastRequest.method).toBe('DELETE');
    expect(server.lastRequest.path).toBe('/v1/collections/orders');
    expect(server.lastRequest.query.get('confirm')).toBe('orders');
  });
});
