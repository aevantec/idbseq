import { describe, expect, it } from 'vitest';
import {
  CollectionExistsError,
  CollectionNotFoundError,
  createApiError,
  ForbiddenError,
  IdbseqError,
  InternalError,
  NotFoundError,
  PayloadTooLargeError,
  ProtectedCollectionError,
  RateLimitedError,
  SequenceExhaustedError,
  UnauthorizedError,
  UnknownCodeError,
  ValidationFailedError,
} from '../src/errors.js';

const base = { message: 'boom', status: 400, details: null, requestId: 'r1', retryAfterSeconds: null };

describe('createApiError', () => {
  it.each([
    ['VALIDATION_FAILED', ValidationFailedError],
    ['UNAUTHORIZED', UnauthorizedError],
    ['FORBIDDEN', ForbiddenError],
    ['PROTECTED_COLLECTION', ProtectedCollectionError],
    ['NOT_FOUND', NotFoundError],
    ['COLLECTION_NOT_FOUND', CollectionNotFoundError],
    ['COLLECTION_EXISTS', CollectionExistsError],
    ['SEQUENCE_EXHAUSTED', SequenceExhaustedError],
    ['PAYLOAD_TOO_LARGE', PayloadTooLargeError],
    ['RATE_LIMITED', RateLimitedError],
    ['INTERNAL', InternalError],
  ])('maps %s to its own class', (code, errorClass) => {
    // Act
    const error = createApiError({ ...base, code });

    // Assert
    expect(error).toBeInstanceOf(errorClass);
    expect(error).toBeInstanceOf(IdbseqError);
    expect(error.name).toBe(errorClass.name);
    expect(error.code).toBe(code);
  });

  it('falls back to UnknownCodeError and keeps the HTTP status for unknown codes', () => {
    // Act
    const error = createApiError({ ...base, code: 'BRAND_NEW', status: 418 });

    // Assert
    expect(error).toBeInstanceOf(UnknownCodeError);
    expect(error.code).toBe('BRAND_NEW');
    expect(error.status).toBe(418);
  });

  it('exposes details, request id and retry-after', () => {
    // Arrange
    const details = { fields: { name: 'too long' } };

    // Act
    const error = createApiError({ ...base, code: 'RATE_LIMITED', details, retryAfterSeconds: 3 });

    // Assert
    expect(error.details).toEqual(details);
    expect(error.requestId).toBe('r1');
    expect(error.retryAfterSeconds).toBe(3);
    expect(error.message).toBe('boom');
  });
});
