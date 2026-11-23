# idbseq — Product and Architecture Plan

Status: **pre-release** (M0–M4 done; v0.1.0 is the first public release) · Last updated: 2026-11-10

IDs such as `US-0017` (user stories) and `ISSUE-0004` (decisions) refer to the
project's planning cards; decisions that shape the code are recorded as ADRs in
[`docs/adr/`](adr/README.md).


## 1. What idbseq is

idbseq is an embeddable ID sequence generator written in Rust that hands out
**increasing numeric IDs**, from one portable file inside the app, or from a small
server when several machines share a sequence (ADR-0006). IDs are grouped into **collections**: each collection is an independent
counter whose only content is the sequence of numbers it has issued.

```
collection "orders"    → 1, 2, 3, 4, …
collection "invoices"  → 1000, 1010, 1020, …   (start 1000, step 10)
collection "tickets"   → 1, 2, 3, …            (protected: only the admin may reset or wipe it)
```

Any language reaches it over HTTP/JSON. Node.js gets an official SDK; other
languages use the OpenAPI contract or plain HTTP.

### Goals

- **Uniqueness above all:** an ID is never issued twice from a collection, including
  under concurrency, process crashes and restarts.
- **Any language:** a documented REST API with an OpenAPI contract, plus an SDK.
- **Many collections:** create, list, inspect, reset and delete collections at runtime.
- **Protected collections:** an admin account guards destructive operations
  (reset, wipe) on collections marked `protected`.
- **Simple to run:** one binary or container, embedded storage, no external database.

### Non-goals (v1)

- Formatted IDs such as `INV-000123`: clients format numbers (US-0036, Won't Do).
- Multiple admin accounts or role hierarchies (US-0037, Won't Do).
- Clustering or HA replication (ISSUE-0004).
- Gapless numbering guarantees (ISSUE-0001).

## 2. Glossary

| Term           | Meaning                                                                     |
| -------------- | --------------------------------------------------------------------------- |
| Collection     | A named, independent counter with `start`, `step`, `max` and `protected`.   |
| generateId     | Consume and return the next ID of a collection (`POST …/next`).             |
| lastId         | Read the most recently issued ID without consuming one (`GET …/last`).      |
| Reset          | Move a collection's counter back to `start` or to a chosen value.           |
| Wipe / delete  | Remove a collection and its counter.                                        |
| Admin          | The single administrator account; required for protected operations.       |
| API key        | A credential for an application, with scopes `read`, `generate`, `manage`.  |

## 3. Architecture

```
 ┌───────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐
 │ Node SDK      │  │ MCP server   │  │ idbseq CLI   │  │ curl / Python │
 │ idbseq (npm)  │  │ idbseq-mcp   │  │ (Rust)       │  │ / Go / …      │
 └──────┬────────┘  └──────┬───────┘  └──────┬───────┘  └──────┬────────┘
        └──────────────────┴─── HTTP/JSON (REST v1) ───────────┘
                                    │
                    ┌───────────────▼────────────────┐
                    │ idbseqd  (idbseq-server)       │
                    │  axum · auth · rate limit ·    │
                    │  envelope · OpenAPI · audit    │
                    └───────────────┬────────────────┘
                                    │
                    ┌───────────────▼────────────────┐
                    │ idbseq-core                    │
                    │  collection model · sequence   │
                    │  engine · SequenceStore trait  │
                    └───────────────┬────────────────┘
                                    │
                    ┌───────────────▼────────────────┐
                    │ redb (embedded, ACID, fsync)   │
                    │  data dir with exclusive lock  │
                    └────────────────────────────────┘
```

### Crates and packages

| Path / crate          | Kind    | Role                                                        |
| --------------------- | ------- | ----------------------------------------------------------- |
| `crates/idbseq-core`  | library | Collection model, validation, sequence engine, storage trait |
| `crates/idbseq-server`| binary  | `idbseqd`: HTTP API, auth, admin bootstrap, ops commands     |
| `crates/idbseq-cli`   | binary  | `idbseq`: command-line client over the HTTP API              |
| `sdk/node`            | npm     | `idbseq` TypeScript SDK                            |
| `api/openapi.json`    | spec    | Committed OpenAPI 3.1 contract                               |

### Storage and durability

- Embedded **redb** (pure Rust, ACID). Each `generateId` is a write transaction that
  is durable before the response is sent, so a crash never leads to reissuing a
  number already returned.
- Storage is behind a `SequenceStore` trait with a shared conformance suite, so a
  different backend (for example Postgres) can be added later without API changes.
- Requests on different collections share group commits (US-0039) and do not wait
  on each other's fsync; one collection's increments are serialized.
- Optional **block reservation** mode (US-0035) trades possible gaps after a crash
  for much higher throughput.

## 4. Data model

| Field        | Type          | Default                 | Notes                                          |
| ------------ | ------------- | ----------------------- | ---------------------------------------------- |
| `name`       | string        | —                       | 1–64 chars, `^[a-z0-9][a-z0-9_.-]*$`, unique   |
| `start`      | integer       | `1`                     | Immutable after the first ID is issued         |
| `step`       | integer > 0   | `1`                     | Immutable after the first ID is issued         |
| `max`        | integer       | 2^63 − 1 (ISSUE-0002)   | Generation past `max` → `SEQUENCE_EXHAUSTED`   |
| `protected`  | boolean       | `false`                 | Admin-only to set or clear                     |
| `last_id`    | integer\|null | `null`                  | Most recently issued ID                        |
| `created_at` | timestamp     | now                     |                                                |

## 5. API sketch (REST v1)

All responses use one envelope. Success:

```json
{ "success": true, "data": { "id": 42 }, "error": null }
```

Failure:

```json
{
  "success": false,
  "data": null,
  "error": { "code": "SEQUENCE_EXHAUSTED", "message": "…" }
}
```

| Method | Path                                  | Purpose                              | Who                          |
| ------ | ------------------------------------- | ------------------------------------ | ---------------------------- |
| GET    | `/healthz`, `/readyz`                 | Liveness / readiness                 | anyone                       |
| POST   | `/v1/auth/login` · `/v1/auth/logout`  | Admin session token                  | admin                        |
| POST   | `/v1/admin/password`                  | Change admin password                | admin                        |
| GET    | `/v1/collections`                     | List collections (paginated)         | `read`                       |
| POST   | `/v1/collections`                     | Create a collection                  | `manage` (admin if protected)|
| GET    | `/v1/collections/{name}`              | Collection details                   | `read`                       |
| PATCH  | `/v1/collections/{name}`              | Change `max` / `protected`           | `manage` / admin             |
| DELETE | `/v1/collections/{name}`              | Wipe a collection (needs `confirm`)  | `manage` / admin if protected|
| POST   | `/v1/collections/{name}/next`         | **generateId** (`?count=n` batch)    | `generate`                   |
| GET    | `/v1/collections/{name}/last`         | **lastId** + preview of next         | `read`                       |
| POST   | `/v1/collections/{name}/reset`        | Reset counter (needs `confirm`)      | `manage` / admin if protected|
| GET/POST/DELETE | `/v1/keys`, `/v1/keys/{id}`  | API key management                   | admin                        |
| GET    | `/v1/audit`                           | Audit log                            | admin                        |
| GET    | `/v1/openapi.json`                    | API contract                         | anyone                       |

Error codes: `VALIDATION_FAILED`, `UNAUTHORIZED`, `FORBIDDEN`,
`PROTECTED_COLLECTION`, `COLLECTION_NOT_FOUND`, `COLLECTION_EXISTS`,
`SEQUENCE_EXHAUSTED`, `RATE_LIMITED`, `INTERNAL`.

### SDK shape (Node example)

```ts
const ids = new IdbSeqClient({
  url: 'http://127.0.0.1:7878',
  apiKey: process.env.IDBSEQ_KEY,
});

// 1043
const id = await ids.generateId('orders');
// { first: 1044, last: 1143, count: 100 }
const batch = await ids.generateIds('orders', 100);
// 1143
const last = await ids.lastId('orders');
const page = await ids.listCollections({ limit: 50 });
```

## 6. Security model

- **One admin account**, created with `idbseqd admin init` (or `IDBSEQ_ADMIN_PASSWORD`
  on first start). Argon2id hash; short-lived opaque session tokens; login lockout.
- **API keys** for applications, created by the admin, shown once, stored hashed,
  scoped (`read` / `generate` / `manage`) and optionally limited to collections.
- **Protected collections:** reset, wipe and limit changes require the admin.
  Generating and reading IDs work with ordinary keys.
- Destructive calls require `confirm: "<collection name>"` in the body.
- Audit log for destructive and admin operations.
- Loopback bind by default, optional TLS, request limits and per-key rate limits.

## 7. Milestones

Each card carries a milestone label (`m0-…` to `m5-…`) and an `area`.

| Milestone             | Stories                                   | Outcome                                          | State |
| --------------------- | ----------------------------------------- | ------------------------------------------------ | ----- |
| M0 Foundation         | US-0001 – US-0003                         | Workspace, CI with 80% coverage gate, ADRs       | Done |
| M1 Core engine        | US-0004 – US-0013, US-0035, US-0038, US-0039 | Durable, concurrent-safe, benchmarked sequence library | Done (benchmarks: `benches/baseline.md`, ADR-0005 proposed) |
| M2 HTTP API           | US-0014 – US-0020                         | `idbseqd` REST API with OpenAPI contract         | Done |
| M3 Security & admin   | US-0021 – US-0026                         | Admin, API keys, protected collections, audit    | Done |
| M4 Clients            | US-0027 – US-0031                         | CLI, Node SDK, other-language docs, e2e | Done (the PHP SDK was dropped on 2026-11-18) |
| M5 Operations         | US-0032 – US-0034, US-0040                | Releases, Docker, backup/restore, metrics, public release | US-0032–34 done (`v0.1.0-rc.2`); US-0040 (go public) in progress |
| M6 After release      | US-0041 – US-0045                         | SDK ID prefetch for distant clients, Windows builds, docs website, agent skill, MCP server | US-0043 (docs website, VitePress) and US-0045 (MCP server) built, in review, published with the first public release; the rest planned |

Suggested critical path: US-0001 → US-0002 → US-0005 → US-0004 → US-0006 →
US-0007 → US-0010 → US-0014 → US-0015 → US-0016/17 → US-0021 → US-0022 →
US-0023 → US-0024 → US-0019 → US-0028/29.

## 8. Open decisions

Settled decisions are recorded as ADRs in [`docs/adr/`](adr/README.md).

ISSUE-0001 to ISSUE-0004, ISSUE-0006 and ISSUE-0007 are settled provisionally (ADRs
0001–0004; the owner may revisit them). ISSUE-0005 is decided: MIT, public npm and GHCR.

| Card       | Decision                                         | Recommendation                         |
| ---------- | ------------------------------------------------ | -------------------------------------- |
| ISSUE-0001 | Are gaps acceptable?                             | Gaps allowed, duplicates never         |
| ISSUE-0002 | Numeric range and JSON representation            | Signed 64-bit, JSON number, SDK bigint |
| ISSUE-0003 | Transport for v1                                 | REST/JSON only                         |
| ISSUE-0004 | Single node vs HA                                | Single node + backups                  |
| ISSUE-0005 | License and package registries                   | MIT; public npm, GHCR (decided) |
| ISSUE-0006 | Who may reset/wipe unprotected collections       | `manage` keys; rewinds need explicit flag |
| ISSUE-0007 | redb has one writer: how collections share it    | Group commit (US-0039) + block mode     |

