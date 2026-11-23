import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { vi, type Mock } from 'vitest';
import { createIdbSeqServer } from '../../src/server.js';
import type { IdbSeqApi } from '../../src/tools.js';

export type FakeApi = { readonly [K in keyof IdbSeqApi]: Mock<IdbSeqApi[K]> };

export function fakeApi(): FakeApi {
  return {
    generateId: vi.fn<IdbSeqApi['generateId']>(),
    generateIds: vi.fn<IdbSeqApi['generateIds']>(),
    listCollections: vi.fn<IdbSeqApi['listCollections']>(),
    getCollection: vi.fn<IdbSeqApi['getCollection']>(),
    createCollection: vi.fn<IdbSeqApi['createCollection']>(),
  };
}

export interface Connected {
  readonly client: Client;
  readonly call: (name: string, args?: Record<string, unknown>) => Promise<CallToolResult>;
  readonly close: () => Promise<void>;
}

/** Connects a real MCP client to the server over an in-memory transport. */
export async function connect(api: IdbSeqApi, allowCreate = false): Promise<Connected> {
  const server = createIdbSeqServer(api, { allowCreate });
  const client = new Client({ name: 'test', version: '0.0.0' });
  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair();
  await Promise.all([server.connect(serverSide), client.connect(clientSide)]);
  const call = async (name: string, args: Record<string, unknown> = {}) =>
    (await client.callTool({ name, arguments: args })) as CallToolResult;
  const close = async () => {
    await client.close();
    await server.close();
  };
  return { client, call, close };
}

/** The text of a single-text tool result. */
export function textOf(result: CallToolResult): string {
  const [first] = result.content;
  if (first?.type !== 'text') {
    throw new Error('expected a text result');
  }
  return first.text;
}
