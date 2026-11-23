import { describe, expect, test } from 'vitest';

import { buildApiPreview, jsId, jsString, type ApiOperation } from '@/lib/apiPreview';
import { parseJson } from '@/lib/json';
import type { Collection } from '@/lib/types';

const URL = 'http://127.0.0.1:7878';
const ORDERS: Collection = {
  name: 'orders',
  start: 1000,
  step: 1,
  max: 9223372036854775807n,
  protected: false,
  durability: { mode: 'strict' },
  created_at: '2026-11-17T09:50:49Z',
  last_id: 1249,
  next_id: 1250,
};

function operation(collection: Collection, id: string): ApiOperation {
  const found = buildApiPreview(collection, { url: URL }).find((op) => op.id === id);
  if (!found) throw new Error(`no operation ${id}`);
  return found;
}

function responseData(op: ApiOperation): unknown {
  return (parseJson(op.response) as { data: unknown }).data;
}

describe('quoting', () => {
  test('quotes strings as single-quoted JavaScript literals', () => {
    expect(jsString('orders')).toBe("'orders'");
    expect(jsString("it's a\\b")).toBe("'it\\'s a\\\\b'");
  });

  test('writes IDs beyond 2^53 as BigInt literals for JavaScript', () => {
    expect(jsId(1250)).toBe('1250');
    expect(jsId(9223372036854775807n)).toBe('9223372036854775807n');
    expect(jsId(BigInt(42))).toBe('42');
  });
});

describe('buildApiPreview', () => {
  test('lists the collection operations in order', () => {
    expect(buildApiPreview(ORDERS, { url: URL }).map((op) => op.id)).toEqual([
      'next', 'next-batch', 'last', 'get', 'update', 'reset', 'delete',
    ]);
  });

  test('generate an ID: SDK call with client setup, the endpoint, scope and the real next ID', () => {
    const op = operation(ORDERS, 'next');

    expect(op.method).toBe('POST');
    expect(op.path).toBe('/v1/collections/orders/next');
    expect(op.access).toContain('generate');
    expect(op.code).toContain("import { IdbSeqClient } from 'idbseq';");
    expect(op.code).toContain(`url: '${URL}'`);
    expect(op.code).toContain('apiKey: process.env.IDBSEQ_KEY');
    expect(op.code).toContain("const id = await ids.generateId('orders');");
    expect(parseJson(op.response)).toEqual({ success: true, data: { id: 1250 }, error: null });
  });

  test('a batch stops at max, so the example is exactly what the server would return', () => {
    expect(responseData(operation(ORDERS, 'next-batch'))).toEqual({ first: 1250, last: 1259, count: 10, step: 1 });
    const nearlyFull = { ...ORDERS, step: 5, max: 1260, next_id: 1250 };
    expect(responseData(operation(nearlyFull, 'next-batch'))).toEqual({ first: 1250, last: 1260, count: 3, step: 5 });
    expect(operation(ORDERS, 'next-batch').code).toContain("await ids.generateIds('orders', 10);");
  });

  test('an exhausted collection shows the SEQUENCE_EXHAUSTED error', () => {
    const exhausted = { ...ORDERS, max: 1249, next_id: null };
    for (const id of ['next', 'next-batch']) {
      expect(parseJson(operation(exhausted, id).response)).toEqual({
        success: false,
        data: null,
        error: { code: 'SEQUENCE_EXHAUSTED', message: 'collection `orders` is exhausted' },
      });
    }
  });

  test('reads show the current values', () => {
    expect(operation(ORDERS, 'last').method).toBe('GET');
    expect(operation(ORDERS, 'last').access).toContain('read');
    expect(responseData(operation(ORDERS, 'last'))).toEqual({ last_id: 1249, next_id: 1250 });
    expect(operation(ORDERS, 'last').code).toContain("const { lastId, nextId } = await ids.lastId('orders');");
    expect(responseData(operation(ORDERS, 'get'))).toEqual(ORDERS);
    expect(operation(ORDERS, 'get').code).toContain("const collection = await ids.getCollection('orders');");
  });

  test('64-bit values stay exact in code and responses', () => {
    const big = { ...ORDERS, start: 9223372036854775000n, last_id: 9223372036854775001n, next_id: 9223372036854775002n };

    expect(operation(big, 'next').response).toContain('"id": 9223372036854775002');
    expect(operation(big, 'update').code).toContain('max: 9223372036854775807n');
  });

  test('reset skips ahead by 1000 steps when that fits, else to the next ID', () => {
    const ahead = operation(ORDERS, 'reset');
    expect(ahead.code).toContain("await ids.resetCollection('orders', { value: 2250 });");
    expect(responseData(ahead)).toEqual({ previous_last_id: 1249, last_id: 2249, next_id: 2250 });

    const tight = { ...ORDERS, max: 2000 };
    expect(responseData(operation(tight, 'reset'))).toEqual({ previous_last_id: 1249, last_id: 1249, next_id: 1250 });
  });

  test('reset of an exhausted collection rewinds to start, which needs allowRewind', () => {
    const exhausted = { ...ORDERS, max: 1249, next_id: null };
    const op = operation(exhausted, 'reset');

    expect(op.code).toContain("await ids.resetCollection('orders', { value: 1000, allowRewind: true });");
    expect(responseData(op)).toEqual({ previous_last_id: 1249, last_id: null, next_id: 1000 });
  });

  test('update and delete use the manage scope and return the collection', () => {
    for (const id of ['update', 'delete']) {
      expect(operation(ORDERS, id).access).toContain('manage');
      expect(responseData(operation(ORDERS, id))).toEqual(ORDERS);
    }
    expect(operation(ORDERS, 'update').method).toBe('PATCH');
    expect(operation(ORDERS, 'delete').method).toBe('DELETE');
    expect(operation(ORDERS, 'delete').code).toContain("await ids.deleteCollection('orders');");
  });

  test('changes to a protected collection sign in as the admin instead of using a key', () => {
    const locked = { ...ORDERS, protected: true };
    for (const id of ['update', 'reset', 'delete']) {
      const op = operation(locked, id);
      expect(op.access).toContain('admin');
      expect(op.code).toContain("await ids.login('admin', process.env.IDBSEQ_ADMIN_PASSWORD ?? '');");
      expect(op.code).not.toContain('apiKey');
    }
    expect(operation(locked, 'next').code).toContain('apiKey: process.env.IDBSEQ_KEY');
  });

  test('names are quoted in code and escaped in paths', () => {
    const odd = { ...ORDERS, name: "eu it's" };
    const op = operation(odd, 'next');

    expect(op.path).toBe("/v1/collections/eu%20it's/next");
    expect(op.code).toContain("ids.generateId('eu it\\'s')");
  });
});

describe('embedded code', () => {
  test('opens the file instead of a server, with no key or admin', () => {
    const operations = buildApiPreview(ORDERS, { file: "app's.idbseq" });

    for (const operation of operations) {
      expect(operation.code).toContain("const ids = await IdbSeqDatabase.open('app\\'s.idbseq');");
      expect(operation.code).not.toContain('IdbSeqClient');
      expect(operation.access).toMatch(/opens the file/);
    }
  });
});

describe('jsString', () => {
  test('escapes what would end or break a string literal', () => {
    const tricky = "a'b\\c\nd\re\u2028f\u2029g";

    const literal = jsString(tricky);

    expect(literal).not.toMatch(/[\n\r\u2028\u2029]/);
    expect(new Function(`return ${literal};`)()).toBe(tricky);
  });
});
