# idbseq (CLI)

Command-line client for the idbseq HTTP API ([`docs/API.md`](../../docs/API.md)).
Binary name: `idbseq`. Full documentation: [aevantec.github.io/idbseq/](https://aevantec.github.io/idbseq/).

```sh
export IDBSEQ_URL=http://127.0.0.1:7878 IDBSEQ_KEY=idb_k7f3a9c2_…

idbseq next orders             # 1044
idbseq next orders --count 100 # 1045..1144 (count 100, step 1)
idbseq last orders
idbseq collections list --prefix eu. --all
idbseq collections create invoices --start 1000 --step 10
idbseq --json collections info invoices

idbseq login # admin; password prompt, or first line of stdin
idbseq keys create billing --scope read,generate --collection 'eu.*'
idbseq collections create tickets --protected
idbseq reset tickets --value 1 --allow-rewind
idbseq audit --collection tickets
idbseq logout
```

## Commands

| Command | API call |
| ------- | -------- |
| `collections list [--limit N] [--cursor C] [--prefix P] [--all]` | `GET /v1/collections` (`--all` follows cursors) |
| `collections create NAME [--start] [--step] [--max] [--protected] [--durability strict\|block --block-size N]` | `POST /v1/collections` |
| `collections info NAME` | `GET /v1/collections/{name}` |
| `collections update NAME [--max] [--start] [--step] [--protected true\|false] [--durability …]` | `PATCH /v1/collections/{name}` |
| `collections delete NAME [--yes]` | `DELETE /v1/collections/{name}?confirm={name}` |
| `next NAME [--count N] [--idempotency-key K]` | `POST /v1/collections/{name}/next` |
| `last NAME` | `GET /v1/collections/{name}/last` |
| `reset NAME [--value N] [--allow-rewind] [--yes]` | `POST /v1/collections/{name}/reset` |
| `keys create NAME --scope S[,S] [--collection C[,C]]`, `keys list`, `keys revoke ID [--yes]` | `/v1/keys` |
| `login [--username admin]`, `logout` | `/v1/auth/login`, `/v1/auth/logout` |
| `audit [--limit] [--cursor] [--collection] [--action]` | `GET /v1/audit` |

Global flags: `--url`, `--key`, `--json`, `--timeout SECONDS` (default 30).

- **Output**: human-readable text by default; `--json` prints the response `data`
  as JSON (errors go to stderr as `{ "code", "message", … }`).
- **Confirmation**: `collections delete`, `reset` and `keys revoke` ask you to type
  the collection name or key id; `--yes` / `-y` skips the prompt. Delete and reset
  send `confirm=<name>` themselves after confirmation.

## Server and credentials

The URL is taken from `--url`, else `IDBSEQ_URL`, else `url` in the config file,
else `http://127.0.0.1:7878`. The credential is `--key`, else `IDBSEQ_KEY`, else
the stored admin session for that same URL, else `key` in the config file.

Config directory: `$IDBSEQ_CONFIG_DIR`, else `$XDG_CONFIG_HOME/idbseq`, else
`~/.config/idbseq`.

```toml
# ~/.config/idbseq/config.toml
url = "https://ids.internal:7878"
key = "idb_k7f3a9c2_…"
```

`idbseq login` stores the admin session token in `session.toml` in that directory
(directory 0700, file 0600); `idbseq logout` revokes it on the server and deletes
the file.

HTTPS uses rustls (ring provider) and the operating system's trusted root
certificates.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | success |
| 1 | other errors: `INTERNAL`, unknown error codes, unexpected responses, local I/O |
| 2 | invalid arguments or configuration |
| 3 | network error: server unreachable or timed out |
| 4 | `UNAUTHORIZED` |
| 5 | `FORBIDDEN` |
| 6 | `PROTECTED_COLLECTION` |
| 7 | `COLLECTION_NOT_FOUND`, `NOT_FOUND` |
| 8 | `COLLECTION_EXISTS` |
| 9 | `SEQUENCE_EXHAUSTED` |
| 10 | `VALIDATION_FAILED`, `PAYLOAD_TOO_LARGE` |
| 11 | `RATE_LIMITED` |
| 12 | aborted: confirmation declined |

`idbseq --help` prints the same table.
