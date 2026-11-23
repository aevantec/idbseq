import { describe, expect, it } from 'vitest';
import { createApiError, IdbSeqNetworkError, UnexpectedResponseError } from '../src/errors.js';
import { DEFAULT_RETRY, isRetryable, resolveRetryPolicy, retryDelayMs } from '../src/retry.js';

const apiError = (status: number, code: string, retryAfterSeconds: number | null = null) =>
  createApiError({ message: 'x', status, code, details: null, requestId: null, retryAfterSeconds });

describe('resolveRetryPolicy', () => {
  it('uses the defaults when nothing is given', () => {
    expect(resolveRetryPolicy(undefined)).toEqual(DEFAULT_RETRY);
  });

  it('turns retries off with false', () => {
    expect(resolveRetryPolicy(false).maxAttempts).toBe(1);
  });

  it('merges partial options over the defaults', () => {
    expect(resolveRetryPolicy({ maxAttempts: 7 })).toEqual({ ...DEFAULT_RETRY, maxAttempts: 7 });
  });

  it.each([
    [{ maxAttempts: 0 }],
    [{ maxAttempts: 1.5 }],
    [{ baseDelayMs: -1 }],
    [{ maxDelayMs: Number.NaN }],
    [{ maxRetryAfterMs: -5 }],
  ])('rejects invalid option %o', (options) => {
    expect(() => resolveRetryPolicy(options)).toThrow(RangeError);
  });
});

describe('isRetryable', () => {
  it('retries network failures, 5xx and 429', () => {
    expect(isRetryable(new IdbSeqNetworkError('down'))).toBe(true);
    expect(isRetryable(apiError(500, 'INTERNAL'))).toBe(true);
    expect(isRetryable(apiError(503, 'SOMETHING'))).toBe(true);
    expect(isRetryable(apiError(429, 'RATE_LIMITED'))).toBe(true);
    expect(isRetryable(new UnexpectedResponseError({ message: 'bad gateway', status: 502, requestId: null }))).toBe(
      true,
    );
  });

  it('does not retry client errors or foreign errors', () => {
    expect(isRetryable(apiError(409, 'SEQUENCE_EXHAUSTED'))).toBe(false);
    expect(isRetryable(apiError(400, 'VALIDATION_FAILED'))).toBe(false);
    expect(isRetryable(new Error('other'))).toBe(false);
  });
});

describe('retryDelayMs', () => {
  const policy = { maxAttempts: 5, baseDelayMs: 100, maxDelayMs: 1000, maxRetryAfterMs: 5000 };

  it('backs off exponentially with jitter, capped at maxDelayMs', () => {
    const error = apiError(500, 'INTERNAL');
    expect(retryDelayMs(policy, 1, error, () => 0)).toBe(50);
    expect(retryDelayMs(policy, 1, error, () => 1)).toBe(100);
    expect(retryDelayMs(policy, 3, error, () => 1)).toBe(400);
    expect(retryDelayMs(policy, 10, error, () => 1)).toBe(1000);
  });

  it('honours Retry-After', () => {
    expect(retryDelayMs(policy, 1, apiError(429, 'RATE_LIMITED', 2))).toBe(2000);
  });

  it('gives up when Retry-After is longer than maxRetryAfterMs', () => {
    expect(retryDelayMs(policy, 1, apiError(429, 'RATE_LIMITED', 60))).toBeNull();
  });
});
