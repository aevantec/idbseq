// The API's data shapes (see api/openapi.json). 64-bit values are `Id`: a
// Number when exact, else a BigInt (see json.ts).

export type Id = number | bigint;
export type Scope = 'read' | 'generate' | 'manage';
export type Durability = { mode: 'strict' } | { mode: 'block'; size: number };

export const AUDIT_ACTIONS = [
  'collection.create',
  'collection.update',
  'collection.reset',
  'collection.delete',
  'key.create',
  'key.revoke',
  'auth.login',
  'auth.login_failed',
  'admin.password_change',
  'admin.init',
] as const;
export type AuditAction = (typeof AUDIT_ACTIONS)[number];

export interface Collection {
  name: string;
  start: Id;
  step: Id;
  max: Id;
  protected: boolean;
  durability: Durability;
  created_at: string;
  last_id: Id | null;
  next_id: Id | null;
}

export interface CollectionSettings {
  name?: string;
  start?: Id;
  step?: Id;
  max?: Id;
  protected?: boolean;
  durability?: Durability;
}

export interface Batch {
  first: Id;
  last: Id;
  count: number;
  step: Id;
}

export interface ResetResult {
  previous_last_id: Id | null;
  last_id: Id | null;
  next_id: Id | null;
}

export interface ApiKey {
  id: string;
  name: string;
  scopes: Scope[];
  collections: string[];
  created_at: string;
  last_used_at: string | null;
}

export interface CreatedKey extends ApiKey {
  key: string;
}

export interface AuditEntry {
  id: number;
  at: string;
  actor: string;
  action: AuditAction;
  collection: string | null;
  previous?: unknown;
  new?: unknown;
  request_id: string | null;
  client_ip: string | null;
}

/** A refreshed session's new expiry; the token is unchanged. */
export interface SessionExpiry {
  expires_at: string;
}

export interface SessionDto {
  token: string;
  expires_at: string;
}

export interface Meta {
  total: number;
  limit: number;
  next_cursor?: string | null;
}

export interface Page<T> {
  data: T;
  meta: Meta | null;
}
