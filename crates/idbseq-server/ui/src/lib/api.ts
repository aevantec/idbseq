// The console's client for the same-origin idbseq API. The session token is
// supplied by the caller and kept only in memory.

import { parseJson, stringifyJson } from './json';
import type {
  ApiKey,
  AuditAction,
  AuditEntry,
  Batch,
  Collection,
  CollectionSettings,
  CreatedKey,
  Id,
  Page,
  ResetResult,
  Scope,
  SessionDto,
  SessionExpiry,
} from './types';

export type ErrorCode = string;

/** A failed call: the server's error code and message, or NETWORK. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: ErrorCode;
  readonly details: unknown;
  readonly requestId: string | null;

  constructor(init: { status?: number; code: ErrorCode; message: string; details?: unknown; requestId?: string | null }) {
    super(init.message);
    this.name = 'ApiError';
    this.status = init.status ?? 0;
    this.code = init.code;
    this.details = init.details ?? null;
    this.requestId = init.requestId ?? null;
  }
}

type Query = Record<string, string | number | bigint | boolean | null | undefined>;
interface RequestOptions {
  body?: unknown;
  query?: Query;
  raw?: boolean;
  /** A 401 here means a wrong password, not an ended session. */
  isPasswordCheck?: boolean;
}
interface Context {
  fetchImpl: typeof fetch;
  getToken: () => string | null | undefined;
  onUnauthorized: () => void;
}

export interface ApiOptions {
  getToken: () => string | null | undefined;
  /** Called when the current token is rejected (expired or revoked). */
  onUnauthorized?: () => void;
  fetchImpl?: typeof fetch;
}

export function createApi({ getToken, onUnauthorized = () => {}, fetchImpl = defaultFetch }: ApiOptions) {
  const context: Context = { fetchImpl, getToken, onUnauthorized };
  const call = <T>(method: string, path: string, options?: RequestOptions) =>
    request<T>(context, method, path, options);
  const collection = (name: string) => `/v1/collections/${encodeURIComponent(name)}`;

  return {
    login: (password: string) =>
      call<SessionDto>('POST', '/v1/auth/login', { body: { username: 'admin', password } }),
    /** First-run setup: create the admin with the token from idbseqd's link, and sign in. */
    setup: (token: string, password: string) =>
      call<SessionDto>('POST', '/v1/auth/setup', { body: { token, password } }),
    logout: () => call<null>('POST', '/v1/auth/logout'),
    /** Keep the admin session alive; the token stays the same. */
    refresh: () => call<SessionExpiry>('POST', '/v1/auth/refresh'),
    /** Revokes every admin session, the caller's included, when it succeeds. */
    changePassword: (currentPassword: string, newPassword: string) =>
      call<null>('POST', '/v1/admin/password', {
        body: { current_password: currentPassword, new_password: newPassword },
        isPasswordCheck: true,
      }),
    /** `mode` is `console` under `idbseqd console <file>` (absent before 0.2). */
    health: () => call<{ status: string; mode?: 'server' | 'console' }>('GET', '/healthz'),
    ready: () => call<{ status: string }>('GET', '/readyz'),
    contract: () => call<{ info?: { version?: string } }>('GET', '/v1/openapi.json', { raw: true }),
    listCollections: (query: { limit?: number; prefix?: string; cursor?: string | null }) =>
      call<Collection[]>('GET', '/v1/collections', { query }),
    getCollection: (name: string) => call<Collection>('GET', collection(name)),
    createCollection: (body: CollectionSettings) => call<Collection>('POST', '/v1/collections', { body }),
    updateCollection: (name: string, body: CollectionSettings) =>
      call<Collection>('PATCH', collection(name), { body }),
    deleteCollection: (name: string, confirm: string) =>
      call<null>('DELETE', collection(name), { query: { confirm } }),
    next: (name: string, count: Id) => call<Batch>('POST', `${collection(name)}/next`, { query: { count } }),
    reset: (name: string, body: { value: Id | null; allow_rewind: boolean; confirm: string }) =>
      call<ResetResult>('POST', `${collection(name)}/reset`, { body }),
    listKeys: () => call<ApiKey[]>('GET', '/v1/keys'),
    createKey: (body: { name: string; scopes: Scope[]; collections: string[] }) =>
      call<CreatedKey>('POST', '/v1/keys', { body }),
    revokeKey: (id: string) => call<null>('DELETE', `/v1/keys/${encodeURIComponent(id)}`),
    listAudit: (query: { limit?: number; cursor?: string | null; collection?: string; action?: AuditAction | '' }) =>
      call<AuditEntry[]>('GET', '/v1/audit', { query }),
  };
}

export type Api = ReturnType<typeof createApi>;

// Browsers reject `fetch` called with any `this` but the window, so never
// call the global as a method of another object.
function defaultFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  return globalThis.fetch(input, init);
}

async function request<T>(context: Context, method: string, path: string, options: RequestOptions = {}): Promise<Page<T>> {
  const { body, query, raw = false, isPasswordCheck = false } = options;
  const token = context.getToken();
  const headers: Record<string, string> = { accept: 'application/json' };
  if (token) headers.authorization = `Bearer ${token}`;
  if (body !== undefined) headers['content-type'] = 'application/json';

  let response: Response;
  try {
    response = await context.fetchImpl(withQuery(path, query), {
      method,
      headers,
      body: body === undefined ? undefined : stringifyJson(body),
      credentials: 'omit',
      cache: 'no-store',
    });
  } catch (cause) {
    const reason = cause instanceof Error ? cause.message : String(cause);
    throw new ApiError({ code: 'NETWORK', message: `Cannot reach the server: ${reason}` });
  }

  const payload = (await readPayload(response)) as Envelope | null;
  if (raw && response.ok) return { data: payload as T, meta: null };
  if (response.ok && payload?.success === true) {
    return { data: payload.data as T, meta: payload.meta ?? null };
  }
  // Only the token in use right now ends the session: a late 401 for a token
  // from an earlier session must not sign out the current one.
  if (response.status === 401 && !isPasswordCheck && token && token === context.getToken()) context.onUnauthorized();
  throw toError(response, payload);
}

interface Envelope {
  success?: boolean;
  data?: unknown;
  meta?: Page<unknown>['meta'];
  error?: { code?: string; message?: string; details?: unknown } | null;
}

async function readPayload(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return null;
  try {
    return parseJson(text);
  } catch {
    return null;
  }
}

function toError(response: Response, payload: Envelope | null): ApiError {
  const error = payload?.error;
  return new ApiError({
    status: response.status,
    code: error?.code ?? 'INTERNAL',
    message: error?.message ?? `The server answered ${response.status} without details.`,
    details: error?.details ?? null,
    requestId: response.headers.get('x-request-id'),
  });
}

/** `/path` plus the defined, non-empty query parameters. */
export function withQuery(path: string, query: Query | null = {}): string {
  const pairs = Object.entries(query ?? {}).filter(
    ([, value]) => value !== undefined && value !== null && value !== '',
  );
  if (pairs.length === 0) return path;
  const search = new URLSearchParams(pairs.map(([key, value]) => [key, String(value)]));
  return `${path}?${search}`;
}

/** "message (CODE)" plus the request ID, for showing a failed call. */
export function describeError(error: unknown): string {
  if (!(error instanceof ApiError)) return error instanceof Error ? error.message : String(error);
  const request = error.requestId ? ` Request ID: ${error.requestId}.` : '';
  return `${error.message} (${error.code}).${request}`;
}

/** The validation details as "field: message" lines, when the server sent them. */
export function fieldErrors(error: unknown): string[] {
  if (!(error instanceof ApiError) || !Array.isArray(error.details)) return [];
  return error.details
    .filter((item): item is { field: string; message: string } =>
      typeof item === 'object' && item !== null && 'field' in item && 'message' in item)
    .map(({ field, message }) => `${field}: ${message}`);
}
