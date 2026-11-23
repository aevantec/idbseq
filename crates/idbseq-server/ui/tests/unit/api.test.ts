import assert from 'node:assert/strict';
import { describe, test } from 'vitest';

import { ApiError, createApi, describeError, fieldErrors, withQuery } from '@/lib/api';

interface Fake {
  body?: string;
  status?: number;
  headers?: Record<string, string>;
}
interface Call {
  url: string;
  init: RequestInit & { headers: Record<string, string>; body?: string };
}

function fakeFetch(responses: Array<Fake | Error>) {
  const calls: Call[] = [];
  const queue = [...responses];
  const fetchImpl = (async (url: string, init: Call['init']) => {
    calls.push({ url, init });
    const next = queue.shift() ?? {};
    if (next instanceof Error) throw next;
    return new Response(next.body ?? '', { status: next.status ?? 200, headers: next.headers ?? {} });
  }) as unknown as typeof fetch;
  return { fetchImpl, calls: calls as Array<Call & { init: Call['init'] }> };
}

const ok = (data: unknown, meta?: unknown): Fake => ({
  body: JSON.stringify({ success: true, data, error: null, ...(meta ? { meta } : {}) }),
});
const call = (calls: Call[], index: number): Call => {
  const found = calls[index];
  if (!found) throw new Error(`no call ${index}`);
  return found;
};

describe('createApi', () => {
  test('sends the session token as a bearer token and returns data and meta', async () => {
    const { fetchImpl, calls } = fakeFetch([ok([{ name: 'orders' }], { total: 1, limit: 50 })]);
    const api = createApi({ getToken: () => 'tok', fetchImpl });

    const result = await api.listCollections({ limit: 50, prefix: '', cursor: null });

    assert.deepEqual(result, { data: [{ name: 'orders' }], meta: { total: 1, limit: 50 } });
    assert.equal(call(calls, 0).url, '/v1/collections?limit=50');
    assert.equal(call(calls, 0).init.headers.authorization, 'Bearer tok');
    assert.equal(call(calls, 0).init.credentials, 'omit');
  });

  test('logs in as admin with a JSON body and no token', async () => {
    const { fetchImpl, calls } = fakeFetch([ok({ token: 't', expires_at: '2026-11-17T10:15:00Z' })]);
    const api = createApi({ getToken: () => undefined, fetchImpl });

    await api.login('secret');

    assert.equal(call(calls, 0).init.method, 'POST');
    assert.equal(call(calls, 0).init.headers.authorization, undefined);
    assert.equal(call(calls, 0).init.headers['content-type'], 'application/json');
    assert.deepEqual(JSON.parse(String(call(calls, 0).init.body)), { username: 'admin', password: 'secret' });
  });

  test('keeps 64-bit IDs exact in both directions', async () => {
    const { fetchImpl, calls } = fakeFetch([{ body: '{"success":true,"data":{"first":9223372036854775806,"last":9223372036854775807,"count":2,"step":1},"error":null}' }, ok({}, undefined)]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    const { data } = await api.next('big', 2);
    assert.ok(data);
    await api.updateCollection('big', { max: 9223372036854775807n });

    assert.equal(data.last, 9223372036854775807n);
    assert.equal(call(calls, 1).init.body, '{"max":9223372036854775807}');
  });

  test('encodes names in paths and confirmation in the query', async () => {
    const { fetchImpl, calls } = fakeFetch([ok(null, undefined), ok(null, undefined), ok(null, undefined)]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    await api.deleteCollection('a/b c', 'a/b c');
    await api.revokeKey('k 1');
    await api.reset('eu.orders', { value: null, allow_rewind: false, confirm: 'eu.orders' });

    assert.equal(call(calls, 0).url, '/v1/collections/a%2Fb%20c?confirm=a%2Fb+c');
    assert.equal(call(calls, 0).init.method, 'DELETE');
    assert.equal(call(calls, 1).url, '/v1/keys/k%201');
    assert.equal(call(calls, 2).url, '/v1/collections/eu.orders/reset');
  });

  test('turns an error envelope into an ApiError with code, details and request ID', async () => {
    const body = JSON.stringify({ success: false, data: null, error: { code: 'VALIDATION_FAILED', message: 'invalid input', details: [{ field: 'step' }] } });
    const { fetchImpl } = fakeFetch([{ status: 400, body, headers: { 'x-request-id': 'req-1' } }]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    const error = await api.createCollection({ name: 'x', step: 0 }).catch((caught) => caught);

    assert.ok(error instanceof ApiError);
    assert.deepEqual([error.status, error.code, error.message, error.requestId], [400, 'VALIDATION_FAILED', 'invalid input', 'req-1']);
    assert.deepEqual(error.details, [{ field: 'step' }]);
  });

  test('calls onUnauthorized when a sent token is rejected', async () => {
    const body = JSON.stringify({ success: false, data: null, error: { code: 'UNAUTHORIZED', message: 'expired' } });
    const { fetchImpl } = fakeFetch([{ status: 401, body }]);
    let ended = 0;
    const api = createApi({ getToken: () => 't', onUnauthorized: () => (ended += 1), fetchImpl });

    await assert.rejects(api.listKeys(), { code: 'UNAUTHORIZED' });
    assert.equal(ended, 1);
  });

  test('ignores a late 401 for a token that is no longer the current one', async () => {
    const body = JSON.stringify({ success: false, data: null, error: { code: 'UNAUTHORIZED', message: 'expired' } });
    let token = 'old';
    let release!: () => void;
    const gate = new Promise<void>((done) => (release = done));
    const fetchImpl = (async () => {
      await gate;
      return new Response(body, { status: 401 });
    }) as typeof fetch;
    let ended = 0;
    const api = createApi({ getToken: () => token, onUnauthorized: () => (ended += 1), fetchImpl });

    const pending = api.listKeys();
    token = 'new';
    release();

    await assert.rejects(pending, { code: 'UNAUTHORIZED' });
    assert.equal(ended, 0);
  });

  test('changes the password, and a wrong current password does not end the session', async () => {
    const body = JSON.stringify({ success: false, data: null, error: { code: 'UNAUTHORIZED', message: 'current password is incorrect' } });
    const { fetchImpl, calls } = fakeFetch([{ status: 401, body }, ok(null, undefined)]);
    let ended = 0;
    const api = createApi({ getToken: () => 't', onUnauthorized: () => (ended += 1), fetchImpl });

    await assert.rejects(api.changePassword('wrong one', 'a new password!'), { code: 'UNAUTHORIZED', message: 'current password is incorrect' });
    await api.changePassword('right one', 'a new password!');

    assert.equal(ended, 0);
    assert.equal(call(calls, 1).url, '/v1/admin/password');
    assert.equal(call(calls, 1).init.method, 'POST');
    assert.deepEqual(JSON.parse(String(call(calls, 1).init.body)), { current_password: 'right one', new_password: 'a new password!' });
  });

  test('completes first-run setup with the link token and returns a session', async () => {
    const session = { token: 's', expires_at: '2026-01-01T00:15:00Z' };
    const { fetchImpl, calls } = fakeFetch([ok(session, undefined)]);
    const api = createApi({ getToken: () => null, fetchImpl });

    const { data } = await api.setup('link-token', 'a chosen password');

    assert.deepEqual(data, session);
    assert.equal(call(calls, 0).url, '/v1/auth/setup');
    assert.equal(call(calls, 0).init.method, 'POST');
    assert.deepEqual(JSON.parse(String(call(calls, 0).init.body)), { token: 'link-token', password: 'a chosen password' });
  });

  test('refreshes the admin session', async () => {
    const { fetchImpl, calls } = fakeFetch([ok({ expires_at: '2026-01-01T00:30:00Z' }, undefined)]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    const { data } = await api.refresh();

    assert.deepEqual(data, { expires_at: '2026-01-01T00:30:00Z' });
    assert.equal(call(calls, 0).url, '/v1/auth/refresh');
    assert.equal(call(calls, 0).init.method, 'POST');
  });

  test('does not end the session for a failed login, which sends no token', async () => {
    const body = JSON.stringify({ success: false, data: null, error: { code: 'UNAUTHORIZED', message: 'bad password' } });
    const { fetchImpl } = fakeFetch([{ status: 401, body }]);
    let ended = 0;
    const api = createApi({ getToken: () => undefined, onUnauthorized: () => (ended += 1), fetchImpl });

    await assert.rejects(api.login('wrong'), { code: 'UNAUTHORIZED' });
    assert.equal(ended, 0);
  });

  test('reports network failures as NETWORK', async () => {
    const { fetchImpl } = fakeFetch([new TypeError('Failed to fetch')]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    await assert.rejects(api.health(), { code: 'NETWORK', message: 'Cannot reach the server: Failed to fetch' });
  });

  test('reports a non-JSON or empty error response as INTERNAL', async () => {
    const { fetchImpl } = fakeFetch([{ status: 502, body: '<html>bad gateway</html>' }, { status: 500 }]);
    const api = createApi({ getToken: () => 't', fetchImpl });

    await assert.rejects(api.ready(), { code: 'INTERNAL', status: 502 });
    await assert.rejects(api.ready(), { code: 'INTERNAL', status: 500 });
  });

  test('returns the raw contract document, which has no envelope', async () => {
    const { fetchImpl } = fakeFetch([{ body: '{"openapi":"3.1.0","info":{"version":"0.1.0"}}' }]);
    const api = createApi({ getToken: () => undefined, fetchImpl });

    const { data } = await api.contract();

    assert.equal(data.info?.version, '0.1.0');
  });

  test('covers every remaining endpoint with the right method and path', async () => {
    const { fetchImpl, calls } = fakeFetch(Array.from({ length: 6 }, () => ok({}, undefined)));
    const api = createApi({ getToken: () => 't', fetchImpl });

    await api.logout();
    await api.getCollection('orders');
    await api.createCollection({ name: 'n' });
    await api.createKey({ name: 'k', scopes: ['read'], collections: [] });
    await api.listAudit({ action: 'key.create', cursor: 'abc', collection: '' });
    await api.ready();

    assert.deepEqual(
      calls.map(({ url, init }) => `${init.method} ${url}`),
      ['POST /v1/auth/logout', 'GET /v1/collections/orders', 'POST /v1/collections', 'POST /v1/keys', 'GET /v1/audit?action=key.create&cursor=abc', 'GET /readyz'],
    );
  });
});

describe('the default fetch', () => {
  test('calls the global fetch without binding it to another object', async () => {
    const original = globalThis.fetch;
    let receiver: unknown = 'unset';
    globalThis.fetch = async function strictFetch(this: unknown) {
      receiver = this;
      return new Response(JSON.stringify({ success: true, data: { status: 'ok' }, error: null }));
    };
    try {
      const { data } = await createApi({ getToken: () => undefined }).health();

      assert.deepEqual(data, { status: 'ok' });
      assert.ok(receiver === undefined || receiver === globalThis, 'fetch must not be called as a method');
    } finally {
      globalThis.fetch = original;
    }
  });
});

describe('withQuery', () => {
  test('skips empty values and leaves bare paths alone', () => {
    assert.equal(withQuery('/p', { a: 1, b: '', c: null, d: undefined }), '/p?a=1');
    assert.equal(withQuery('/p', {}), '/p');
    assert.equal(withQuery('/p'), '/p');
    assert.equal(withQuery('/p', null), '/p');
  });
});

describe('describeError and fieldErrors', () => {
  const error = new ApiError({ status: 400, code: 'VALIDATION_FAILED', message: 'invalid input', requestId: 'r-1', details: [{ field: 'step', message: 'must be at least 1' }, 'junk'] });

  test('describes API errors with code and request ID, and other errors by message', () => {
    assert.equal(describeError(error), 'invalid input (VALIDATION_FAILED). Request ID: r-1.');
    assert.equal(describeError(new ApiError({ code: 'NETWORK', message: 'down' })), 'down (NETWORK).');
    assert.equal(describeError(new Error('plain')), 'plain');
    assert.equal(describeError('text'), 'text');
  });

  test('lists field errors, ignoring malformed details', () => {
    assert.deepEqual(fieldErrors(error), ['step: must be at least 1']);
    assert.deepEqual(fieldErrors(new Error('x')), []);
    assert.deepEqual(fieldErrors(new ApiError({ code: 'X', message: 'y', details: { field: 'a' } })), []);
  });
});
