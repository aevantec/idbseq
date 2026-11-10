import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { IdbseqClient, NotFoundError, UnauthorizedError } from '../src/index.js';
import { fail, MockServer, ok } from './helpers/mock-server.js';

const WIRE_KEY = {
  id: 'k7f3a9c2',
  name: 'billing-service',
  scopes: ['read', 'generate'],
  collections: ['orders', 'eu.*'],
  created_at: '2026-11-10T08:15:30Z',
  last_used_at: null,
};

const SDK_KEY = {
  id: 'k7f3a9c2',
  name: 'billing-service',
  scopes: ['read', 'generate'],
  collections: ['orders', 'eu.*'],
  createdAt: '2026-11-10T08:15:30Z',
  lastUsedAt: null,
};

let server: MockServer;
let admin: IdbseqClient;

beforeAll(async () => {
  server = await MockServer.start();
});

afterAll(async () => {
  await server.close();
});

beforeEach(() => {
  server.reset();
  admin = new IdbseqClient({ url: server.url });
});

describe('login and logout', () => {
  it('sends credentials without an Authorization header and stores the token', async () => {
    // Arrange
    server.reply(ok({ token: 'sess-1', expires_at: '2026-11-10T08:30:30Z' }), ok({ id: 1 }));

    // Act
    const session = await admin.login('admin', 'correct horse');
    await admin.generateId('orders');

    // Assert
    expect(session).toEqual({ token: 'sess-1', expiresAt: '2026-11-10T08:30:30Z' });
    const [loginRequest, generateRequest] = server.requests;
    expect(loginRequest?.path).toBe('/v1/auth/login');
    expect(loginRequest?.headers.authorization).toBeUndefined();
    expect(loginRequest?.body).toBe('{"username":"admin","password":"correct horse"}');
    expect(generateRequest?.headers.authorization).toBe('Bearer sess-1');
  });

  it('throws UnauthorizedError on a failed login and keeps no token', async () => {
    server.reply(fail(401, 'UNAUTHORIZED', 'invalid credentials'), ok({ id: 1 }));

    await expect(admin.login('admin', 'wrong')).rejects.toBeInstanceOf(UnauthorizedError);
    await admin.generateId('orders');

    expect(server.lastRequest.headers.authorization).toBeUndefined();
  });

  it('logout revokes the session and falls back to the API key', async () => {
    // Arrange
    const client = new IdbseqClient({ url: server.url, apiKey: 'idb_k_s' });
    server.reply(ok({ token: 'sess-2', expires_at: 'x' }), ok(null), ok({ id: 1 }));
    await client.login('admin', 'pw');

    // Act
    await client.logout();
    await client.generateId('orders');

    // Assert
    expect(server.requests[1]?.path).toBe('/v1/auth/logout');
    expect(server.requests[1]?.headers.authorization).toBe('Bearer sess-2');
    expect(server.lastRequest.headers.authorization).toBe('Bearer idb_k_s');
  });

  it('logout without a session does nothing', async () => {
    await admin.logout();

    expect(server.requests).toHaveLength(0);
  });
});

describe('API keys', () => {
  beforeEach(async () => {
    server.reply(ok({ token: 'sess', expires_at: 'x' }));
    await admin.login('admin', 'pw');
    server.reset();
  });

  it('creates a key and returns the secret once', async () => {
    // Arrange
    server.reply({ ...ok({ ...WIRE_KEY, key: 'idb_k7f3a9c2_secret' }), status: 201 });

    // Act
    const created = await admin.createKey({
      name: 'billing-service',
      scopes: ['read', 'generate'],
      collections: ['orders', 'eu.*'],
    });

    // Assert
    expect(created).toEqual({ ...SDK_KEY, key: 'idb_k7f3a9c2_secret' });
    expect(server.lastRequest.path).toBe('/v1/keys');
    expect(server.lastRequest.body).toBe(
      '{"name":"billing-service","scopes":["read","generate"],"collections":["orders","eu.*"]}',
    );
  });

  it('lists keys and treats a missing allow-list as empty', async () => {
    const { collections: _omitted, ...withoutCollections } = WIRE_KEY;
    server.reply(ok([WIRE_KEY, withoutCollections]));

    const keys = await admin.listKeys();

    expect(keys).toEqual([SDK_KEY, { ...SDK_KEY, collections: [] }]);
    expect(server.lastRequest.method).toBe('GET');
  });

  it('revokes a key', async () => {
    server.reply(ok(null));

    await expect(admin.revokeKey('k7f3a9c2')).resolves.toBeUndefined();
    expect(server.lastRequest.method).toBe('DELETE');
    expect(server.lastRequest.path).toBe('/v1/keys/k7f3a9c2');
  });

  it('throws NotFoundError for an unknown key id', async () => {
    server.reply(fail(404, 'NOT_FOUND'));

    await expect(admin.revokeKey('nope')).rejects.toBeInstanceOf(NotFoundError);
  });
});
