# idbseq HTTP API v1

This is the wire contract that `idbseqd`, the Node.js SDK and the CLI implement. The
machine-readable version is [`api/openapi.json`](../api/openapi.json), generated
from the server code; if the two ever disagree, the OpenAPI file wins and this
document is fixed.

- Base URL: `http://127.0.0.1:7878` by default.
- Requests and responses are JSON (`Content-Type: application/json`), UTF-8.
- Request bodies are limited to 16 KiB by default.
- Timestamps are RFC 3339 strings in UTC, e.g. `"2026-11-10T08:15:30Z"`.
- IDs are signed 64-bit integers sent as **JSON numbers** (never strings), up to
  `9223372036854775807`. Clients in languages whose JSON numbers are doubles
  (JavaScript) must parse them losslessly.

## Envelope

Every response, success or failure, has the same shape. Success:

```json
{ "success": true, "data": { "id": 42 }, "error": null }
```

Failure:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "SEQUENCE_EXHAUSTED",
    "message": "collection `orders` is exhausted",
    "details": null
  }
}
```

Paginated lists add `meta`:

```json
{
  "success": true,
  "data": [ … ],
  "error": null,
  "meta": { "total": 120, "limit": 50, "next_cursor": "orders" }
}
```

`next_cursor` is `null` on the last page. Pass it back as `?cursor=` for the next page.

### Error codes

| Code                   | HTTP | When                                                                |
| ---------------------- | ---- | ------------------------------------------------------------------- |
| `VALIDATION_FAILED`    | 400  | Bad input. `details` is `{ "fields": { "<field>": "<problem>" } }`  |
| `UNAUTHORIZED`         | 401  | Missing, unknown, expired or revoked credentials; failed login     |
| `FORBIDDEN`            | 403  | Credentials lack the scope, or the collection is outside the key's allow-list, or no admin exists yet |
| `PROTECTED_COLLECTION` | 403  | The operation needs the admin because the collection is protected  |
| `NOT_FOUND`            | 404  | Unknown route or unknown API key id                                 |
| `COLLECTION_NOT_FOUND` | 404  | No collection with that name                                        |
| `COLLECTION_EXISTS`    | 409  | Creating a collection whose name is taken                           |
| `SEQUENCE_EXHAUSTED`   | 409  | The request would pass the collection's `max`; nothing was issued  |
| `PAYLOAD_TOO_LARGE`    | 413  | Body over the configured limit                                      |
| `REQUEST_TIMEOUT`      | 408  | The request took longer than `limits.request_timeout_secs` (default 10) |
| `RATE_LIMITED`         | 429  | Too many requests; the `Retry-After` header gives seconds to wait  |
| `INTERNAL`             | 500  | Unexpected server error; the message is generic, details are logged |
| `DATABASE_BUSY`        | 503  | `idbseqd console` only: another process held the file for the whole busy timeout (5 s). Has `Retry-After` |
| `DATABASE_UNAVAILABLE` | 503  | `idbseqd console` only: the file cannot be opened (deleted or moved while in use, unreadable, or not an idbseq file) |

Clients must treat unknown codes as errors of the given HTTP status.

## Authentication

Send `Authorization: Bearer <token>` on every `/v1` call except `POST /v1/auth/login`
and `POST /v1/auth/setup`.
The token is either:

- an **API key**, `idb_<id>_<secret>`, created by the admin (`POST /v1/keys`), with
  scopes `read`, `generate`, `manage` and an optional collection allow-list; or
- an **admin session token** from `POST /v1/auth/login`, valid 15 minutes by default.
  The admin may do everything.

Scopes:

| Scope      | Allows                                                                    |
| ---------- | ------------------------------------------------------------------------- |
| `read`     | `GET /v1/collections`, `GET /v1/collections/{name}`, `GET …/last`         |
| `generate` | `POST …/next`                                                             |
| `manage`   | create, update, reset and delete **unprotected** collections              |

The same scopes in the CLI and the MCP server:

| Scope      | CLI commands                                      | MCP tools                            |
| ---------- | ------------------------------------------------- | ------------------------------------ |
| `read`     | `collections list`, `collections info`, `last`    | `list_collections`, `get_collection` |
| `generate` | `next`                                            | `next_id`                            |
| `manage`   | `collections create`, `update`, `delete`; `reset` | `create_collection`                  |

- Scopes are independent: `generate` does not include `read`. A call outside the
  key's scopes gets `FORBIDDEN`.
- No scope covers API keys or the audit log; those are admin-only whatever the
  key holds.

Choosing scopes: give each key the fewest scopes and the narrowest allow-list it
needs, so a leaked key can only do that much, to those collections.

| Who                                | Scopes                 | Example                                                                      |
| ---------------------------------- | ---------------------- | ---------------------------------------------------------------------------- |
| A service that only needs IDs      | `generate`             | `idbseq keys create billing --scope generate --collection orders`            |
| An AI agent through the MCP server | `generate`, `read`     | `idbseq keys create my-agent --scope generate,read --collection tickets`     |
| A dashboard or monitoring          | `read`                 | `idbseq keys create grafana --scope read`                                    |
| Provisioning or setup scripts      | `manage` (plus `read`) | `idbseq keys create provisioner --scope manage,read --collection 'tenant.*'` |

The allow-list (`collections` on the key) holds exact names or prefixes ending in `*`
(`"eu.*"`). An empty or missing list allows every collection. List results are
filtered to allowed collections.

Protected collections: only the admin may create them, set or clear `protected`,
change `max`, reset or delete them. API keys get `PROTECTED_COLLECTION`.
Reading and generating work with ordinary `read` / `generate` keys.

`GET /healthz`, `GET /readyz`, `GET /metrics` (when enabled) and
`GET /v1/openapi.json` need no credentials.

## Objects

### Collection

```json
{
  "name": "orders",
  "start": 1,
  "step": 1,
  "max": 9223372036854775807,
  "protected": false,
  "durability": { "mode": "strict" },
  "created_at": "2026-11-10T08:15:30Z",
  "last_id": 1043,
  "next_id": 1044
}
```

- `name`: 1–64 chars, `^[a-z0-9][a-z0-9_.-]*$`, case-sensitive, unique.
- `step` > 0 and at most 9223372036854775807 (an int64). `start <= max`.
- `durability`: `{ "mode": "strict" }` (default; each issued ID is on disk before it
  is returned) or `{ "mode": "block", "size": N }` (reserve N IDs on disk at a time;
  a crash skips the rest of the block, leaving a gap, never a duplicate).
- `last_id`: most recently issued ID, `null` before the first.
- `next_id`: what the next `next` call would return (not reserved); `null` when
  the collection is exhausted.

### API key (as listed)

```json
{
  "id": "k7f3a9c2",
  "name": "billing-service",
  "scopes": ["read", "generate"],
  "collections": ["orders", "eu.*"],
  "created_at": "2026-11-10T08:15:30Z",
  "last_used_at": null
}
```

## Endpoints

### Health

- `GET /healthz` → `200`:

  ```json
  { "success": true, "data": { "status": "ok", "mode": "server" }, "error": null }
  ```

  `mode` is `console` under `idbseqd console <file>`, which has no accounts.
- `GET /readyz` → `200` with `{ "status": "ready", "mode": "server" }` once the store is open, `503` with `INTERNAL` otherwise.

### Auth

- `POST /v1/auth/login` body `{ "username": "admin", "password": "…" }`
  → `200 data: { "token": "…", "expires_at": "…" }`.
  The single admin's username is `admin`. Wrong user or password → `UNAUTHORIZED`
  with the same message. After 5 failures in 15 minutes from one client address
  (an IPv6 client's /64 network), that address is locked out for 15 minutes
  (`UNAUTHORIZED`, same message); other addresses can still sign in. After 50
  failures from all addresses together, every login is refused for 15 minutes.
  Changing or resetting the password ends every lockout.
- `POST /v1/auth/setup` body `{ "token": "…", "password": "…" }`
  → `200 data: { "token": "…", "expires_at": "…" }`. First-run setup from the
  [console](guide/console.md#first-run-setup): served only when the console is on
  (`404` otherwise). `token` comes from the one-time link `idbseqd` prints when it
  starts with no admin; it works once and expires after 30 minutes. Creates the
  admin with `password` (≥ 8 chars, else `VALIDATION_FAILED` and the link stays
  valid) and signs in. A wrong, used or expired token → `UNAUTHORIZED`; an
  existing admin → `FORBIDDEN`.
- `POST /v1/auth/logout` (admin token) → `200 data: null`. Revokes the token.
- `POST /v1/auth/refresh` (admin token) → `200 data: { "expires_at": "…" }`. Keeps
  the session alive: it now expires a full session TTL from now, but never more
  than 12 hours after sign-in. The token is unchanged. An API key → `FORBIDDEN`;
  an ended session → `UNAUTHORIZED`.
- `POST /v1/admin/password` (admin) body `{ "current_password": "…", "new_password": "…" }`
  → `200 data: null`. New password ≥ 8 chars. Revokes every session, including this one.

### Collections

- `GET /v1/collections?limit=50&cursor=…&prefix=…` (`read`)
  → `data: [Collection…]`, `meta`. Sorted by name. `limit` 1–500, default 50.
- `POST /v1/collections` (`manage`; admin if `protected: true`)
  body `{ "name", "start"?, "step"?, "max"?, "protected"?, "durability"? }`
  → `201 data: Collection`.
- `GET /v1/collections/{name}` (`read`) → `data: Collection`.
- `PATCH /v1/collections/{name}` (`manage`; admin for `protected` or for any change on a protected collection)
  body any of `{ "max", "protected", "start", "step", "durability" }`
  → `data: Collection`. `start` and `step` can only change before the first ID is
  issued (`VALIDATION_FAILED` afterwards). `max` cannot go below `last_id`.
- `DELETE /v1/collections/{name}?confirm={name}` (`manage`; admin if protected)
  — `confirm` may instead be sent in the body `{ "confirm": "…" }`. Missing or
  different → `VALIDATION_FAILED`. → `data: Collection` (as it was before deletion).

### IDs

- `POST /v1/collections/{name}/next` (`generate`)
  → `data: { "id": 1044 }`.
- `POST /v1/collections/{name}/next?count=100` (`generate`)
  → `data: { "first": 1045, "last": 1144, "count": 100, "step": 1 }`.
  `count` 1–10,000 (configurable). `count=1` returns the batch shape too; only a
  request **without** `count` returns `{ "id" }`.
  The whole batch is issued or nothing is (`SEQUENCE_EXHAUSTED`).
  Optional header `Idempotency-Key: <1–255 visible ASCII chars>`: a retry with the same key, collection
  and `count` from the same credentials returns the original `data` without issuing
  new IDs. The same key with a different collection or `count` → `VALIDATION_FAILED`.
  Keys are remembered for 24 hours.
- `GET /v1/collections/{name}/last` (`read`)
  → `data: { "last_id": 1144, "next_id": 1145 }` (either may be `null`).
- `POST /v1/collections/{name}/reset` (`manage`; admin if protected)
  body `{ "confirm": "<name>", "value"?: n, "allow_rewind"?: false }`
  → `data: { "previous_last_id": 1144, "last_id": null, "next_id": 1 }`.
  `value` is the next ID to issue; default `start`. It must be within
  `[start, max]` and on the step grid (`(value - start) % step == 0`). If `value`
  is not greater than the current `last_id`, the counter moves backwards and
  `allow_rewind: true` is required (`VALIDATION_FAILED` otherwise).

### API keys (admin)

- `GET /v1/keys` → `data: [ApiKey…]` (no secrets).
- `POST /v1/keys` body `{ "name", "scopes": ["read"|"generate"|"manage", …], "collections"?: [ … ] }`
  → `201 data: ApiKey + { "key": "idb_<id>_<secret>" }`.
  The full key is shown only here.
- `DELETE /v1/keys/{id}` → `data: null`. Revoked immediately. Unknown id → `NOT_FOUND`.

### Audit (admin)

- `GET /v1/audit?limit=50&cursor=…&collection=…&action=…`
  → `data: [AuditEntry…]`, `meta`. Newest first.
  `AuditEntry` fields:

  ```
  { "id", "at", "actor", "action", "collection", "previous", "new",
    "request_id", "client_ip" }
  ```

  Actions: `collection.create`, `collection.update`, `collection.reset`,
  `collection.delete`, `key.create`, `key.revoke`, `auth.login`, `auth.login_failed`,
  `admin.password_change`, `admin.init`.

### Contract

- `GET /v1/openapi.json` → the OpenAPI 3.1 document (not wrapped in the envelope).

## Headers

- Every response carries `X-Request-Id`: echoed from the request if sent (1–128
  visible ASCII characters; anything else is replaced), else generated.
- `429` responses carry `Retry-After` (seconds).
- Security headers: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`,
  `Referrer-Policy: no-referrer`, `Cache-Control: no-store`.
