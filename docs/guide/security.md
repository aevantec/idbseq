# Admin, API keys and scopes

Every `/v1` call needs a bearer token, sent as `Authorization: Bearer <token>`.
There is no anonymous access: taking an ID uses it up for good, so an open
server would let anyone burn through a collection. The token is either an
**API key** (for applications and agents) or an **admin session** (for the one
person who runs the server).

Only `GET /healthz`, `GET /readyz`, `GET /metrics` (when enabled) and
`GET /v1/openapi.json` work without credentials.

## The admin account

There is exactly one admin, named `admin`. Only the admin can:

- **manage API keys**: create, list and revoke them. This is how every
  application and agent gets its credentials;
- **look after [protected collections](./collections.md#protected-collections)**:
  create them, change their settings, reset or delete them;
- **read the [audit log](#audit-log)**;
- **change the admin password**.

An admin session can also do everything a key can.

| Topic             | How it works                                                                     |
| ----------------- | -------------------------------------------------------------------------------- |
| Creating it       | `idbseqd admin init` on the server host, `IDBSEQ_ADMIN_PASSWORD` on first start, or the one-time [console setup link](./console.md#first-run-setup) |
| Password          | At least 8 characters, stored only as an Argon2id hash                           |
| Signing in        | `idbseq login` (or `POST /v1/auth/login`) returns a session token                |
| Session length    | 15 minutes (`auth.session_ttl_minutes`); `POST /v1/auth/refresh` restarts the 15 minutes, never past 12 hours after sign-in; `idbseq logout` revokes it early |
| Guessing          | 5 failed logins in 15 minutes lock out that client address for 15 minutes; 50 from all addresses lock out everyone. A password change or reset ends every lockout |
| Changing it       | `POST /v1/admin/password`; revokes every session                                 |
| Forgotten         | `idbseqd admin reset-password` on the server host; revokes every session         |

`reset-password` needs access to the data directory, so recovering the account
needs access to the machine, not just the network.

## API keys

An API key looks like `idb_k7f3a9c2_<secret>`. The admin creates it, and the full
key is shown **only once**; the server keeps just a hash of the secret.

```bash
idbseq login
idbseq keys create billing --scope generate --collection orders
idbseq keys list               # names, scopes, last use; no secrets
idbseq keys revoke k7f3a9c2    # takes effect immediately
```

Store keys like passwords: in a secret manager or an environment variable, never
in source code or in browser JavaScript. Give every application its own key, so
you can see its use (`last_used_at`) and revoke it alone.

## API key scopes

A scope is a permission. A key holds one or more, and the server checks them on
every call; a call outside the key's scopes gets `403 FORBIDDEN`.

| Scope      | Allows                                                       | CLI                                               | MCP tools                            |
| ---------- | ------------------------------------------------------------ | ------------------------------------------------- | ------------------------------------ |
| `read`     | List collections, show one, read its last and next ID        | `collections list`, `collections info`, `last`    | `list_collections`, `get_collection` |
| `generate` | Take IDs, one at a time or in batches                        | `next`                                            | `next_id`                            |
| `manage`   | Create, change, reset and delete **unprotected** collections | `collections create/update/delete`, `reset`       | `create_collection`                  |

- Scopes are independent: `generate` does not include `read`.
- No scope covers API keys or the audit log; those stay with the admin.
- `manage` never reaches protected collections; those operations need the admin
  (`403 PROTECTED_COLLECTION`).

## Collection allow-lists

A key can be limited to certain collections, by exact name or by a prefix ending
in `*`:

```bash
idbseq keys create eu-billing --scope generate,read --collection 'eu.*' --collection orders
```

The allow-list applies to every scope. Lists only show allowed collections, and
anything else gets `403 FORBIDDEN`. Without an allow-list, the key works on every
collection.

## Choosing scopes

Give each key the fewest scopes and the narrowest allow-list it needs. A leaked
key can then only do that much, to those collections.

| Who                                | Scopes                 | Example                                                                      |
| ---------------------------------- | ---------------------- | ---------------------------------------------------------------------------- |
| A service that only needs IDs      | `generate`             | `idbseq keys create billing --scope generate --collection orders`            |
| An AI agent through the MCP server | `generate`, `read`     | `idbseq keys create my-agent --scope generate,read --collection tickets`     |
| A dashboard or monitoring          | `read`                 | `idbseq keys create grafana --scope read`                                    |
| Provisioning or setup scripts      | `manage` (plus `read`) | `idbseq keys create provisioner --scope manage,read --collection 'tenant.*'` |

Never hand an admin session to an application or to the MCP server. Sessions
expire after 15 minutes anyway, and they can do everything.

## Audit log

Every change that matters is recorded with who did it, the values before and
after, the request ID and the client IP:

- creating, changing, resetting and deleting collections;
- creating and revoking API keys;
- logins, including failed ones;
- the admin's creation and password changes.

```bash
idbseq audit --collection orders
idbseq audit --action collection.reset
```

Entries are kept for 365 days (`audit.retention_days`). Taking IDs is not
audited; use the [metrics](./monitoring.md#metrics) for that.

## Network and TLS

- The server listens on `127.0.0.1:7878` by default, reachable only from the
  same machine.
- To serve other machines, either put it behind a TLS reverse proxy or set the
  `[tls]` section to serve HTTPS directly. Binding to a non-loopback address
  without `[tls]` logs a warning, because keys would cross the network in plain
  text. See [Deployment](./deployment.md).
- The server sends no CORS headers, so browsers cannot call it directly. That is
  deliberate: a key in browser code is visible to everyone. Call idbseq from your
  backend. The optional [admin console](./console.md) is served by the server
  itself, so it needs no CORS either.
- Request bodies are limited to 16 KiB and requests to 10 s by default, and every
  credential is [rate limited](./retries.md#rate-limiting).

The design is recorded in [ADR-0003](../adr/0003-auth-model.md). To report a
vulnerability, see the repository's
[security policy](https://github.com/aevantec/idbseq/blob/main/SECURITY.md).
