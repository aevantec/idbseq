# What is idbseq?

idbseq is an **embeddable ID sequence generator**. Like SQLite, it runs inside
your app with its data in one portable file: no server, no network, nothing to
install or run. You create named **collections**, each an independent counter,
and take the next ID:

```ts
const ids = await IdbSeqDatabase.open('app.idbseq');
const id = await ids.generateId('orders'); // 1043
```

It is a Rust library, with a Node.js package on top. When several machines need
one sequence, the same engine runs as a small server, `idbseqd`, and your file
can move to it.

## What it guarantees

- **Unique**: an ID is never handed out twice, even after a crash, a power loss
  or a restart. Every ID is written to disk before it is returned.
- **Increasing**: each ID of a collection is greater than every ID handed out
  before it.
- **Not gapless**: a number can be skipped, for example when a reserved batch
  is not used or a response is lost on the network. Gaps are normal; see
  [IDs, order and gaps](./ids.md).

## When to use it

- You want **short, readable, ordered numbers**: order 1043, ticket 88, invoice
  2026-000412, instead of a UUID.
- A **desktop app, CLI tool, offline app or single service** needs numbers that
  survive restarts, without running a database.
- **Several processes on one machine** take numbers from the same sequence: they
  [share the file](./embedded.md#modes).
- **Several services or machines** need numbers from one sequence without
  coordinating: [run the server](./server.md).
- AI agents or scripts create records in parallel and need IDs that never collide.

## When not to use it

- **Legally gapless numbering** (some invoice rules require no missing numbers).
  idbseq can skip numbers; do that numbering inside your own database
  transaction. See [ADR-0004](../adr/0004-gap-policy.md).
- **IDs generated offline on many devices** that must never collide. Each copy
  of a file counts on its own, so two devices would issue the same numbers; use
  UUIDv7 or ULIDs, which need no coordination.
- **High availability across several nodes.** The server runs as a single
  node; it is durable, not replicated. See [ADR-0001](../adr/0001-storage-engine.md).

## How it fits together

```
 Embedded: in your app                    Server: shared over the network
 ┌──────────────────────────────┐         ┌───────────┐ ┌───────────┐ ┌──────────┐
 │ your app                     │         │ Node SDK  │ │ CLI, MCP  │ │ curl, …  │
 │ IdbSeqDatabase (Node.js)     │         └─────┬─────┘ └─────┬─────┘ └────┬─────┘
 │ or the idbseq crate (Rust)   │               └──── HTTP/JSON ───────────┘
 └──────────────┬───────────────┘                            │
                │                               ┌────────────▼─────────────┐
                │                               │ idbseqd                  │
                │                               │ API keys, admin, console │
                │                               └────────────┬─────────────┘
        ┌───────▼────────┐    copy the file     ┌────────────▼─────────────┐
        │ app.idbseq     │ ───────────────────▶ │ idbseq.redb              │
        └────────────────┘                      └──────────────────────────┘
```

- **Embedded**: the [`idbseq` crate](./embedded.md#rust) or
  [`IdbSeqDatabase`](./embedded.md#node-js) opens the file and hands out IDs in
  your process. Several processes can [share the file](./embedded.md#modes).
- **Server**: [`idbseqd`](./server.md) serves the same engine over HTTP/JSON.
  The [Node.js SDK](../clients/node.md), the [`idbseq` CLI](../clients/cli.md)
  and the [MCP server](../clients/mcp.md) are official; [any other language](../clients.md)
  works with plain HTTP or a client generated from the
  [OpenAPI contract](/api-explorer.html){target="_self"}. People sign in as the
  admin; applications use API keys with only the permissions they need. See
  [Admin, API keys and scopes](./security.md).
- **The file** has the same format everywhere: an embedded file can be copied to
  another machine or served by `idbseqd`.

## Versions

The Rust crate, the Node.js package, the server, the CLI and the MCP server
are released together under one version number, listed on
[GitHub Releases](https://github.com/aevantec/idbseq/releases). The HTTP API is
versioned separately, under `/v1`.

Next: [Getting started](./getting-started.md).
