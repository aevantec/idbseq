// End-to-end: the built `idbseq-mcp` binary over stdio against a running idbseqd.
// Skipped unless IDBSEQ_E2E_URL and IDBSEQ_E2E_ADMIN_PASSWORD are set (scripts/e2e.sh mcp).
import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { IdbSeqClient, type CreatedApiKey } from 'idbseq';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const url = process.env.IDBSEQ_E2E_URL ?? '';
const adminPassword = process.env.IDBSEQ_E2E_ADMIN_PASSWORD ?? '';
const enabled = url !== '' && adminPassword !== '';

const CLI = fileURLToPath(new URL('../../dist/cli.js', import.meta.url));
const runId = `${Date.now().toString(36)}-${randomBytes(4).toString('hex')}`;
const allowed = `e2e-mcp-${runId}`;
const other = `e2e-mcp-other-${runId}`;

function textOf(result: CallToolResult): unknown {
  const [first] = result.content;
  return first?.type === 'text' ? JSON.parse(first.text) : undefined;
}

describe.skipIf(!enabled)('idbseq-mcp over stdio against a real server', () => {
  let admin: IdbSeqClient;
  let key: CreatedApiKey | undefined;
  let mcp: Client;

  const call = async (name: string, args: Record<string, unknown> = {}) =>
    (await mcp.callTool({ name, arguments: args })) as CallToolResult;

  beforeAll(async () => {
    admin = new IdbSeqClient({ url });
    await admin.login('admin', adminPassword);
    await admin.createCollection({ name: allowed, start: 100 });
    await admin.createCollection({ name: other });
    key = await admin.createKey({ name: `e2e-mcp-${runId}`, scopes: ['read', 'generate'], collections: [allowed] });
    mcp = new Client({ name: 'e2e', version: '0.0.0' });
    const env = { PATH: process.env.PATH ?? '', IDBSEQ_URL: url, IDBSEQ_KEY: key.key };
    await mcp.connect(new StdioClientTransport({ command: process.execPath, args: [CLI], env, stderr: 'pipe' }));
  });

  afterAll(async () => {
    await mcp?.close();
    await Promise.allSettled([
      admin.deleteCollection(allowed),
      admin.deleteCollection(other),
      key === undefined ? Promise.resolve() : admin.revokeKey(key.id),
    ]);
    await admin.logout();
  });

  it('reserves one ID, then a batch', async () => {
    expect(textOf(await call('next_id', { collection: allowed }))).toEqual({ id: 100 });
    expect(textOf(await call('next_id', { collection: allowed, count: 3 }))).toEqual({
      first: 101,
      last: 103,
      count: 3,
      step: 1,
    });
  });

  it('returns the same IDs for a retried idempotency key', async () => {
    const args = { collection: allowed, count: 2, idempotency_key: `retry-${runId}` };

    const first = textOf(await call('next_id', args));
    const retry = textOf(await call('next_id', args));

    expect(first).toEqual({ first: 104, last: 105, count: 2, step: 1 });
    expect(retry).toEqual(first);
  });

  it('shows the collection and lists only allowed ones', async () => {
    expect(textOf(await call('get_collection', { collection: allowed }))).toMatchObject({
      name: allowed,
      last_id: 105,
      next_id: 106,
    });
    const page = textOf(await call('list_collections', { prefix: 'e2e-mcp' })) as { collections: { name: string }[] };
    expect(page.collections.map((collection) => collection.name)).toEqual([allowed]);
  });

  it('returns FORBIDDEN as a tool error outside the allow-list', async () => {
    const result = await call('next_id', { collection: other });

    expect(result.isError).toBe(true);
    expect(textOf(result)).toMatchObject({ code: 'FORBIDDEN' });
  });

  it('refuses to start with a revoked key', async () => {
    const revoked = await admin.createKey({ name: `e2e-mcp-revoked-${runId}`, scopes: ['generate'] });
    await admin.revokeKey(revoked.id);

    const run = promisify(execFile)(process.execPath, [CLI], {
      env: { IDBSEQ_URL: url, IDBSEQ_KEY: revoked.key },
    });

    await expect(run).rejects.toMatchObject({ code: 1, stderr: expect.stringMatching(/rejected IDBSEQ_KEY/) });
  });
});
