import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { IdbSeqError, stringifyJson, type Collection, type CollectionPage } from 'idbseq';

// Tool results use the wire names of the HTTP API (snake_case), so what an agent
// reads matches docs/API.md. IDs above 2^53 are bigints; stringifyJson writes them
// as exact JSON numbers.

/** A successful tool result carrying `value` as JSON text. */
export function jsonResult(value: unknown): CallToolResult {
  return { content: [{ type: 'text', text: stringifyJson(value) }] };
}

/** A tool error the agent can read and act on. */
export function errorResult(error: unknown): CallToolResult {
  return { isError: true, content: [{ type: 'text', text: describeError(error) }] };
}

function describeError(error: unknown): string {
  if (error instanceof IdbSeqError) {
    const body = { code: error.code, message: error.message, details: error.details ?? null };
    return stringifyJson(body);
  }
  const message = error instanceof Error ? error.message : String(error);
  return stringifyJson({ code: 'CLIENT_ERROR', message, details: null });
}

export function collectionView(collection: Collection): Readonly<Record<string, unknown>> {
  return {
    name: collection.name,
    start: collection.start,
    step: collection.step,
    max: collection.max,
    protected: collection.protected,
    durability: collection.durability,
    created_at: collection.createdAt,
    last_id: collection.lastId,
    next_id: collection.nextId,
  };
}

export function pageView(page: CollectionPage): Readonly<Record<string, unknown>> {
  return {
    collections: page.items.map(collectionView),
    total: page.total,
    next_cursor: page.nextCursor,
  };
}
