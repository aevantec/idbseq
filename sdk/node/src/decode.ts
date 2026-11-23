// Turns snake_case response data into SDK objects, checking each field's type.
// A mismatch throws a TypeError naming the field; the client wraps it in an
// UnexpectedResponseError.
import type {
  ApiKey,
  Collection,
  CreatedApiKey,
  Durability,
  Id,
  IdBatch,
  LastId,
  ResetResult,
  Scope,
  Session,
} from './types.js';

type Fields = Readonly<Record<string, unknown>>;

export function record(value: unknown, path: string): Fields {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`expected an object at ${path}`);
  }
  return value as Fields;
}

function list(value: unknown, path: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw new TypeError(`expected an array at ${path}`);
  }
  return value;
}

export function str(value: unknown, path: string): string {
  if (typeof value !== 'string') {
    throw new TypeError(`expected a string at ${path}`);
  }
  return value;
}

function nullableStr(value: unknown, path: string): string | null {
  return value === null || value === undefined ? null : str(value, path);
}

function bool(value: unknown, path: string): boolean {
  if (typeof value !== 'boolean') {
    throw new TypeError(`expected a boolean at ${path}`);
  }
  return value;
}

function int(value: unknown, path: string): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    throw new TypeError(`expected an integer at ${path}`);
  }
  return value;
}

export function id(value: unknown, path: string): Id {
  if (typeof value === 'bigint' || (typeof value === 'number' && Number.isSafeInteger(value))) {
    return value;
  }
  throw new TypeError(`expected an integer ID at ${path}`);
}

function nullableId(value: unknown, path: string): Id | null {
  return value === null || value === undefined ? null : id(value, path);
}

function durability(value: unknown, path: string): Durability {
  const fields = record(value, path);
  const mode = str(fields.mode, `${path}.mode`);
  if (mode === 'block') {
    return { mode, size: int(fields.size, `${path}.size`) };
  }
  return { mode } as Durability;
}

export function toCollection(value: unknown, path = 'data'): Collection {
  const f = record(value, path);
  return {
    name: str(f.name, `${path}.name`),
    start: id(f.start, `${path}.start`),
    step: id(f.step, `${path}.step`),
    max: id(f.max, `${path}.max`),
    protected: bool(f.protected, `${path}.protected`),
    durability: durability(f.durability, `${path}.durability`),
    createdAt: str(f.created_at, `${path}.created_at`),
    lastId: nullableId(f.last_id, `${path}.last_id`),
    nextId: nullableId(f.next_id, `${path}.next_id`),
  };
}

export function toCollections(value: unknown): readonly Collection[] {
  return list(value, 'data').map((item, index) => toCollection(item, `data[${index}]`));
}

export function toGeneratedId(value: unknown): Id {
  return id(record(value, 'data').id, 'data.id');
}

export function toBatch(value: unknown): IdBatch {
  const f = record(value, 'data');
  return {
    first: id(f.first, 'data.first'),
    last: id(f.last, 'data.last'),
    count: int(f.count, 'data.count'),
    step: id(f.step, 'data.step'),
  };
}

export function toLastId(value: unknown): LastId {
  const f = record(value, 'data');
  return {
    lastId: nullableId(f.last_id, 'data.last_id'),
    nextId: nullableId(f.next_id, 'data.next_id'),
  };
}

export function toResetResult(value: unknown): ResetResult {
  const f = record(value, 'data');
  return {
    previousLastId: nullableId(f.previous_last_id, 'data.previous_last_id'),
    lastId: nullableId(f.last_id, 'data.last_id'),
    nextId: nullableId(f.next_id, 'data.next_id'),
  };
}

export interface PageMeta {
  readonly total: number;
  readonly limit: number;
  readonly nextCursor: string | null;
}

/** Reads `meta`; a missing `meta` means a single, complete page. */
export function toPageMeta(value: unknown, itemCount: number): PageMeta {
  if (value === null || value === undefined) {
    return { total: itemCount, limit: itemCount, nextCursor: null };
  }
  const f = record(value, 'meta');
  return {
    total: int(f.total, 'meta.total'),
    limit: int(f.limit, 'meta.limit'),
    nextCursor: nullableStr(f.next_cursor, 'meta.next_cursor'),
  };
}

function strings(value: unknown, path: string): readonly string[] {
  if (value === null || value === undefined) {
    return [];
  }
  return list(value, path).map((item, index) => str(item, `${path}[${index}]`));
}

export function toApiKey(value: unknown, path = 'data'): ApiKey {
  const f = record(value, path);
  return {
    id: str(f.id, `${path}.id`),
    name: str(f.name, `${path}.name`),
    scopes: strings(f.scopes, `${path}.scopes`) as readonly Scope[],
    collections: strings(f.collections, `${path}.collections`),
    createdAt: str(f.created_at, `${path}.created_at`),
    lastUsedAt: nullableStr(f.last_used_at, `${path}.last_used_at`),
  };
}

export function toApiKeys(value: unknown): readonly ApiKey[] {
  return list(value, 'data').map((item, index) => toApiKey(item, `data[${index}]`));
}

export function toCreatedApiKey(value: unknown): CreatedApiKey {
  return { ...toApiKey(value), key: str(record(value, 'data').key, 'data.key') };
}

export function toSession(value: unknown): Session {
  const f = record(value, 'data');
  return { token: str(f.token, 'data.token'), expiresAt: str(f.expires_at, 'data.expires_at') };
}
