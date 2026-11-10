// End-to-end flow against a running idbseqd (US-0031). Skipped unless
// IDBSEQ_E2E_URL and IDBSEQ_E2E_ADMIN_PASSWORD are set.
import { randomBytes } from 'node:crypto';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { IdbseqClient, ProtectedCollectionError, type CreatedApiKey } from '../../src/index.js';

const url = process.env.IDBSEQ_E2E_URL ?? '';
const adminPassword = process.env.IDBSEQ_E2E_ADMIN_PASSWORD ?? '';
const enabled = url !== '' && adminPassword !== '';

const runId = `${Date.now().toString(36)}-${randomBytes(4).toString('hex')}`;
const collectionName = `e2e-node-${runId}`;

describe.skipIf(!enabled)('critical flow against a real server', () => {
  let admin: IdbseqClient;
  let apiKey: CreatedApiKey | undefined;
  let keyClient: IdbseqClient;

  beforeAll(async () => {
    admin = new IdbseqClient({ url });
    await admin.login('admin', adminPassword);
    apiKey = await admin.createKey({
      name: `e2e-node-${runId}`,
      scopes: ['read', 'generate', 'manage'],
    });
    keyClient = new IdbseqClient({ url, apiKey: apiKey.key });
  });

  afterAll(async () => {
    const cleanups = [
      admin.deleteCollection(collectionName),
      apiKey === undefined ? Promise.resolve() : admin.revokeKey(apiKey.id),
    ];
    await Promise.allSettled(cleanups);
    await admin.logout();
  });

  it('admin creates a protected collection', async () => {
    const created = await admin.createCollection({ name: collectionName, protected: true });

    expect(created).toMatchObject({ name: collectionName, protected: true, lastId: null, nextId: 1 });
  });

  it('the key generates a single ID', async () => {
    await expect(keyClient.generateId(collectionName)).resolves.toBe(1);
  });

  it('the key generates a batch', async () => {
    await expect(keyClient.generateIds(collectionName, 5)).resolves.toEqual({
      first: 2,
      last: 6,
      count: 5,
      step: 1,
    });
  });

  it('the key reads the last ID', async () => {
    await expect(keyClient.lastId(collectionName)).resolves.toEqual({ lastId: 6, nextId: 7 });
  });

  it('the key may not reset the protected collection', async () => {
    await expect(
      keyClient.resetCollection(collectionName, { allowRewind: true }),
    ).rejects.toBeInstanceOf(ProtectedCollectionError);
  });

  it('the admin resets the protected collection', async () => {
    const result = await admin.resetCollection(collectionName, { allowRewind: true });

    expect(result).toEqual({ previousLastId: 6, lastId: null, nextId: 1 });
    await expect(keyClient.lastId(collectionName)).resolves.toEqual({ lastId: null, nextId: 1 });
  });
});
