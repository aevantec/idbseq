<img src="docs/public/logo.svg" width="96" height="96" alt="idbseq">

# idbseq

[![CI](https://github.com/aevantec/idbseq/actions/workflows/ci.yml/badge.svg)](https://github.com/aevantec/idbseq/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/idbseq)](https://www.npmjs.com/package/idbseq)
[![crates.io](https://img.shields.io/crates/v/idbseq)](https://crates.io/crates/idbseq)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

An embeddable ID sequence generator, like SQLite for counters. It runs inside
your app with its data in **one portable file**: no server, no network. Create
named **collections**, each an independent counter, and take the next
increasing ID, never the same one twice, even across crashes.

```ts
import { IdbSeqDatabase } from 'idbseq';

const ids = await IdbSeqDatabase.open('app.idbseq');
await ids.createCollection({ name: 'orders', start: 1043 });
const id = await ids.generateId('orders'); // 1043
```

```rust
let ids = idbseq::Database::open("app.idbseq")?;
let id = ids.next_id("orders")?;
```

When several machines need one sequence, `idbseqd` serves the same engine over
HTTP/JSON, and an embedded file can move to it:

```
POST /v1/collections/orders/next
→ { "success": true, "data": { "id": 1044 }, "error": null }
```

- **Embedded**: a Rust crate (`idbseq`) and a Node.js package (`IdbSeqDatabase`);
  works offline; several processes on one machine can share a file
- **Portable**: the same file format on every OS; copy it, back it up, or serve it
- **Durable**: every ID is on disk before it is returned, so none is ever issued twice
- Many collections, each with its own `start`, `step` and `max`; batches,
  `lastId`, list, reset and delete
- **Server** (`idbseqd`): any language over HTTP/JSON with an OpenAPI contract,
  scoped API keys, an admin console, **protected** collections, an audit log, a
  CLI and an MCP server for AI agents

## Documentation

**Documentation: [aevantec.github.io/idbseq/](https://aevantec.github.io/idbseq/)** (published with
each release; built from [`docs/`](docs)). It has a guide to the concepts,
deployment, monitoring and every client, plus an interactive API explorer.

- [HTTP API reference](docs/API.md) and the committed [OpenAPI contract](api/openapi.json)
- [Running idbseqd](docs/operations.md): configuration, backup/restore, health, metrics
- [Releasing](docs/releasing.md): what a `v*` tag publishes and the one-time registry setup
- [Clients](docs/clients.md): Node, the CLI, the MCP server, and curl/Python/Go/Java examples
- [Product and architecture plan](docs/PLAN.md) and [decision records](docs/adr/README.md)

## Quick start: embedded

```bash
npm install idbseq   # Node.js 20+
cargo add idbseq     # Rust
```

Then open a file and take IDs as above. See
[Getting started](docs/guide/getting-started.md) and
[Embedding in depth](docs/guide/embedded.md).

## Quick start: server from source

```bash
cargo build --release
# Serves on 127.0.0.1:7878, data in ./data
IDBSEQ_ADMIN_PASSWORD='choose-a-long-password' ./target/release/idbseqd

export IDBSEQ_URL=http://127.0.0.1:7878
./target/release/idbseq login # stores an admin session
./target/release/idbseq collections create orders --start 1000
./target/release/idbseq next orders # 1000
./target/release/idbseq keys create billing --scope generate --collection orders
```

Applications then call `POST /v1/collections/orders/next` with
`Authorization: Bearer <key>`, or use an SDK.

## Quick start: server with Docker

Each `v*` tag publishes a multi-arch image (linux/amd64, linux/arm64) to GHCR.

```bash
docker volume create idbseq-data
docker run -d --name idbseq -p 7878:7878 \
  -v idbseq-data:/data \
  -e IDBSEQ_ADMIN_PASSWORD='choose-a-long-password' \
  ghcr.io/aevantec/idbseq:latest

# {"success":true,"data":{"status":"ok"},"error":null}
curl -s http://127.0.0.1:7878/healthz
docker inspect --format '{{.State.Health.Status}}' idbseq
```

The image runs `idbseqd` as a non-root user on distroless, listens on
`0.0.0.0:7878` (`IDBSEQ_BIND`), keeps its data in the `/data` volume
(`IDBSEQ_DATA_DIR`) and reports health with `idbseqd healthcheck`.
`IDBSEQ_ADMIN_PASSWORD` creates the admin on first start; remove it afterwards.
The image also contains the `idbseq` CLI:
`docker exec -it idbseq idbseq --help`. Prebuilt binaries for Linux, macOS and
Windows (with `SHA256SUMS`) are attached to each GitHub Release. To build the image
yourself: `docker build -t idbseq .`

## Admin account and API keys

The **admin** (one account, `admin`) is the person who runs the server. Only the
admin creates and revokes API keys, looks after **protected** collections
(create, change `max`, reset, delete) and reads the audit log. Sign in with
`idbseq login`; a session lasts 15 minutes, and 5 failed logins lock out that
address for 15 minutes. A forgotten password is reset on the server host with
`idbseqd admin reset-password`.

**API keys** are for applications and agents. Each key holds one or more
scopes, and the server checks them on every call (`FORBIDDEN` otherwise):

| Scope      | Allows                                                       | CLI / MCP                                                                 |
| ---------- | ------------------------------------------------------------ | ------------------------------------------------------------------------- |
| `read`     | List collections, show one, read its last and next ID        | `collections list`, `info`, `last` / `list_collections`, `get_collection` |
| `generate` | Take IDs, one at a time or in batches                        | `next` / `next_id`                                                        |
| `manage`   | Create, change, reset and delete **unprotected** collections | `collections create`, `update`, `delete`, `reset` / `create_collection`   |

Scopes are independent: `generate` does not include `read`. No scope covers keys
or the audit log; those stay with the admin. `--collection orders` (or
`'eu.*'`) limits a key to those collections; without it the key works on all of
them. Give each key the fewest scopes and the narrowest list it needs, for
example `--scope generate --collection orders` for a service that only takes
IDs, or `--scope generate,read` for an AI agent.

Never hand an admin session to an application or the MCP server: reset and
delete stay behind a person's password, so a leaked key cannot wipe the
counters that matter. More in [docs/API.md](docs/API.md#authentication) and
[docs/operations.md](docs/operations.md#the-admin-account).

## Commands

`idbseq` is the command-line client, `idbseqd` the server, and `idbseq-mcp` an
MCP server for AI agents. Each prints its own help with `--help`.

### `idbseq` (client)

| Command                            | Does                                                              |
| ---------------------------------- | ----------------------------------------------------------------- |
| `idbseq next <name>`               | Take the next ID, or a batch with `--count N`                     |
| `idbseq last <name>`               | Show the last issued ID and the next one, without using anything  |
| `idbseq collections list`          | List collections (`--prefix`, `--all` for every page)             |
| `idbseq collections create <name>` | Create a collection (`--start`, `--step`, `--max`, `--protected`) |
| `idbseq collections info <name>`   | Show a collection's settings and its last and next ID             |
| `idbseq collections update <name>` | Change a collection's settings                                    |
| `idbseq collections delete <name>` | Delete a collection and its counter (asks first)                  |
| `idbseq reset <name>`              | Reset the counter (asks first; backwards needs `--allow-rewind`)  |
| `idbseq keys create <name>`        | Create an API key (admin); the secret is shown once               |
| `idbseq keys list`                 | List API keys, without secrets (admin)                            |
| `idbseq keys revoke <id>`          | Revoke an API key (admin; asks first)                             |
| `idbseq login` / `idbseq logout`   | Start or end an admin session (15 minutes)                        |
| `idbseq audit`                     | Show the audit log, newest first (admin)                          |

`next` takes the ID: it moves the counter forward, and that ID is never handed
out again, even if you don't use it. `last` and `collections info` only show
`next_id` as a preview; it is not reserved, and another client may take it
first. To retry a failed `next` safely, pass the same `--idempotency-key` to get
the original IDs back instead of new ones.

Every command takes `--url`, `--key` (else `IDBSEQ_URL` / `IDBSEQ_KEY`, else
`~/.config/idbseq/config.toml`), `--json` and `--timeout`. Flags per command and
exit codes: [docs/clients.md](docs/clients.md#command-line-client-idbseq).

### `idbseqd` (server)

| Command                            | Does                                                       |
| ---------------------------------- | ---------------------------------------------------------- |
| `idbseqd` / `idbseqd serve`        | Run the server                                             |
| `idbseqd admin init`               | Create the admin account                                   |
| `idbseqd admin reset-password`     | Set a new admin password and revoke every session          |
| `idbseqd backup <path>`            | Write a consistent snapshot; safe while the server runs    |
| `idbseqd restore <path>`           | Replace the data with a backup; the server must be stopped |
| `idbseqd openapi [--check <file>]` | Print the OpenAPI contract, or check a copy for drift      |
| `idbseqd healthcheck`              | Exit 0 if `/healthz` answers (container health checks)     |
| `idbseqd console <file>`           | Open the admin console on an embedded file, locally        |

Prefer a browser? Start `idbseqd --ui` (or set `ui = true` in `idbseq.toml`) and open
`/ui/` for the built-in [admin console](https://aevantec.github.io/idbseq/guide/console):
collections, API keys, the audit log and server status, signed in as the admin. On a
first start with no admin, `idbseqd --ui` prints a one-time link to choose the admin
password in the browser.

Details: [docs/operations.md](docs/operations.md#commands).

### `idbseq-mcp` (AI agents)

`idbseq-mcp [--allow-create]` serves these tools over stdio. It needs
`IDBSEQ_KEY`, reads `IDBSEQ_URL`, and never offers reset or delete.

| Tool                | Does                                                          |
| ------------------- | ------------------------------------------------------------- |
| `next_id`           | Take the next ID or a batch, with an optional idempotency key |
| `list_collections`  | List the collections the key may use                          |
| `get_collection`    | Show one collection                                           |
| `create_collection` | Create a collection; only with `--allow-create`               |

Setup for Claude Code, Claude Desktop and other clients:
[sdk/mcp/README.md](sdk/mcp/README.md).

## Repository layout

| Path                   | Crate / binary              | Role                                                          |
| ---------------------- | --------------------------- | ------------------------------------------------------------- |
| `crates/idbseq-core`   | `idbseq-core` (library)     | Collection model, validation, sequence engine, storage trait  |
| `crates/idbseq`        | `idbseq` (library)          | Embedded idbseq: one file, no server; JSON interface for bindings |
| `crates/idbseq-node`   | `idbseq-node` (Node module) | Native module behind `IdbSeqDatabase` in the Node.js SDK      |
| `crates/idbseq-server` | `idbseq-server` → `idbseqd` | HTTP API server, auth, admin bootstrap, ops commands          |
| `crates/idbseq-cli`    | `idbseq-cli` → `idbseq`     | Command-line client over the HTTP API                         |
| `sdk/node`             | `idbseq`                    | Official Node.js SDK (npm)                                    |
| `sdk/mcp`              | `idbseq-mcp`                | MCP server for AI agents, on top of the Node SDK (npm)        |
| `examples/`            | —                           | curl, Python, Go and Java clients                             |
| `api/openapi.json`     | —                           | The OpenAPI 3.1 contract; CI fails if it drifts from the code |
| `scripts/e2e.sh`       | —                           | Runs every client end to end against a throwaway server       |
| `docs/`                | —                           | Documentation website (VitePress): guide, API, operations, ADRs |

Business logic lives in `idbseq-core`; the two binaries stay thin. New code goes
in the crate that owns its concern, organised by feature inside the crate.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024);
`rustup` installs it on first use.

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all # format (rustfmt.toml)
cargo lint      # clippy on all targets, warnings are errors
```

Lints are set once in the workspace `Cargo.toml` and inherited by every crate:
`unsafe_code` is forbidden, and clippy's `all` and `pedantic` groups plus
`unwrap_used`, `expect_used` and `dbg_macro` are on (unwrap/expect are allowed in tests).

### CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push to `main`
and on pull requests. Run the same gates locally before pushing:

```bash
cargo fmt --all --check
cargo lint
cargo test --workspace --locked
# needs cargo-llvm-cov
cargo llvm-cov --workspace --locked --fail-under-lines 80
# needs cargo-deny (deny.toml)
cargo deny check
```

| Job        | Runs on       | Fails when                                                  |
| ---------- | ------------- | ----------------------------------------------------------- |
| `check`    | Linux, macOS  | Code isn't formatted, clippy warns, or a test fails         |
| `coverage` | Linux         | Line coverage is under 80%                                  |
| `deny`     | Linux         | A dependency has a RustSec advisory, is yanked, or has a license outside `deny.toml` |

[`.github/workflows/sdks.yml`](.github/workflows/sdks.yml) tests the Node SDK and the MCP
server (Node 20/22/24) with an 80% coverage gate, then runs `scripts/e2e.sh`: the Node SDK,
the MCP server, the CLI, every example and the admin console against a real `idbseqd`. Run it
locally with `scripts/e2e.sh` (or `scripts/e2e.sh node mcp`).

## License

[MIT](LICENSE) © 2026 Aevantec. The Node.js SDK and the MCP server ship under the same license.
