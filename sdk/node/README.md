# idbseq

Node.js / TypeScript client for the [idbseq](../../README.md) ID sequence server.
Node 20+, ESM and CommonJS, type declarations included. Not published yet
(`"private": true`).

```ts
import { IdbseqClient, SequenceExhaustedError } from 'idbseq';

const ids = new IdbseqClient({ url: 'http://127.0.0.1:7878', apiKey: process.env.IDBSEQ_KEY });

const id = await ids.generateId('orders');               // 1043 (number, or bigint above 2^53)
const batch = await ids.generateIds('orders', 100);      // { first, last, count, step }
const { lastId, nextId } = await ids.lastId('orders');
const page = await ids.listCollections({ limit: 50 });   // { items, total, limit, nextCursor }

try {
  await ids.generateId('tickets');
} catch (error) {
  if (error instanceof SequenceExhaustedError) {
    // error.code, error.status, error.details, error.requestId
  }
}
```

## API

| Method                                              | Endpoint                                  |
| --------------------------------------------------- | ----------------------------------------- |
| `generateId(name, { idempotencyKey? })`             | `POST /v1/collections/{name}/next`        |
| `generateIds(name, count, { idempotencyKey? })`     | `POST /v1/collections/{name}/next?count=` |
| `lastId(name)` → `{ lastId, nextId }`               | `GET /v1/collections/{name}/last`         |
| `listCollections({ limit, cursor, prefix })`        | `GET /v1/collections`                     |
| `getCollection(name)`                               | `GET /v1/collections/{name}`              |
| `createCollection({ name, start, step, max, protected, durability })` | `POST /v1/collections`  |
| `updateCollection(name, patch)`                     | `PATCH /v1/collections/{name}`            |
| `resetCollection(name, { value?, allowRewind? })`   | `POST /v1/collections/{name}/reset`       |
| `deleteCollection(name)`                            | `DELETE /v1/collections/{name}`           |
| `login(username, password)` / `logout()`            | `POST /v1/auth/login` / `logout`          |
| `createKey({ name, scopes, collections? })`, `listKeys()`, `revokeKey(id)` | `/v1/keys`         |

`resetCollection` and `deleteCollection` send `confirm: name` for you. `login`
stores the admin session token on the client; later calls use it instead of the
API key until `logout()`.

- **IDs** are `number` when `Number.isSafeInteger`, `bigint` above (JSON is parsed
  losslessly). Inputs such as `max` or `value` accept either.
- **Errors**: every failure is an `IdbseqError` with `code`, `status`, `details`,
  `requestId` and `retryAfterSeconds`. Each API code has its own class
  (`ValidationFailedError`, `UnauthorizedError`, `ForbiddenError`,
  `ProtectedCollectionError`, `NotFoundError`, `CollectionNotFoundError`,
  `CollectionExistsError`, `SequenceExhaustedError`, `PayloadTooLargeError`,
  `RateLimitedError`, `InternalError`); unknown codes give `UnknownCodeError`,
  non-envelope responses `UnexpectedResponseError`, and connection failures or
  timeouts `IdbseqNetworkError` (`status` 0).
- **Retries**: `generateId` / `generateIds` send an `Idempotency-Key` (a random
  UUID per call, or yours) and retry network errors, 5xx and 429 with the same
  key, honouring `Retry-After`. Configure with
  `retry: { maxAttempts: 4, baseDelayMs: 100, maxDelayMs: 2000, maxRetryAfterMs: 30000 }`
  or turn off with `retry: false`. Other calls are not retried.
- Options: `url`, `apiKey`, `timeoutMs` (default 10 s), `retry`, `fetch`.

## Development

```sh
npm ci
npm test              # unit tests against a local mock server, fails below 80% coverage
npm run typecheck
npm run test:package  # builds dist/esm + dist/cjs and loads both
IDBSEQ_E2E_URL=http://127.0.0.1:7878 IDBSEQ_E2E_ADMIN_PASSWORD=… npm run test:e2e
```

The e2e suite runs the critical flow against a real `idbseqd` and is skipped
unless both variables are set.
