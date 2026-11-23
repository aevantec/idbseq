# Running idbseqd

`idbseqd` is a single binary holding one data file, `<data_dir>/idbseq.redb`.
Only one process may open a data directory at a time; a second one exits with
`data directory … is already in use by another idbseqd process`.

## First start

```bash
idbseqd admin init # prompts for the admin password (≥ 8 chars)
idbseqd            # serves on 127.0.0.1:7878, data in ./data
```

Alternatively, set `IDBSEQ_ADMIN_PASSWORD` on the first start; it is used only when
no admin exists yet and ignored afterwards, so remove it once the server is up.
Without an admin, login answers `FORBIDDEN` with a hint to run `idbseqd admin init`.

## The admin account

There is exactly one admin, named `admin`: the person who runs the server.
Applications and agents never use it; they use API keys the admin creates.

Only the admin can:

- **manage API keys**: `idbseq keys create`, `keys list`, `keys revoke`. This is
  how every application and agent gets its credentials;
- **look after protected collections**: create them, turn `protected` on or off,
  change their `max`, and reset or delete them. A key with the `manage` scope can
  still reset or delete *unprotected* collections;
- **read the audit log** (`idbseq audit`), which records every destructive and
  admin action;
- **change the admin password**.

An admin session can also do everything a key can: take IDs, read, and create
and manage collections.

- **Password**: at least 8 characters, stored only as an Argon2id hash.
- **Sessions**: `idbseq login` returns a session token valid for 15 minutes;
  `idbseq logout` revokes it. After 5 failed logins in 15 minutes, that client
  address is locked out for 15 minutes; after 50 from all addresses, every login
  is. `idbseqd admin reset-password` ends every lockout. Behind a reverse proxy
  every login comes from the proxy's address, so add rate limiting there.
- **Forgot the password?** On the server host: `idbseqd admin reset-password`.
  It needs access to the data directory, works whether the server is running or
  not, and revokes every session.

Why a separate admin: reset and delete do lasting damage. Keeping them behind a
person's password for protected collections, instead of behind any API key,
means a leaked or over-scoped key cannot wipe the counters that matter. For the
same reason, give the MCP server and other automation an API key, never an
admin session. The design is recorded in
[ADR-0003](adr/0003-auth-model.md).

## Commands

| Command                              | Does                                                                       |
| ------------------------------------ | -------------------------------------------------------------------------- |
| `idbseqd` / `idbseqd serve`          | Run the server                                                             |
| `idbseqd admin init`                 | Create the admin account (`--password-stdin` to pipe the password)         |
| `idbseqd admin reset-password`       | Set a new admin password and revoke all sessions (`--password-stdin`)      |
| `idbseqd backup <path>`              | Write a consistent snapshot; safe while the server runs                    |
| `idbseqd restore <path> [--force]`   | Replace the data file with a backup; the server must be stopped            |
| `idbseqd openapi [--check <file>]`   | Print the OpenAPI 3.1 contract, or fail if `<file>` differs from it        |
| `idbseqd healthcheck`                | Exit 0 if `GET /healthz` answers 200 on the configured address (plain HTTP) |
| `idbseqd console <file> [--port <n>] [--create]` | Open the admin console on an [embedded file](guide/embedded.md#looking-inside-a-file), on 127.0.0.1 only |

The data directory is created with mode 0700 and the data file, backups and
restored files with 0600: they hold the admin password hash and every API key's
hash. An existing directory is tightened on start unless another user owns it.

While the server runs, `admin` and `backup` talk to it over a Unix socket,
`<data_dir>/idbseqd.sock` (mode 0600), so only users who can read the data
directory can use them. Socket paths are limited to about 100 bytes (104 on macOS); with a longer
data directory path the server still starts, logs a warning, and those commands
need the server stopped.

## Configuration

Settings come from the file named by `IDBSEQ_CONFIG`, else `./idbseq.toml` if it
exists, else the defaults below. Then `IDBSEQ_BIND`, `IDBSEQ_DATA_DIR`,
`IDBSEQ_LOG` and `IDBSEQ_UI` (`true`/`false`) override, and `idbseqd --ui` turns
the console on. Unknown keys are an error.

```toml
bind = "127.0.0.1:7878"
data_dir = "data"
log = "info"  # tracing filter, e.g. "idbseq_server=debug"
swagger_ui = false  # serve Swagger UI at /docs (assets from a CDN)
ui = false  # serve the admin console at /ui/ (built in, no CDN)

# [tls]                      # serve HTTPS directly (rustls)
# cert = "/etc/idbseq/cert.pem"
# key = "/etc/idbseq/key.pem"

[limits]
body_bytes = 16384  # larger bodies get 413 PAYLOAD_TOO_LARGE
request_timeout_secs = 10  # slower requests get 408 REQUEST_TIMEOUT
max_batch = 10000  # largest ?count=
idempotency_ttl_hours = 24  # how long Idempotency-Key replays are kept

[rate_limit]  # per credential; per_second = 0 disables it
per_second = 10000
burst = 20000

[auth]
session_ttl_minutes = 15

[group_commit]  # see ADR-0004
max_batch = 1024  # most ID requests sharing one commit
max_wait_us = 0  # extra wait to gather a group; 0 = only what is already queued

[audit]
retention_days = 365

[metrics]
enabled = false  # serve Prometheus/OpenMetrics text at /metrics
```

Binding to anything but loopback without `[tls]` logs a warning: put a TLS
reverse proxy in front, or configure `[tls]`. `idbseqd healthcheck` speaks plain
HTTP, so with `[tls]` use a probe that speaks HTTPS instead.

## Health, metrics and logs

- `GET /healthz`: the process is up. `GET /readyz`: the store is open and the
  server is not shutting down (`503` while draining).
- `GET /metrics` (when enabled): request counts and latency, auth failures, IDs
  issued per collection, and remaining capacity per collection.
- Logs are JSON lines on stdout. Every response carries `X-Request-Id` (yours is
  echoed back if you send one), and the same ID is in the logs and the audit log.

## Shutdown

`SIGTERM` or `SIGINT` stops accepting connections, marks `/readyz` not ready,
lets in-flight requests finish (up to 30 s), flushes buffered key usage, and
exits 0. A `SIGKILL` or power loss is also safe: every ID returned to a caller is
on disk first (or covered by a reserved block), so after a restart the next ID is
always greater than any ID handed out before.

## Backup and restore

```bash
idbseqd backup /backups/idbseq-$(date +%F).redb # while running
```

To restore, stop the server, then:

```bash
idbseqd restore /backups/idbseq-2026-11-01.redb
```

The current data file is kept beside it as `idbseq.redb.pre-restore-<unix time>`.

A backup holds the credentials too: after a restore, API keys, the admin password
and sessions are as they were when it was taken. Revoke again any key revoked
since then (`idbseq keys list`), and reset the admin password if it changed
since. `restore` prints this reminder.

A backup is older than the live data, so its counters are behind: restoring it
as is would hand out IDs that were already issued after the backup was taken.
`restore` therefore refuses when any collection's `last_id` in the backup is lower
than in the current data (or the collection is missing), and lists those
collections. If the current data is lost or you accept the risk, use `--force`, and
then **skip each listed collection ahead** past every ID that may have been issued
since the backup, before serving traffic:

```bash
idbseq reset orders --value 250000 --yes # CLI, or:
curl -X POST https://ids.example.com/v1/collections/orders/reset \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H "Content-Type: application/json" \
  -d '{"confirm": "orders", "value": 250000}'
```

Moving forward never needs `allow_rewind`. Pick a value with a safety margin, for
example the backup's `last_id` plus the peak IDs per second times the time since
the backup.

## Audit log

Creating, changing, resetting and deleting collections, key changes, logins
(including failures) and password changes are recorded with the actor, the
before/after values, the request ID and the client IP. Read them with
`GET /v1/audit?collection=…&action=…` (admin). Entries older than
`audit.retention_days` are removed during periodic maintenance.
