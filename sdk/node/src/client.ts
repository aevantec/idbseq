import { randomUUID } from 'node:crypto';
import * as decode from './decode.js';
import { UnauthorizedError, UnexpectedResponseError } from './errors.js';
import { assertCount, collectionBody, compact } from './requests.js';
import { resolveRetryPolicy, withRetry, type RetryOptions, type RetryPolicy } from './retry.js';
import { send, type ApiRequest, type ApiResponse, type TransportConfig } from './transport.js';
import type {
  ApiKey,
  Collection,
  CollectionPage,
  CollectionPatch,
  CreateCollectionSpec,
  CreatedApiKey,
  CreateKeySpec,
  GenerateOptions,
  Id,
  IdBatch,
  LastId,
  ListCollectionsOptions,
  ResetOptions,
  ResetResult,
  Session,
} from './types.js';

export const DEFAULT_URL = 'http://127.0.0.1:7878';
export const DEFAULT_TIMEOUT_MS = 10_000;
const USER_AGENT = 'idbseq-node/0.1.0';

export interface ClientOptions {
  /** Server base URL. Default `http://127.0.0.1:7878`. */
  readonly url?: string;
  /** API key `idb_<id>_<secret>` (or any bearer token). */
  readonly apiKey?: string;
  /** Per-request timeout. Default 10 s. */
  readonly timeoutMs?: number;
  /** Retry policy for `generateId` / `generateIds`; `false` disables retries. */
  readonly retry?: RetryOptions | false;
  /** Custom fetch implementation (proxies, tests). Default `globalThis.fetch`. */
  readonly fetch?: typeof fetch;
}

function parseBaseUrl(raw: string): URL {
  let url: URL;
  try {
    url = new URL(raw);
  } catch (cause) {
    throw new TypeError(`invalid idbseq URL: ${raw}`, { cause });
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new TypeError(`idbseq URL must use http or https: ${raw}`);
  }
  return url.pathname.endsWith('/') ? url : new URL(`${url.href}/`);
}

function resolveTimeout(timeoutMs: number): number {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    throw new RangeError('timeoutMs must be a finite number > 0');
  }
  return timeoutMs;
}

function collectionPath(name: string, suffix = ''): string {
  if (typeof name !== 'string' || name.length === 0) {
    throw new TypeError('collection name must be a non-empty string');
  }
  return `v1/collections/${encodeURIComponent(name)}${suffix}`;
}

/** Client for the idbseq HTTP API v1. */
export class IdbSeqClient {
  readonly #transport: TransportConfig;
  readonly #retry: RetryPolicy;
  readonly #apiKey: string | null;
  #sessionToken: string | null = null;

  constructor(options: ClientOptions = {}) {
    this.#transport = {
      baseUrl: parseBaseUrl(options.url ?? DEFAULT_URL),
      timeoutMs: resolveTimeout(options.timeoutMs ?? DEFAULT_TIMEOUT_MS),
      fetch: options.fetch ?? globalThis.fetch,
      userAgent: USER_AGENT,
    };
    this.#retry = resolveRetryPolicy(options.retry);
    this.#apiKey = options.apiKey ?? null;
  }

  // ── IDs ────────────────────────────────────────────────────────────────

  /** Consumes and returns the next ID. Retried safely with an idempotency key. */
  async generateId(name: string, options: GenerateOptions = {}): Promise<Id> {
    const request = { method: 'POST', path: collectionPath(name, '/next') } as const;
    return this.#generate(request, options, decode.toGeneratedId);
  }

  /** Consumes `count` IDs at once; the whole batch is issued or nothing is. */
  async generateIds(name: string, count: number, options: GenerateOptions = {}): Promise<IdBatch> {
    assertCount(count);
    const request = { method: 'POST', path: collectionPath(name, '/next'), query: { count } } as const;
    return this.#generate(request, options, decode.toBatch);
  }

  /** Reads the last issued ID and the next one without consuming anything. */
  async lastId(name: string): Promise<LastId> {
    return this.#call({ method: 'GET', path: collectionPath(name, '/last') }, decode.toLastId);
  }

  // ── Collections ────────────────────────────────────────────────────────

  async listCollections(options: ListCollectionsOptions = {}): Promise<CollectionPage> {
    const response = await this.#request({
      method: 'GET',
      path: 'v1/collections',
      query: { limit: options.limit, cursor: options.cursor, prefix: options.prefix },
    });
    return this.#decode(response, ({ data, meta }) => {
      const items = decode.toCollections(data);
      return { items, ...decode.toPageMeta(meta, items.length) };
    });
  }

  async getCollection(name: string): Promise<Collection> {
    return this.#call({ method: 'GET', path: collectionPath(name) }, decode.toCollection);
  }

  async createCollection(spec: CreateCollectionSpec): Promise<Collection> {
    const body = collectionBody(spec);
    return this.#call({ method: 'POST', path: 'v1/collections', body }, decode.toCollection);
  }

  async updateCollection(name: string, patch: CollectionPatch): Promise<Collection> {
    const request = { method: 'PATCH', path: collectionPath(name), body: collectionBody(patch) } as const;
    return this.#call(request, decode.toCollection);
  }

  /** Resets the counter; sends `confirm: name` for you. */
  async resetCollection(name: string, options: ResetOptions = {}): Promise<ResetResult> {
    const body = compact({ confirm: name, value: options.value, allow_rewind: options.allowRewind });
    return this.#call({ method: 'POST', path: collectionPath(name, '/reset'), body }, decode.toResetResult);
  }

  /** Deletes the collection; sends `confirm` for you. Returns it as it was. */
  async deleteCollection(name: string): Promise<Collection> {
    const request = { method: 'DELETE', path: collectionPath(name), query: { confirm: name } } as const;
    return this.#call(request, decode.toCollection);
  }

  // ── Admin ──────────────────────────────────────────────────────────────

  /** Logs in as the admin; later calls use the session token instead of the API key. */
  async login(username: string, password: string): Promise<Session> {
    const request = { method: 'POST', path: 'v1/auth/login', body: { username, password } } as const;
    const response = await send(this.#transport, request, null);
    const session = this.#decode(response, ({ data }) => decode.toSession(data));
    this.#sessionToken = session.token;
    return session;
  }

  /**
   * Revokes the admin session, if any, and falls back to the API key. The
   * session is dropped even if the request fails; an already expired session
   * (401) counts as signed out.
   */
  async logout(): Promise<void> {
    if (this.#sessionToken === null) {
      return;
    }
    try {
      await this.#request({ method: 'POST', path: 'v1/auth/logout' });
    } catch (error) {
      if (!(error instanceof UnauthorizedError)) {
        throw error;
      }
    } finally {
      this.#sessionToken = null;
    }
  }

  async createKey(spec: CreateKeySpec): Promise<CreatedApiKey> {
    const body = compact({ name: spec.name, scopes: spec.scopes, collections: spec.collections });
    return this.#call({ method: 'POST', path: 'v1/keys', body }, decode.toCreatedApiKey);
  }

  async listKeys(): Promise<readonly ApiKey[]> {
    return this.#call({ method: 'GET', path: 'v1/keys' }, decode.toApiKeys);
  }

  async revokeKey(id: string): Promise<void> {
    if (typeof id !== 'string' || id.length === 0) {
      throw new TypeError('key id must be a non-empty string');
    }
    await this.#request({ method: 'DELETE', path: `v1/keys/${encodeURIComponent(id)}` });
  }

  // ── Plumbing ───────────────────────────────────────────────────────────

  #request(request: ApiRequest): Promise<ApiResponse> {
    return send(this.#transport, request, this.#sessionToken ?? this.#apiKey);
  }

  async #call<T>(request: ApiRequest, decoder: (data: unknown) => T): Promise<T> {
    const response = await this.#request(request);
    return this.#decode(response, ({ data }) => decoder(data));
  }

  async #generate<T>(request: ApiRequest, options: GenerateOptions, decoder: (data: unknown) => T): Promise<T> {
    const idempotencyKey = options.idempotencyKey ?? randomUUID();
    const withKey = { ...request, headers: { 'idempotency-key': idempotencyKey } };
    const response = await withRetry(this.#retry, () => this.#request(withKey));
    return this.#decode(response, ({ data }) => decoder(data));
  }

  #decode<T>(response: ApiResponse, decoder: (response: ApiResponse) => T): T {
    try {
      return decoder(response);
    } catch (cause) {
      const detail = cause instanceof Error ? cause.message : String(cause);
      throw new UnexpectedResponseError({
        message: `unexpected response data: ${detail}`,
        status: response.status,
        requestId: response.requestId,
        cause,
      });
    }
  }
}
