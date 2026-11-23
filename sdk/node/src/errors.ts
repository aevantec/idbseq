/** Fields shared by every idbseq error. */
export interface IdbSeqErrorInit {
  readonly message: string;
  readonly code: string;
  /** HTTP status; `0` when no response was received. */
  readonly status: number;
  readonly details: unknown;
  readonly requestId: string | null;
  readonly retryAfterSeconds: number | null;
  readonly cause?: unknown;
}

/** Base class of every error thrown by the SDK for a failed call. */
export class IdbSeqError extends Error {
  /** API error code, e.g. `SEQUENCE_EXHAUSTED`, or `NETWORK_ERROR` / `UNEXPECTED_RESPONSE`. */
  readonly code: string;
  readonly status: number;
  /** `error.details` from the response; for `VALIDATION_FAILED`, `{ fields: { … } }`. */
  readonly details: unknown;
  /** The `X-Request-Id` response header, for correlating with server logs. */
  readonly requestId: string | null;
  /** Seconds to wait from the `Retry-After` header, when present. */
  readonly retryAfterSeconds: number | null;

  constructor(init: IdbSeqErrorInit) {
    super(init.message, init.cause === undefined ? undefined : { cause: init.cause });
    this.name = new.target.name;
    this.code = init.code;
    this.status = init.status;
    this.details = init.details;
    this.requestId = init.requestId;
    this.retryAfterSeconds = init.retryAfterSeconds;
  }
}

/** `VALIDATION_FAILED` (400). */
export class ValidationFailedError extends IdbSeqError {}
/** `UNAUTHORIZED` (401). */
export class UnauthorizedError extends IdbSeqError {}
/** `FORBIDDEN` (403). */
export class ForbiddenError extends IdbSeqError {}
/** `PROTECTED_COLLECTION` (403): the operation needs the admin. */
export class ProtectedCollectionError extends IdbSeqError {}
/** `NOT_FOUND` (404): unknown route or API key id. */
export class NotFoundError extends IdbSeqError {}
/** `COLLECTION_NOT_FOUND` (404). */
export class CollectionNotFoundError extends IdbSeqError {}
/** `COLLECTION_EXISTS` (409). */
export class CollectionExistsError extends IdbSeqError {}
/** `SEQUENCE_EXHAUSTED` (409): nothing was issued. */
export class SequenceExhaustedError extends IdbSeqError {}
/** `PAYLOAD_TOO_LARGE` (413). */
export class PayloadTooLargeError extends IdbSeqError {}
/** `RATE_LIMITED` (429). */
export class RateLimitedError extends IdbSeqError {}
/** `INTERNAL` (500). */
export class InternalError extends IdbSeqError {}
/** An error code this SDK version does not know; check `status`. */
export class UnknownCodeError extends IdbSeqError {}

/** The request never got a response (connection failure or timeout). */
export class IdbSeqNetworkError extends IdbSeqError {
  constructor(message: string, cause?: unknown) {
    super({
      message,
      code: 'NETWORK_ERROR',
      status: 0,
      details: null,
      requestId: null,
      retryAfterSeconds: null,
      cause,
    });
  }
}

/** The server answered with something that is not a valid idbseq envelope. */
export class UnexpectedResponseError extends IdbSeqError {
  constructor(init: { message: string; status: number; requestId: string | null; cause?: unknown }) {
    super({ ...init, code: 'UNEXPECTED_RESPONSE', details: null, retryAfterSeconds: null });
  }
}

/** An embedded database's file stayed in use by another process for the whole busy timeout. */
export class DatabaseBusyError extends IdbSeqError {}
/** An embedded database's file could not be opened or created (missing folder, no permission, not an idbseq file). */
export class DatabaseUnavailableError extends IdbSeqError {}
/** The embedded database was closed. */
export class DatabaseClosedError extends IdbSeqError {}

type ApiErrorClass = new (init: IdbSeqErrorInit) => IdbSeqError;

const ERROR_CLASSES: Readonly<Record<string, ApiErrorClass>> = Object.freeze({
  VALIDATION_FAILED: ValidationFailedError,
  UNAUTHORIZED: UnauthorizedError,
  FORBIDDEN: ForbiddenError,
  PROTECTED_COLLECTION: ProtectedCollectionError,
  NOT_FOUND: NotFoundError,
  COLLECTION_NOT_FOUND: CollectionNotFoundError,
  COLLECTION_EXISTS: CollectionExistsError,
  SEQUENCE_EXHAUSTED: SequenceExhaustedError,
  PAYLOAD_TOO_LARGE: PayloadTooLargeError,
  RATE_LIMITED: RateLimitedError,
  INTERNAL: InternalError,
  DATABASE_BUSY: DatabaseBusyError,
  DATABASE_UNAVAILABLE: DatabaseUnavailableError,
  DATABASE_CLOSED: DatabaseClosedError,
});

/** Builds the error class matching an API error code. */
export function createApiError(init: IdbSeqErrorInit): IdbSeqError {
  const ErrorClass = Object.hasOwn(ERROR_CLASSES, init.code) ? ERROR_CLASSES[init.code] : undefined;
  return new (ErrorClass ?? UnknownCodeError)(init);
}
