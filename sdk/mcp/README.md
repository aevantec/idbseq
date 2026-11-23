# idbseq-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI agents take unique,
increasing IDs from an [idbseq](https://github.com/aevantec/idbseq) server in one
tool call. Several agents working at once (writing tickets, records, files named
by number) never get the same ID.

Full documentation: [aevantec.github.io/idbseq/](https://aevantec.github.io/idbseq/).

It runs next to the agent over stdio and talks to `idbseqd` through the official
Node SDK. `idbseqd` itself needs no change.

## Tools

| Tool                | Does                                                          | Key scope  |
| ------------------- | ------------------------------------------------------------- | ---------- |
| `next_id`           | Reserves the next ID, or `count` IDs at once; optional `idempotency_key` | `generate` |
| `list_collections`  | Lists the collections the key may use, with `last_id` and `next_id` | `read` |
| `get_collection`    | Shows one collection (reading reserves nothing)               | `read`     |
| `create_collection` | Only with `--allow-create`; strict durability unless `block_size` is set | `manage` |

Reset and delete are never offered. Results use the API's wire names
(`last_id`, `next_cursor`) and write 64-bit IDs exactly. A failed call comes back
as a tool error carrying the API error, for example
`{"code":"SEQUENCE_EXHAUSTED","message":"…","details":null}`, so the agent can act on it.

The server tells every agent the rules: IDs are never reused, gaps are normal,
collection names are lowercase, and a retry should send the same
`idempotency_key` to get the same IDs back.

## Why it needs an API key

Every `/v1` call to idbseq needs a bearer token; there is no anonymous mode. Taking an
ID uses it up for good, so an open endpoint would let anyone burn through a
collection. The key is also what limits the agent: give it only what it needs.

```bash
# As the admin (after `idbseq login`):
idbseq collections create tickets
idbseq keys create my-agent --scope generate --scope read --collection tickets
```

- `generate` lets it take IDs, `read` lets it list and show collections. Leave out
  `manage` unless you start the server with `--allow-create`.
- `--collection` limits the key to those collections (exact names or `prefix*`).
- Use an API key, not an admin session token: those expire after 15 minutes and
  may do everything. The server warns when `IDBSEQ_KEY` does not look like a key.
- The key stays in the MCP server's environment; the model never sees it.

## Configuration

| Variable     | Meaning                                              |
| ------------ | ---------------------------------------------------- |
| `IDBSEQ_KEY` | API key, **required**                                |
| `IDBSEQ_URL` | Server base URL, default `http://127.0.0.1:7878`     |

On start the server checks that idbseq answers and accepts the key. If not, it
exits with a message on stderr (shown in your MCP client's logs) instead of
failing on the first tool call.

### Claude Code

```bash
claude mcp add idbseq \
  --env IDBSEQ_URL=http://127.0.0.1:7878 \
  --env IDBSEQ_KEY=idb_... \
  -- npx -y idbseq-mcp
```

To share it with a project without committing the key, put this in `.mcp.json`;
Claude Code fills in `${IDBSEQ_KEY}` from each developer's environment:

```json
{
  "mcpServers": {
    "idbseq": {
      "command": "npx",
      "args": ["-y", "idbseq-mcp"],
      "env": {
        "IDBSEQ_URL": "http://127.0.0.1:7878",
        "IDBSEQ_KEY": "${IDBSEQ_KEY}"
      }
    }
  }
}
```

### Claude Desktop and other clients

Any client that starts stdio servers takes the same command and environment. For
Claude Desktop, add it to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "idbseq": {
      "command": "npx",
      "args": ["-y", "idbseq-mcp"],
      "env": { "IDBSEQ_URL": "http://127.0.0.1:7878", "IDBSEQ_KEY": "idb_..." }
    }
  }
}
```

Add `"--allow-create"` to `args` to offer `create_collection`.

### From a checkout

```bash
(cd sdk/node && npm ci && npm run build)
cd sdk/mcp && npm ci && npm run build
claude mcp add idbseq --env IDBSEQ_KEY=idb_... -- node "$PWD/dist/cli.js"
```

## Development

```bash
npm run typecheck
npm test # unit tests over a real MCP client, 80% coverage gate
# From the repository root: the built binary against a real idbseqd
scripts/e2e.sh mcp
```

## License

[MIT](../../LICENSE)
