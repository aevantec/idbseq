import {
  createApiError,
  IdbseqError,
  IdbseqNetworkError,
  UnexpectedResponseError,
} from './errors.js';
import { parseJson, stringifyJson } from './json.js';

export type HttpMethod = 'GET' | 'POST' | 'PATCH' | 'DELETE';

export interface ApiRequest {
  readonly method: HttpMethod;
  /** Path relative to the base URL, without a leading slash, e.g. `v1/collections`. */
  readonly path: string;
  readonly query?: Readonly<Record<string, string | number | undefined>>;
  readonly body?: unknown;
  readonly headers?: Readonly<Record<string, string>>;
}

export interface ApiResponse {
  readonly data: unknown;
  readonly meta: unknown;
  readonly status: number;
  readonly requestId: string | null;
}

export interface TransportConfig {
  /** Base URL ending with `/`. */
  readonly baseUrl: URL;
  readonly timeoutMs: number;
  readonly fetch: typeof fetch;
  readonly userAgent: string;
}

const MS_PER_SECOND = 1_000;
const SECONDS_ONLY = /^\d+$/;

function buildUrl(baseUrl: URL, request: ApiRequest): URL {
  const url = new URL(request.path, baseUrl);
  for (const [key, value] of Object.entries(request.query ?? {})) {
    if (value !== undefined) {
      url.searchParams.set(key, String(value));
    }
  }
  return url;
}

function buildHeaders(config: TransportConfig, request: ApiRequest, credential: string | null): Headers {
  const headers = new Headers({ accept: 'application/json', 'user-agent': config.userAgent });
  if (request.body !== undefined) {
    headers.set('content-type', 'application/json');
  }
  if (credential !== null) {
    headers.set('authorization', `Bearer ${credential}`);
  }
  for (const [name, value] of Object.entries(request.headers ?? {})) {
    headers.set(name, value);
  }
  return headers;
}

/** Parses `Retry-After` given as delta-seconds or as an HTTP date. */
export function parseRetryAfter(value: string | null, now: number = Date.now()): number | null {
  if (value === null) {
    return null;
  }
  const trimmed = value.trim();
  if (SECONDS_ONLY.test(trimmed)) {
    return Number(trimmed);
  }
  const at = Date.parse(trimmed);
  return Number.isNaN(at) ? null : Math.max(0, Math.ceil((at - now) / MS_PER_SECOND));
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function errorFromEnvelope(
  error: unknown,
  status: number,
  requestId: string | null,
  retryAfterSeconds: number | null,
): IdbseqError {
  if (!isRecord(error) || typeof error.code !== 'string') {
    return new UnexpectedResponseError({
      message: `HTTP ${status} with an error envelope that has no error code`,
      status,
      requestId,
    });
  }
  return createApiError({
    code: error.code,
    message: typeof error.message === 'string' ? error.message : error.code,
    status,
    details: error.details ?? null,
    requestId,
    retryAfterSeconds,
  });
}

function parseBody(text: string, status: number, requestId: string | null): Readonly<Record<string, unknown>> {
  let body: unknown;
  try {
    body = parseJson(text);
  } catch (cause) {
    throw new UnexpectedResponseError({ message: `HTTP ${status} with a non-JSON body`, status, requestId, cause });
  }
  if (!isRecord(body) || typeof body.success !== 'boolean') {
    throw new UnexpectedResponseError({ message: `HTTP ${status} without an idbseq envelope`, status, requestId });
  }
  return body;
}

function interpret(response: Response, text: string): ApiResponse {
  const { status } = response;
  const requestId = response.headers.get('x-request-id');
  const body = parseBody(text, status, requestId);
  if (body.success === false) {
    const retryAfter = parseRetryAfter(response.headers.get('retry-after'));
    throw errorFromEnvelope(body.error, status, requestId, retryAfter);
  }
  if (!response.ok) {
    throw new UnexpectedResponseError({ message: `HTTP ${status} with a success envelope`, status, requestId });
  }
  return { data: body.data ?? null, meta: body.meta ?? null, status, requestId };
}

function networkError(cause: unknown, timeoutMs: number): IdbseqNetworkError {
  if (cause instanceof Error && cause.name === 'TimeoutError') {
    return new IdbseqNetworkError(`request timed out after ${timeoutMs} ms`, cause);
  }
  const detail = cause instanceof Error ? cause.message : String(cause);
  return new IdbseqNetworkError(`request failed: ${detail}`, cause);
}

/** Sends one request and unwraps the envelope; throws an IdbseqError on failure. */
export async function send(
  config: TransportConfig,
  request: ApiRequest,
  credential: string | null,
): Promise<ApiResponse> {
  const init: RequestInit = {
    method: request.method,
    headers: buildHeaders(config, request, credential),
    signal: AbortSignal.timeout(config.timeoutMs),
    ...(request.body === undefined ? {} : { body: stringifyJson(request.body) }),
  };
  let response: Response;
  let text: string;
  try {
    response = await config.fetch(buildUrl(config.baseUrl, request), init);
    text = await response.text();
  } catch (cause) {
    throw networkError(cause, config.timeoutMs);
  }
  return interpret(response, text);
}
