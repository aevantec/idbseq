// IdbSeqDatabase: idbseq embedded in this process, in one file, with no server.
// It has the same methods, types and errors as IdbSeqClient (minus admin), so
// code written against `Sequences` works with either.
import type { IdbSeqClient } from './client.js';
import * as decode from './decode.js';
import { createApiError, UnexpectedResponseError } from './errors.js';
import { parseJson, stringifyJson } from './json.js';
import { loadNative, type NativeDatabase } from './native.cjs';
import { assertCount, collectionBody, compact } from './requests.js';
import type {
  Collection,
  CollectionPage,
  CollectionPatch,
  CreateCollectionSpec,
  GenerateOptions,
  Id,
  IdBatch,
  LastId,
  ListCollectionsOptions,
  ResetOptions,
  ResetResult,
} from './types.js';

/** How long a call waits for a file another process holds, by default. */
export const DEFAULT_BUSY_TIMEOUT_MS = 5_000;
const MAX_BUSY_TIMEOUT_MS = 2 ** 32 - 1;

/** The ID and collection methods shared by `IdbSeqClient` and `IdbSeqDatabase`. */
export type Sequences = Pick<
  IdbSeqClient,
  | 'generateId'
  | 'generateIds'
  | 'lastId'
  | 'listCollections'
  | 'getCollection'
  | 'createCollection'
  | 'updateCollection'
  | 'resetCollection'
  | 'deleteCollection'
>;

export interface DatabaseOptions {
  /**
   * `false` (default): this handle holds the file until `close()`; fastest.
   * `true`: every call opens the file, writes and closes it, so several
   * processes can take turns on one file.
   */
  readonly shared?: boolean;
  /** How long a call waits while another process holds the file. Default 5000. */
  readonly busyTimeoutMs?: number;
}

interface Envelope {
  readonly data: unknown;
  readonly meta: unknown;
}

/** Embedded errors have no HTTP status, request ID or Retry-After. */
const NO_REQUEST = { status: 0, requestId: null, retryAfterSeconds: null } as const;

/** An idbseq file opened in this process. */
export class IdbSeqDatabase implements Sequences {
  readonly #native: NativeDatabase;
  /** Calls not finished yet: `close()` waits for them before letting go of the file. */
  readonly #running = new Set<Promise<string>>();
  #closing: Promise<void> | null = null;
  /** The file this handle uses. */
  readonly path: string;

  private constructor(path: string, native: NativeDatabase) {
    this.path = path;
    this.#native = native;
  }

  /** Opens the file at `path`, creating it if needed. Its folder must exist. */
  static async open(path: string, options: DatabaseOptions = {}): Promise<IdbSeqDatabase> {
    if (typeof path !== 'string' || path.length === 0) {
      throw new TypeError('path must be a non-empty string');
    }
    const busyTimeoutMs = options.busyTimeoutMs ?? DEFAULT_BUSY_TIMEOUT_MS;
    if (!Number.isInteger(busyTimeoutMs) || busyTimeoutMs < 0 || busyTimeoutMs > MAX_BUSY_TIMEOUT_MS) {
      throw new RangeError('busyTimeoutMs must be an integer from 0 to 4294967295');
    }
    const { NativeDatabase } = loadNative();
    const database = new IdbSeqDatabase(path, new NativeDatabase(path, options.shared ?? false, busyTimeoutMs));
    await database.#request('open', {});
    return database;
  }

  // ── IDs ────────────────────────────────────────────────────────────────

  /** Consumes and returns the next ID. It is on disk before it is returned. */
  async generateId(name: string, options: GenerateOptions = {}): Promise<Id> {
    return this.#call('next', { name, idempotency_key: options.idempotencyKey }, decode.toGeneratedId);
  }

  /** Consumes `count` IDs at once; the whole batch is issued or nothing is. */
  async generateIds(name: string, count: number, options: GenerateOptions = {}): Promise<IdBatch> {
    assertCount(count);
    return this.#call('next', { name, count, idempotency_key: options.idempotencyKey }, decode.toBatch);
  }

  /** Reads the last issued ID and the next one without consuming anything. */
  async lastId(name: string): Promise<LastId> {
    return this.#call('last', { name }, decode.toLastId);
  }

  // ── Collections ────────────────────────────────────────────────────────

  async listCollections(options: ListCollectionsOptions = {}): Promise<CollectionPage> {
    const args = { limit: options.limit, cursor: options.cursor, prefix: options.prefix };
    const { data, meta } = await this.#request('list', args);
    return this.#decode(() => {
      const items = decode.toCollections(data);
      return { items, ...decode.toPageMeta(meta, items.length) };
    });
  }

  async getCollection(name: string): Promise<Collection> {
    return this.#call('get', { name }, decode.toCollection);
  }

  async createCollection(spec: CreateCollectionSpec): Promise<Collection> {
    return this.#call('create', collectionBody(spec), decode.toCollection);
  }

  async updateCollection(name: string, patch: CollectionPatch): Promise<Collection> {
    return this.#call('update', { ...collectionBody(patch), name }, decode.toCollection);
  }

  /** Moves the counter; moving it backwards needs `allowRewind`. */
  async resetCollection(name: string, options: ResetOptions = {}): Promise<ResetResult> {
    const args = { name, value: options.value, allow_rewind: options.allowRewind };
    return this.#call('reset', args, decode.toResetResult);
  }

  /** Deletes the collection. Returns it as it was. */
  async deleteCollection(name: string): Promise<Collection> {
    return this.#call('delete', { name }, decode.toCollection);
  }

  /**
   * Waits for calls already running, then lets go of the file: once it
   * resolves, another handle or process can open the file at once. Calls made
   * after `close()` throw `DatabaseClosedError`. Closing twice is fine.
   */
  async close(): Promise<void> {
    this.#closing ??= this.#closeAfterRunningCalls();
    return this.#closing;
  }

  async #closeAfterRunningCalls(): Promise<void> {
    await Promise.allSettled([...this.#running]);
    await this.#send('close', {});
  }

  // ── Plumbing ───────────────────────────────────────────────────────────

  async #call<T>(op: string, args: Readonly<Record<string, unknown>>, decoder: (data: unknown) => T): Promise<T> {
    const { data } = await this.#request(op, args);
    return this.#decode(() => decoder(data));
  }

  /** Runs `op` in the native module; throws the error class for an error envelope. */
  async #request(op: string, args: Readonly<Record<string, unknown>>): Promise<Envelope> {
    if (this.#closing !== null) {
      throw createApiError({
        ...NO_REQUEST,
        code: 'DATABASE_CLOSED',
        message: 'the database is closed',
        details: null,
      });
    }
    return this.#send(op, args);
  }

  async #send(op: string, args: Readonly<Record<string, unknown>>): Promise<Envelope> {
    const call = this.#native.call(op, stringifyJson(compact(args)));
    this.#running.add(call);
    let text: string;
    try {
      text = await call;
    } finally {
      this.#running.delete(call);
    }
    const envelope = this.#decode(() => decode.record(parseJson(text), 'response'));
    if (envelope.error !== undefined) {
      const error = this.#decode(() => decode.record(envelope.error, 'error'));
      throw createApiError({
        ...NO_REQUEST,
        code: this.#decode(() => decode.str(error.code, 'error.code')),
        message: this.#decode(() => decode.str(error.message, 'error.message')),
        details: error.details ?? null,
      });
    }
    return { data: envelope.data, meta: envelope.meta };
  }

  #decode<T>(decoder: () => T): T {
    try {
      return decoder();
    } catch (cause) {
      const detail = cause instanceof Error ? cause.message : String(cause);
      throw new UnexpectedResponseError({
        message: `unexpected data from the embedded database: ${detail}`,
        status: 0,
        requestId: null,
        cause,
      });
    }
  }
}
