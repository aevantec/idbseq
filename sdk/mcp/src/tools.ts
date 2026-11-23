import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { Durability, Id, IdbSeqClient } from 'idbseq';
import { z } from 'zod';
import { collectionView, errorResult, jsonResult, pageView } from './results.js';

/** The part of the SDK client the tools use; tests pass a fake. */
export type IdbSeqApi = Pick<
  IdbSeqClient,
  'generateId' | 'generateIds' | 'listCollections' | 'getCollection' | 'createCollection'
>;

export interface ToolOptions {
  /** Register `create_collection` (opt-in; the key needs the `manage` scope). */
  readonly allowCreate: boolean;
}

/** Rules every agent should follow; also sent as the server's instructions. */
export const RULES =
  'IDs from idbseq are unique and increasing. Every ID handed out is used up for good: ' +
  'IDs are never reused, and gaps are normal (never try to fill or reuse one). ' +
  'Collection names are lowercase. When retrying a next_id call that may have succeeded, ' +
  'send the same idempotency_key to get the same IDs back instead of new ones. ' +
  'This server cannot reset or delete collections.';

const COLLECTION_NAME = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[a-z0-9][a-z0-9_.-]*$/, 'lowercase letters, digits, `_`, `.` and `-`, starting with a letter or digit')
  .describe('Collection name, e.g. `orders`. Lowercase.');

const IDEMPOTENCY_KEY = z
  .string()
  .regex(/^[\x21-\x7e]{1,255}$/, '1-255 visible ASCII characters')
  .describe('Optional. Reuse the same value when retrying to get the same IDs back (remembered 24 hours).');

const INT64_MIN = -(2n ** 63n);
const INT64_MAX = 2n ** 63n - 1n;

// JSON numbers above 2^53 lose precision in most clients, so large values may also
// be sent as decimal strings; both become an exact Id.
const ID_VALUE = z.union([z.number().int(), z.string().regex(/^-?\d{1,19}$/, 'a whole number')]);

/** Converts a tool argument to an Id, checking the signed 64-bit range. */
export function toId(value: number | string): Id {
  if (typeof value === 'number') {
    return value;
  }
  const big = BigInt(value);
  if (big < INT64_MIN || big > INT64_MAX) {
    throw new RangeError(`${value} is outside the signed 64-bit range`);
  }
  return Number.isSafeInteger(Number(big)) ? Number(big) : big;
}

function optionalId(value: number | string | undefined): Id | undefined {
  return value === undefined ? undefined : toId(value);
}

type Handler<A> = (args: A) => Promise<unknown>;

type Defined<T> = { [K in keyof T]?: Exclude<T[K], undefined> };

/** Drops fields whose value is `undefined`, so optional arguments are not sent. */
function withoutUndefined<T extends object>(fields: T): Defined<T> {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined)) as Defined<T>;
}

/** Turns a handler into a tool callback that never throws: failures become tool errors. */
function safely<A>(handler: Handler<A>): (args: A) => Promise<CallToolResult> {
  return async (args) => {
    try {
      return jsonResult(await handler(args));
    } catch (error) {
      return errorResult(error);
    }
  };
}

function registerNextId(server: McpServer, api: IdbSeqApi): void {
  server.registerTool(
    'next_id',
    {
      title: 'Reserve the next ID',
      description:
        'Reserve the next unique ID from an idbseq collection, or `count` consecutive IDs at once. ' +
        'Returns `{ "id": n }` without count, `{ "first", "last", "count", "step" }` with count. ' +
        'The IDs are yours: no other caller will ever get them, even ones you end up not using. ' +
        RULES,
      inputSchema: {
        collection: COLLECTION_NAME,
        count: z.number().int().min(1).optional().describe('Reserve this many IDs in one call.'),
        idempotency_key: IDEMPOTENCY_KEY.optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    safely(async ({ collection, count, idempotency_key: idempotencyKey }) => {
      const options = idempotencyKey === undefined ? {} : { idempotencyKey };
      if (count === undefined) {
        return { id: await api.generateId(collection, options) };
      }
      return api.generateIds(collection, count, options);
    }),
  );
}

function registerListCollections(server: McpServer, api: IdbSeqApi): void {
  server.registerTool(
    'list_collections',
    {
      title: 'List collections',
      description:
        'List the idbseq collections this key may use, sorted by name, with each one\'s `last_id` and `next_id`. ' +
        'Pass `next_cursor` back as `cursor` for the next page; it is null on the last page.',
      inputSchema: {
        prefix: z.string().optional().describe('Only names starting with this.'),
        limit: z.number().int().min(1).max(500).optional().describe('Page size, 1-500, default 50.'),
        cursor: z.string().optional().describe('`next_cursor` from the previous page.'),
      },
      annotations: { readOnlyHint: true, openWorldHint: false },
    },
    safely(async ({ prefix, limit, cursor }) => {
      return pageView(await api.listCollections(withoutUndefined({ prefix, limit, cursor })));
    }),
  );
}

function registerGetCollection(server: McpServer, api: IdbSeqApi): void {
  server.registerTool(
    'get_collection',
    {
      title: 'Show a collection',
      description:
        'Show one idbseq collection: its settings, the last issued ID (`last_id`) and the next one ' +
        '(`next_id`). Reading does not reserve anything: another caller may take `next_id` first, so ' +
        'use next_id to get an ID you can keep.',
      inputSchema: { collection: COLLECTION_NAME },
      annotations: { readOnlyHint: true, openWorldHint: false },
    },
    safely(async ({ collection }) => collectionView(await api.getCollection(collection))),
  );
}

function durabilityFor(blockSize: number | undefined): Durability | undefined {
  return blockSize === undefined ? undefined : { mode: 'block', size: blockSize };
}

function registerCreateCollection(server: McpServer, api: IdbSeqApi): void {
  server.registerTool(
    'create_collection',
    {
      title: 'Create a collection',
      description:
        'Create a new idbseq collection (an independent counter). Only do this when the user asked for ' +
        'one or clearly needs a new sequence; check list_collections first. Fails with COLLECTION_EXISTS ' +
        'if the name is taken.',
      inputSchema: {
        name: COLLECTION_NAME,
        start: ID_VALUE.optional().describe('First ID, default 1.'),
        step: ID_VALUE.optional().describe('Increment, at least 1, default 1.'),
        max: ID_VALUE.optional().describe(
          'Largest ID, default 9223372036854775807. Send values above 2^53 as a decimal string.',
        ),
        block_size: z
          .number()
          .int()
          .min(1)
          .optional()
          .describe(
            'Leave unset for the default strict durability (every ID on disk before it is returned). ' +
              'Set it only for very high throughput: a crash may then skip up to this many IDs.',
          ),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    safely(async ({ name, start, step, max, block_size: blockSize }) => {
      const settings = withoutUndefined({
        start: optionalId(start),
        step: optionalId(step),
        max: optionalId(max),
        durability: durabilityFor(blockSize),
      });
      return collectionView(await api.createCollection({ ...settings, name }));
    }),
  );
}

/** Registers the idbseq tools. Delete and reset are never exposed. */
export function registerTools(server: McpServer, api: IdbSeqApi, options: ToolOptions): void {
  registerNextId(server, api);
  registerListCollections(server, api);
  registerGetCollection(server, api);
  if (options.allowCreate) {
    registerCreateCollection(server, api);
  }
}
