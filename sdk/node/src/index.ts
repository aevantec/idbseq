export { DEFAULT_TIMEOUT_MS, DEFAULT_URL, IdbSeqClient, type ClientOptions } from './client.js';
export { DEFAULT_BUSY_TIMEOUT_MS, IdbSeqDatabase, type DatabaseOptions, type Sequences } from './embedded.js';
export {
  CollectionExistsError,
  CollectionNotFoundError,
  DatabaseBusyError,
  DatabaseClosedError,
  DatabaseUnavailableError,
  ForbiddenError,
  IdbSeqError,
  IdbSeqNetworkError,
  InternalError,
  NotFoundError,
  PayloadTooLargeError,
  ProtectedCollectionError,
  RateLimitedError,
  SequenceExhaustedError,
  UnauthorizedError,
  UnexpectedResponseError,
  UnknownCodeError,
  ValidationFailedError,
  type IdbSeqErrorInit,
} from './errors.js';
export { parseJson, stringifyJson } from './json.js';
export { DEFAULT_RETRY, type RetryOptions } from './retry.js';
export type * from './types.js';
