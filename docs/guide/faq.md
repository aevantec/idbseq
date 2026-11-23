# FAQ

## Do I need a server?

No. idbseq runs [inside your app](./getting-started.md), like SQLite, with its
data in one file: `IdbSeqDatabase` in Node.js, or the `idbseq` Rust crate.
Several processes on one machine can [share the file](./embedded.md#modes).
Several machines cannot: each copy of a file counts on its own, so they need
[a server](./server.md).

## Does `next` take the ID, or only show it?

It takes it. `next` hands you the ID for good and moves the counter on; nobody
else will ever get it, even if you never use it. `last` and `collections info`
only show `next_id` as a preview, which another client may take first. See
[Taking an ID versus looking at one](./ids.md#taking-an-id-versus-looking-at-one).

## Why are there gaps in my IDs?

A number is skipped when a response is lost and the request is repeated without
an idempotency key, when a taken ID or batch is not used, after a crash in block
mode, or after a forward reset. Gaps are normal and never mean a problem; never
fill them. See [Gaps are normal](./ids.md#gaps-are-normal).

## Can idbseq give me gapless invoice numbers?

Not on its own. It guarantees unique and increasing, not gapless. Assign legally
gapless numbers inside your own database transaction.

## Can two callers get the same ID?

No. Every ID is on disk before it is returned, so even after a crash the server
continues above the last ID handed out. The only ways to repeat an ID are
deliberate: a reset with `allow_rewind`, deleting and recreating a collection,
or force-restoring an old backup without skipping ahead.

## Why do I need an API key? Can I turn auth off?

No. Taking an ID uses it up for good, so an open server would let anyone burn
through or exhaust a collection. Give each application a key with only the
[scopes](./security.md#api-key-scopes) it needs, limited to its collections.

## What is the admin password for?

For the one person who runs the server: creating and revoking API keys, managing
protected collections and reading the audit log. Applications never use it. See
[The admin account](./security.md#the-admin-account).

## I forgot the admin password.

On the server host, run `idbseqd admin reset-password`. It needs access to the
data directory and signs out every session.

## Can a browser call idbseq directly?

No. The server sends no CORS headers, on purpose: an API key in browser code is
visible to anyone. Call idbseq from your backend and pass the ID to the page.

## How fast is it?

Embedded, one ID at a time with a full disk flush each: about 5 ms in exclusive
mode and 25 ms in shared mode on a Mac; batches and block mode are far faster.
See [Modes](./embedded.md#modes).

Server, on the reference laptop: about 6,800 requests/s with durable commits and 64
callers, about 100,000 requests/s in block mode, and over 600,000 IDs/s with
batches of 100. A Linux server with a fast NVMe drive is usually quicker. See
[Durability and speed](./durability.md).

## Can I run several servers for high availability?

Not in v1. Run one `idbseqd` per data directory; two servers on copies of the
data would hand out the same IDs. Restart quickly on failure and keep
[backups](../operations.md#backup-and-restore). See [Deployment](./deployment.md).

## How do I back up and restore?

`idbseqd backup <file>` writes a consistent snapshot while the server runs.
`idbseqd restore <file>` (server stopped) refuses to move counters backwards
unless you pass `--force`; then skip each collection ahead with a forward
reset. See [Backup and restore](../operations.md#backup-and-restore).

## Which languages are supported?

Embedded, in your app: **Rust** (the `idbseq` crate) and **Node.js**
(`IdbSeqDatabase`). Through a server: the [Node.js SDK](../clients/node.md), the
[command line](../clients/cli.md) and the [MCP server](../clients/mcp.md) for AI
agents, and every other language over plain HTTP; there are tested examples for
[curl, Python, Go and Java](../clients.md), and you can generate a client from the
[OpenAPI contract](/api-explorer.html){target="_self"}.

## Why `/healthz` and not `/health`?

It is a convention from Google's z-pages, made common by Kubernetes. The `z`
avoids clashes with real application routes. See
[Health checks and monitoring](./monitoring.md).

## Are IDs strings or numbers?

Numbers: signed 64-bit integers sent as JSON numbers. JavaScript needs care
above 2^53; the Node.js SDK handles it. See [What an ID is](./ids.md#what-an-id-is).
