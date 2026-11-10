/**
 * A sequence value. IDs are signed 64-bit integers: a `number` when it fits
 * `Number.isSafeInteger`, a `bigint` above that.
 */
export type Id = number | bigint;

export type Scope = 'read' | 'generate' | 'manage';

/** How a collection persists issued IDs. */
export type Durability = { readonly mode: 'strict' } | { readonly mode: 'block'; readonly size: number };

export interface Collection {
  readonly name: string;
  readonly start: Id;
  readonly step: Id;
  readonly max: Id;
  readonly protected: boolean;
  readonly durability: Durability;
  /** RFC 3339 timestamp in UTC. */
  readonly createdAt: string;
  /** Most recently issued ID; `null` before the first. */
  readonly lastId: Id | null;
  /** What the next call would return (not reserved); `null` when exhausted. */
  readonly nextId: Id | null;
}

export interface CollectionPage {
  readonly items: readonly Collection[];
  readonly total: number;
  readonly limit: number;
  /** Pass back as `cursor` for the next page; `null` on the last page. */
  readonly nextCursor: string | null;
}

export interface ListCollectionsOptions {
  /** 1–500, server default 50. */
  readonly limit?: number;
  readonly cursor?: string;
  readonly prefix?: string;
}

export interface CreateCollectionSpec {
  readonly name: string;
  readonly start?: Id;
  readonly step?: Id;
  readonly max?: Id;
  readonly protected?: boolean;
  readonly durability?: Durability;
}

export type CollectionPatch = Omit<CreateCollectionSpec, 'name'>;

export interface IdBatch {
  readonly first: Id;
  readonly last: Id;
  readonly count: number;
  readonly step: Id;
}

export interface LastId {
  readonly lastId: Id | null;
  readonly nextId: Id | null;
}

export interface ResetOptions {
  /** The next ID to issue; defaults to the collection's `start`. */
  readonly value?: Id;
  /** Required when the reset moves the counter backwards. */
  readonly allowRewind?: boolean;
}

export interface ResetResult {
  readonly previousLastId: Id | null;
  readonly lastId: Id | null;
  readonly nextId: Id | null;
}

export interface GenerateOptions {
  /**
   * Idempotency key sent with every attempt of this call. Defaults to a random
   * UUID per call; pass your own to make a retry across process restarts safe.
   */
  readonly idempotencyKey?: string;
}

export interface ApiKey {
  readonly id: string;
  readonly name: string;
  readonly scopes: readonly Scope[];
  /** Allow-list of names or `prefix*` patterns; empty allows every collection. */
  readonly collections: readonly string[];
  readonly createdAt: string;
  readonly lastUsedAt: string | null;
}

export interface CreatedApiKey extends ApiKey {
  /** The full key `idb_<id>_<secret>`; shown only once. */
  readonly key: string;
}

export interface CreateKeySpec {
  readonly name: string;
  readonly scopes: readonly Scope[];
  readonly collections?: readonly string[];
}

export interface Session {
  readonly token: string;
  readonly expiresAt: string;
}
