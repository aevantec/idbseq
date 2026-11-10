# idbseq

A Rust ID sequence server. Create named **collections**, each an independent
counter, and ask any of them for the next increasing ID from any language over
HTTP/JSON.

```
POST /v1/collections/orders/next   →  { "success": true, "data": { "id": 1043 }, "error": null }
GET  /v1/collections/orders/last   →  { "success": true, "data": { "last_id": 1043, "next_id": 1044 }, "error": null }
```

- Many collections, each with its own `start`, `step` and `max`
- `generateId`, batch generation, `lastId`, list, reset and wipe
- Durable: an ID is never issued twice, even across crashes
- An admin account protects reset and wipe on **protected** collections
- Official Node.js and PHP SDKs, a CLI, and an OpenAPI contract for every other language

## Status

**Pre-release.** The server, CLI, Node and PHP SDKs work end to end; v0.1.0 is the
first release.

- [HTTP API reference](docs/API.md) and the committed [OpenAPI contract](api/openapi.json)
- [Running idbseqd](docs/operations.md): configuration, backup/restore, health, metrics
- [Releasing](docs/releasing.md): what a `v*` tag publishes and the one-time registry setup
- [Clients](docs/clients.md): Node, PHP, the CLI, and curl/Python/Go/Java examples
- [Product and architecture plan](docs/PLAN.md) and [decision records](docs/adr/README.md)

## Quick start from source

```bash
cargo build --release
IDBSEQ_ADMIN_PASSWORD='choose-a-long-password' ./target/release/idbseqd   # 127.0.0.1:7878, data in ./data

export IDBSEQ_URL=http://127.0.0.1:7878
./target/release/idbseq login                          # stores an admin session
./target/release/idbseq collections create orders --start 1000
./target/release/idbseq next orders                    # 1000
./target/release/idbseq keys create billing --scope generate --collection orders
```

Applications then call `POST /v1/collections/orders/next` with
`Authorization: Bearer <key>`, or use an SDK.

## Quick start with Docker

Each `v*` tag publishes a multi-arch image (linux/amd64, linux/arm64) to GHCR.

```bash
docker volume create idbseq-data
docker run -d --name idbseq -p 7878:7878 \
  -v idbseq-data:/data \
  -e IDBSEQ_ADMIN_PASSWORD='choose-a-long-password' \
  ghcr.io/aevantec/idbseq:latest

curl -s http://127.0.0.1:7878/healthz        # {"success":true,"data":{"status":"ok"},"error":null}
docker inspect --format '{{.State.Health.Status}}' idbseq
```

The image runs `idbseqd` as a non-root user on distroless, listens on
`0.0.0.0:7878` (`IDBSEQ_BIND`), keeps its data in the `/data` volume
(`IDBSEQ_DATA_DIR`) and reports health with `idbseqd healthcheck`.
`IDBSEQ_ADMIN_PASSWORD` creates the admin on first start; remove it afterwards.
The image also contains the `idbseq` CLI:
`docker exec -it idbseq idbseq --help`. Prebuilt binaries for Linux and macOS
(with `SHA256SUMS`) are attached to each GitHub Release. To build the image
yourself: `docker build -t idbseq .`

## Repository layout

| Path                   | Crate / binary              | Role                                                          |
| ---------------------- | --------------------------- | ------------------------------------------------------------- |
| `crates/idbseq-core`   | `idbseq-core` (library)     | Collection model, validation, sequence engine, storage trait  |
| `crates/idbseq-server` | `idbseq-server` → `idbseqd` | HTTP API server, auth, admin bootstrap, ops commands          |
| `crates/idbseq-cli`    | `idbseq-cli` → `idbseq`     | Command-line client over the HTTP API                         |
| `sdk/node`, `sdk/php`  | `idbseq`, `idbseq/idbseq`   | Official SDKs (npm, Composer)                                 |
| `examples/`            | —                           | curl, Python, Go and Java clients                             |
| `api/openapi.json`     | —                           | The OpenAPI 3.1 contract; CI fails if it drifts from the code |
| `scripts/e2e.sh`       | —                           | Runs every client end to end against a throwaway server       |
| `docs/`                | —                           | API reference, operations guide, plan, ADRs                   |

Business logic lives in `idbseq-core`; the two binaries stay thin. New code goes
in the crate that owns its concern, organised by feature inside the crate.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, edition 2024);
`rustup` installs it on first use.

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all            # format (rustfmt.toml)
cargo lint                 # clippy on all targets, warnings are errors
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
cargo llvm-cov --workspace --locked --fail-under-lines 80   # needs cargo-llvm-cov
cargo deny check                                            # needs cargo-deny (deny.toml)
```

| Job        | Runs on       | Fails when                                                  |
| ---------- | ------------- | ----------------------------------------------------------- |
| `check`    | Linux, macOS  | Code isn't formatted, clippy warns, or a test fails         |
| `coverage` | Linux         | Line coverage is under 80%                                  |
| `deny`     | Linux         | A dependency has a RustSec advisory, is yanked, or has a license outside `deny.toml` |

[`.github/workflows/sdks.yml`](.github/workflows/sdks.yml) tests the Node SDK (Node 20/22/24)
and PHP SDK (PHP 8.1/latest) with an 80% coverage gate, then runs `scripts/e2e.sh`:
both SDKs, the CLI and every example against a real `idbseqd`. Run it locally with
`scripts/e2e.sh` (or `scripts/e2e.sh node php`).

## License

[MIT](LICENSE) © 2026 Aevantec. The Node.js and PHP SDKs ship under the same license.
