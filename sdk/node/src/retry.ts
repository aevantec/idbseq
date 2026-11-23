import { IdbSeqError, IdbSeqNetworkError } from './errors.js';

/** Retry settings for `generateId` / `generateIds`. */
export interface RetryOptions {
  /** Total attempts including the first one. Default 4. */
  readonly maxAttempts?: number;
  /** First backoff delay; doubles per attempt. Default 100 ms. */
  readonly baseDelayMs?: number;
  /** Upper bound of the exponential backoff. Default 2000 ms. */
  readonly maxDelayMs?: number;
  /** A `Retry-After` longer than this is not waited for; the error is thrown. Default 30 s. */
  readonly maxRetryAfterMs?: number;
}

export type RetryPolicy = Required<RetryOptions>;

export const DEFAULT_RETRY: RetryPolicy = Object.freeze({
  maxAttempts: 4,
  baseDelayMs: 100,
  maxDelayMs: 2_000,
  maxRetryAfterMs: 30_000,
});

const HTTP_TOO_MANY_REQUESTS = 429;
const HTTP_SERVER_ERROR = 500;
const MS_PER_SECOND = 1_000;

function assertNonNegative(name: string, value: number): void {
  if (!Number.isFinite(value) || value < 0) {
    throw new RangeError(`retry.${name} must be a finite number >= 0`);
  }
}

/** Validates retry options and fills in defaults; `false` disables retries. */
export function resolveRetryPolicy(options: RetryOptions | false | undefined): RetryPolicy {
  if (options === false) {
    return { ...DEFAULT_RETRY, maxAttempts: 1 };
  }
  const policy: RetryPolicy = { ...DEFAULT_RETRY, ...options };
  if (!Number.isInteger(policy.maxAttempts) || policy.maxAttempts < 1) {
    throw new RangeError('retry.maxAttempts must be an integer >= 1');
  }
  assertNonNegative('baseDelayMs', policy.baseDelayMs);
  assertNonNegative('maxDelayMs', policy.maxDelayMs);
  assertNonNegative('maxRetryAfterMs', policy.maxRetryAfterMs);
  return policy;
}

/** Network failures, 5xx and 429 are worth retrying; nothing else is. */
export function isRetryable(error: unknown): boolean {
  if (error instanceof IdbSeqNetworkError) {
    return true;
  }
  return error instanceof IdbSeqError && (error.status >= HTTP_SERVER_ERROR || error.status === HTTP_TOO_MANY_REQUESTS);
}

/**
 * Delay before the next attempt, after `attempt` attempts failed with `error`, or
 * `null` when the server asks to wait longer than the policy allows.
 */
export function retryDelayMs(
  policy: RetryPolicy,
  attempt: number,
  error: IdbSeqError,
  random: () => number = Math.random,
): number | null {
  if (error.retryAfterSeconds !== null) {
    const requested = error.retryAfterSeconds * MS_PER_SECOND;
    return requested > policy.maxRetryAfterMs ? null : requested;
  }
  const ceiling = Math.min(policy.maxDelayMs, policy.baseDelayMs * 2 ** (attempt - 1));
  return ceiling / 2 + (random() * ceiling) / 2;
}

const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** Runs `operation` until it succeeds, fails with a non-retryable error, or attempts run out. */
export async function withRetry<T>(policy: RetryPolicy, operation: () => Promise<T>): Promise<T> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      if (attempt >= policy.maxAttempts || !isRetryable(error)) {
        throw error;
      }
      const delay = retryDelayMs(policy, attempt, error as IdbSeqError);
      if (delay === null) {
        throw error;
      }
      await sleep(delay);
    }
  }
}
