# Getting started

idbseq runs inside your app: add the library, open a file, take IDs. There is
no server to install or run.

## 1. Install

::: code-group

```bash [Node.js]
npm install idbseq
```

```bash [Rust]
cargo add idbseq
```

:::

The Node.js package includes the native module for macOS, Linux and Windows on
x64 and arm64, and works with ESM, CommonJS and TypeScript (Node 20+).

## 2. Open a file and create a collection

A **collection** is one named counter: one per thing you number, such as
orders, invoices or tickets. The file is created on first open; its folder must
exist.

::: code-group

```ts [Node.js]
import { IdbSeqDatabase } from 'idbseq';

const ids = await IdbSeqDatabase.open('app.idbseq');
await ids.createCollection({ name: 'orders', start: 1000 });
```

```rust [Rust]
use idbseq::{CollectionSpec, Database};

let ids = Database::open("app.idbseq")?;
ids.create_collection(CollectionSpec { start: 1000, ..CollectionSpec::named("orders") })?;
```

:::

## 3. Take IDs

::: code-group

```ts [Node.js]
const id = await ids.generateId('orders'); // 1000
const batch = await ids.generateIds('orders', 3); // { first: 1001, last: 1003, count: 3, step: 1 }
const { lastId, nextId } = await ids.lastId('orders'); // 1003, 1004 (only a preview)

await ids.close();
```

```rust [Rust]
let id = ids.next_id("orders")?; // 1000
let batch = ids.next_ids("orders", 3)?; // batch.first 1001, batch.last 1003
let last = ids.last_id("orders")?; // last 1003, next 1004 (only a preview)
```

:::

Taking an ID **consumes** it: it is on disk before you get it and is never
handed out again, even after a crash. Reading the last ID only looks. Restart
the app and the next ID is 1004.

## 4. Share or move the file

- **Several processes on this machine** (a desktop app and its helper, several
  CLI runs): open the file in shared mode, `{ shared: true }` in Node.js or
  `Options::shared()` in Rust. They take turns and never get the same ID.
- **Another machine**: copy the file while it is closed. The copy carries on
  where the original stopped. Copies count on their own, so do not hand out IDs
  from two copies.
- **Several machines on one sequence**: [run a server](./server.md) and move the
  file to it. Your code keeps the same methods.

## Next steps

- [Embedding in depth](./embedded.md): modes, speed, errors and the file.
- Learn the model: [collections](./collections.md), [IDs and gaps](./ids.md),
  [durability](./durability.md) and [safe retries](./retries.md).
- Outgrowing one machine? [Run a server](./server.md).
