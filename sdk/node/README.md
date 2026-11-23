# idbseq

[idbseq](../../README.md) for Node.js and TypeScript: unique, increasing IDs
from one portable file, embedded in your app with no server (`IdbSeqDatabase`),
or from an idbseq server over HTTP (`IdbSeqClient`).
Node 20+, ESM and CommonJS, type declarations included. Full documentation:
[aevantec.github.io/idbseq/](https://aevantec.github.io/idbseq/).

## Install

```bash
npm install idbseq
```

Both `import { IdbSeqDatabase } from 'idbseq'` and `require('idbseq')` work.
The package includes the embedded module for macOS, Linux and Windows on x64
and arm64.

## Embedded (no server)

`IdbSeqDatabase` runs idbseq inside your process, like SQLite: one file, no
server, no API key, offline. It has the same methods, types and errors as
`IdbSeqClient` (minus admin), so code typed against `Sequences` works with both.

```ts
import { IdbSeqDatabase } from 'idbseq';

const ids = await IdbSeqDatabase.open('app.idbseq'); // { shared: true } for several processes
await ids.createCollection({ name: 'invoices', start: 1000 });
const id = await ids.generateId('invoices'); // 1000
await ids.close();
```

Copies of a file count on their own, so several machines need the server. See
[Embedded](../../docs/guide/embedded.md) for modes, speed and moving a file to a
server.

## With a server

`IdbSeqClient` needs a running `idbseqd` and an API key with the scopes your
code uses (create one in the admin console, or with `idbseq keys create`); see
[Admin, API keys and scopes](../../docs/guide/security.md).

```ts
import { IdbSeqClient, SequenceExhaustedError } from 'idbseq';

const ids = new IdbSeqClient({
  url: 'http://127.0.0.1:7878',
  apiKey: process.env.IDBSEQ_KEY,
});

// 1043 (number, or bigint above 2^53)
const id = await ids.generateId('orders');
// { first, last, count, step }
const batch = await ids.generateIds('orders', 100);
const { lastId, nextId } = await ids.lastId('orders');
// { items, total, limit, nextCursor }
const page = await ids.listCollections({ limit: 50 });

try {
  await ids.generateId('tickets');
} catch (error) {
  if (error instanceof SequenceExhaustedError) {
    // error.code, error.status, error.details, error.requestId
  }
}
```

## API

`IdbSeqDatabase` and `IdbSeqClient` both have these methods; the endpoint is
what `IdbSeqClient` calls. The admin methods below the table are `IdbSeqClient` only.

| Method                                                                     | Endpoint                                  |
| -------------------------------------------------------------------------- | ----------------------------------------- |
| `generateId(name, { idempotencyKey? })`                                    | `POST /v1/collections/{name}/next`        |
| `generateIds(name, count, { idempotencyKey? })`                            | `POST /v1/collections/{name}/next?count=` |
| `lastId(name)` → `{ lastId, nextId }`                                      | `GET /v1/collections/{name}/last`         |
| `listCollections({ limit, cursor, prefix })`                               | `GET /v1/collections`                     |
| `getCollection(name)`                                                      | `GET /v1/collections/{name}`              |
| `createCollection({ name, start, step, max, protected, durability })`      | `POST /v1/collections`                    |
| `updateCollection(name, patch)`                                            | `PATCH /v1/collections/{name}`            |
| `resetCollection(name, { value?, allowRewind? })`                          | `POST /v1/collections/{name}/reset`       |
| `deleteCollection(name)`                                                   | `DELETE /v1/collections/{name}`           |
| `login(username, password)` / `logout()`                                   | `POST /v1/auth/login` / `logout`          |
| `createKey({ name, scopes, collections? })`, `listKeys()`, `revokeKey(id)` | `/v1/keys`                                |

`resetCollection` and `deleteCollection` send `confirm: name` for you. `login`
stores the admin session token on the client; later calls use it instead of the
API key until `logout()`.

- **IDs** are `number` when `Number.isSafeInteger`, `bigint` above (JSON is parsed
  losslessly). Inputs such as `max` or `value` accept either.
- **Errors**: every failure is an `IdbSeqError` with `code`, `status`, `details`,
  `requestId` and `retryAfterSeconds`. Each API code has its own class
  (`ValidationFailedError`, `UnauthorizedError`, `ForbiddenError`,
  `ProtectedCollectionError`, `NotFoundError`, `CollectionNotFoundError`,
  `CollectionExistsError`, `SequenceExhaustedError`, `PayloadTooLargeError`,
  `RateLimitedError`, `InternalError`); unknown codes give `UnknownCodeError`,
  non-envelope responses `UnexpectedResponseError`, and connection failures or
  timeouts `IdbSeqNetworkError` (`status` 0). `IdbSeqDatabase` throws the same
  classes (with `status` 0), plus `DatabaseBusyError`, `DatabaseUnavailableError`
  and `DatabaseClosedError`.
- **Retries**: `generateId` / `generateIds` send an `Idempotency-Key` (a random
  UUID per call, or yours) and retry network errors, 5xx and 429 with the same
  key, honouring `Retry-After`. Configure with
  `retry: { maxAttempts: 4, baseDelayMs: 100, maxDelayMs: 2000, maxRetryAfterMs: 30000 }`
  or turn off with `retry: false`. Other calls are not retried. `IdbSeqDatabase`
  needs no retries; it sends an idempotency key only when you pass one.
- `IdbSeqClient` options: `url`, `apiKey`, `timeoutMs` (default 10 s), `retry`, `fetch`.
- `IdbSeqDatabase.open(path, options)` options: `shared` (default `false`),
  `busyTimeoutMs` (default 5000).

## Development

```sh
npm ci
npm test # unit tests against a local mock server, fails below 80% coverage
npm run typecheck
npm run format # Prettier; CI runs npm run format:check
npm run build:native # builds the embedded module from the Rust workspace into native/
npm run test:package # builds dist/esm + dist/cjs, loads both, shares a file between processes
IDBSEQ_E2E_URL=http://127.0.0.1:7878 IDBSEQ_E2E_ADMIN_PASSWORD=… \
  npm run test:e2e
```

The e2e suite runs the critical flow against a real `idbseqd` and is skipped
unless both variables are set.
