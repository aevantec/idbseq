import { type Collection, ForbiddenError, IdbSeqNetworkError, SequenceExhaustedError } from 'idbseq';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { connect, fakeApi, textOf, type Connected, type FakeApi } from './helpers/connect.js';

const ORDERS: Collection = {
  name: 'orders',
  start: 1,
  step: 1,
  max: 9223372036854775807n,
  protected: false,
  durability: { mode: 'strict' },
  createdAt: '2026-11-17T08:00:00Z',
  lastId: 1043,
  nextId: 1044,
};

const ORDERS_WIRE =
  '{"name":"orders","start":1,"step":1,"max":9223372036854775807,"protected":false,' +
  '"durability":{"mode":"strict"},"created_at":"2026-11-17T08:00:00Z","last_id":1043,"next_id":1044}';

function apiError<T>(ErrorClass: new (init: ConstructorParameters<typeof SequenceExhaustedError>[0]) => T, code: string, message: string): T {
  return new ErrorClass({ message, code, status: 409, details: null, requestId: 'req-1', retryAfterSeconds: null });
}

describe('tools', () => {
  let api: FakeApi;
  let mcp: Connected;

  beforeEach(async () => {
    api = fakeApi();
    mcp = await connect(api);
  });

  afterEach(async () => {
    await mcp.close();
  });

  it('offers read and generate tools only by default', async () => {
    const { tools } = await mcp.client.listTools();

    expect(tools.map((tool) => tool.name).sort()).toEqual(['get_collection', 'list_collections', 'next_id']);
  });

  it('tells the agent the rules in the server instructions', () => {
    expect(mcp.client.getInstructions()).toMatch(/never reused.*gaps are normal/s);
  });

  it('reports the package version', () => {
    expect(mcp.client.getServerVersion()).toEqual({ name: 'idbseq', version: '0.1.0' });
  });

  describe('next_id', () => {
    it('reserves one ID', async () => {
      api.generateId.mockResolvedValue(1044);

      const result = await mcp.call('next_id', { collection: 'orders' });

      expect(result.isError).toBeFalsy();
      expect(textOf(result)).toBe('{"id":1044}');
      expect(api.generateId).toHaveBeenCalledWith('orders', {});
    });

    it('reserves a batch with an idempotency key', async () => {
      api.generateIds.mockResolvedValue({ first: 1045, last: 1047, count: 3, step: 1 });

      const result = await mcp.call('next_id', { collection: 'orders', count: 3, idempotency_key: 'retry-7' });

      expect(textOf(result)).toBe('{"first":1045,"last":1047,"count":3,"step":1}');
      expect(api.generateIds).toHaveBeenCalledWith('orders', 3, { idempotencyKey: 'retry-7' });
    });

    it('writes IDs above 2^53 exactly', async () => {
      api.generateId.mockResolvedValue(9007199254740993n);

      const result = await mcp.call('next_id', { collection: 'orders' });

      expect(textOf(result)).toBe('{"id":9007199254740993}');
    });

    it('returns an API error as a tool error with its code', async () => {
      api.generateId.mockRejectedValue(
        apiError(SequenceExhaustedError, 'SEQUENCE_EXHAUSTED', 'collection `orders` is exhausted'),
      );

      const result = await mcp.call('next_id', { collection: 'orders' });

      expect(result.isError).toBe(true);
      expect(JSON.parse(textOf(result))).toEqual({
        code: 'SEQUENCE_EXHAUSTED',
        message: 'collection `orders` is exhausted',
        details: null,
      });
    });

    it('returns a network failure as a tool error', async () => {
      api.generateId.mockRejectedValue(new IdbSeqNetworkError('request failed: fetch failed'));

      const result = await mcp.call('next_id', { collection: 'orders' });

      expect(result.isError).toBe(true);
      expect(JSON.parse(textOf(result))).toMatchObject({ code: 'NETWORK_ERROR' });
    });

    it('returns an unexpected exception as a tool error', async () => {
      api.generateId.mockRejectedValue(new TypeError('boom'));

      const result = await mcp.call('next_id', { collection: 'orders' });

      expect(JSON.parse(textOf(result))).toEqual({ code: 'CLIENT_ERROR', message: 'boom', details: null });
    });

    it('rejects an uppercase collection name before calling the server', async () => {
      const result = await mcp.call('next_id', { collection: 'Orders' });

      expect(result.isError).toBe(true);
      expect(textOf(result)).toMatch(/lowercase/);
      expect(api.generateId).not.toHaveBeenCalled();
    });

    it('rejects a count below 1', async () => {
      const result = await mcp.call('next_id', { collection: 'orders', count: 0 });

      expect(result.isError).toBe(true);
      expect(api.generateIds).not.toHaveBeenCalled();
    });
  });

  describe('list_collections', () => {
    it('lists a page in wire names', async () => {
      api.listCollections.mockResolvedValue({ items: [ORDERS], total: 3, limit: 1, nextCursor: 'orders' });

      const result = await mcp.call('list_collections', { prefix: 'or', limit: 1 });

      expect(textOf(result)).toBe(`{"collections":[${ORDERS_WIRE}],"total":3,"next_cursor":"orders"}`);
      expect(api.listCollections).toHaveBeenCalledWith({ prefix: 'or', limit: 1 });
    });

    it('sends no options when none are given', async () => {
      api.listCollections.mockResolvedValue({ items: [], total: 0, limit: 50, nextCursor: null });

      await mcp.call('list_collections');

      expect(api.listCollections).toHaveBeenCalledWith({});
    });
  });

  describe('get_collection', () => {
    it('shows a collection', async () => {
      api.getCollection.mockResolvedValue(ORDERS);

      const result = await mcp.call('get_collection', { collection: 'orders' });

      expect(textOf(result)).toBe(ORDERS_WIRE);
    });

    it('passes on a scope error', async () => {
      api.getCollection.mockRejectedValue(apiError(ForbiddenError, 'FORBIDDEN', 'key lacks the read scope'));

      const result = await mcp.call('get_collection', { collection: 'orders' });

      expect(JSON.parse(textOf(result))).toMatchObject({ code: 'FORBIDDEN' });
    });
  });
});

describe('create_collection (opt-in)', () => {
  let api: FakeApi;
  let mcp: Connected;

  beforeEach(async () => {
    api = fakeApi();
    mcp = await connect(api, true);
  });

  afterEach(async () => {
    await mcp.close();
  });

  it('is offered with --allow-create, and delete and reset still are not', async () => {
    const { tools } = await mcp.client.listTools();

    expect(tools.map((tool) => tool.name).sort()).toEqual([
      'create_collection',
      'get_collection',
      'list_collections',
      'next_id',
    ]);
  });

  it('creates a strict collection by default', async () => {
    api.createCollection.mockResolvedValue(ORDERS);

    const result = await mcp.call('create_collection', { name: 'orders', start: 1000 });

    expect(textOf(result)).toBe(ORDERS_WIRE);
    expect(api.createCollection).toHaveBeenCalledWith({ name: 'orders', start: 1000 });
  });

  it('takes 64-bit values as decimal strings', async () => {
    api.createCollection.mockResolvedValue(ORDERS);

    await mcp.call('create_collection', { name: 'big', start: '9007199254740993', max: '9223372036854775807' });

    expect(api.createCollection).toHaveBeenCalledWith({
      name: 'big',
      start: 9007199254740993n,
      max: 9223372036854775807n,
    });
  });

  it('turns small decimal strings into numbers', async () => {
    api.createCollection.mockResolvedValue(ORDERS);

    await mcp.call('create_collection', { name: 'small', step: '5' });

    expect(api.createCollection).toHaveBeenCalledWith({ name: 'small', step: 5 });
  });

  it('refuses a value outside the 64-bit range without calling the server', async () => {
    const result = await mcp.call('create_collection', { name: 'big', max: '9223372036854775808' });

    expect(result.isError).toBe(true);
    expect(textOf(result)).toMatch(/outside the signed 64-bit range/);
    expect(api.createCollection).not.toHaveBeenCalled();
  });

  it('maps block_size to block durability', async () => {
    api.createCollection.mockResolvedValue(ORDERS);

    await mcp.call('create_collection', { name: 'events', step: 2, max: 500, block_size: 100 });

    expect(api.createCollection).toHaveBeenCalledWith({
      name: 'events',
      step: 2,
      max: 500,
      durability: { mode: 'block', size: 100 },
    });
  });
});
