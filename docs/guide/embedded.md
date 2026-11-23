# Embedding in depth

idbseq runs **inside your app**, like SQLite: its data in one file, with no
server, no network and no API keys. It works offline, and the file can be copied
to another machine or served later by `idbseqd`. New here? Start with
[Getting started](./getting-started.md).

```ts
import { IdbSeqDatabase } from 'idbseq';

const ids = await IdbSeqDatabase.open('app.idbseq');
await ids.createCollection({ name: 'invoices', start: 1000 });

const id = await ids.generateId('invoices'); // 1000
const batch = await ids.generateIds('invoices', 10); // 1001–1010

await ids.close();
```

Every ID is on disk before it is returned, so an embedded file keeps the same
promise as the server: an ID is never issued twice, even after a crash.

## Embedded or server?

| Who takes IDs                                   | Use                                                  |
| ----------------------------------------------- | ---------------------------------------------------- |
| One app: a desktop app, a CLI tool, one service | Embedded                                             |
| Several processes on one machine                | Embedded, in [shared mode](#modes)                   |
| Several machines, devices or users              | [A server](./server.md)                              |

An embedded file only knows about the IDs issued from it. Two copies of a file,
for example one per laptop synced through Dropbox or git, count on their own and
**will hand out the same IDs**. When several machines need one sequence, run
`idbseqd` and point them all at it.

## Node.js

`IdbSeqDatabase` is in the same `idbseq` package as the HTTP client, with the
same method names, return types and error classes, minus the admin methods:

| Method                                   | Returns                                          |
| ---------------------------------------- | ------------------------------------------------ |
| `IdbSeqDatabase.open(path, options?)`    | An open database; creates the file if needed     |
| `generateId(name, { idempotencyKey? })`  | The next ID                                      |
| `generateIds(name, count, { idempotencyKey? })` | `{ first, last, count, step }`            |
| `lastId(name)`                           | `{ lastId, nextId }`, without issuing anything   |
| `createCollection`, `getCollection`, `listCollections`, `updateCollection`, `resetCollection`, `deleteCollection` | As on [`IdbSeqClient`](../clients/node.md) |
| `close()`                                | Waits for running calls, then lets go of the file |

Code written against the `Sequences` type works with either, so an app can
start embedded and move to a server by changing one line:

```ts
import { IdbSeqClient, IdbSeqDatabase, type Sequences } from 'idbseq';

const ids: Sequences = process.env.IDBSEQ_URL
  ? new IdbSeqClient({ url: process.env.IDBSEQ_URL, apiKey: process.env.IDBSEQ_KEY })
  : await IdbSeqDatabase.open('app.idbseq');
```

Calls run on threads of their own, never on the event loop and never on
Node's small thread pool, so a call waiting for a busy file does not hold up
the app's file reads or crypto. Once `close()` resolves, the file is free:
another handle or process can open it straight away. The package includes the
native module for macOS, Linux (glibc and musl) and Windows on x64 and arm64.
In Electron, open the database in the main process.

Errors:

| Error                      | When                                                         |
| -------------------------- | ------------------------------------------------------------ |
| `DatabaseBusyError`        | Another process held the file for the whole busy timeout     |
| `DatabaseUnavailableError` | The file cannot be opened or created: missing folder, no permission, not an idbseq file, deleted or moved while open |
| `DatabaseClosedError`      | A call made after `close()`                                   |
| `InternalError`            | A stored record is damaged (see [The file](#the-file))        |
| `CollectionNotFoundError`, `SequenceExhaustedError`, `ValidationFailedError`, … | As from the server |

## Rust

The [`idbseq`](https://github.com/aevantec/idbseq/tree/main/crates/idbseq) crate
has the same calls, synchronous:

```rust
use idbseq::{CollectionSpec, Database, Options};

let ids = Database::open("app.idbseq")?;
ids.create_collection(CollectionSpec::named("invoices"))?;
let id = ids.next_id("invoices")?;
let batch = ids.next_ids("invoices", 10)?; // batch.first ..= batch.last

// Several processes on one file:
let shared = Database::open_with("app.idbseq", Options::shared())?;
```

A `Database` can be shared between threads; calls from several threads at once
share disk writes.

## Modes

| Mode                  | Who can use the file                                         | Cost of one ID¹ |
| --------------------- | ------------------------------------------------------------ | --------------- |
| Exclusive (default)   | This handle, until `close()`; other processes wait, then get `DatabaseBusyError` | ~5 ms |
| Shared (`shared: true`) | Every process with a handle, in turn: each call opens the file, writes and closes it | ~25 ms |

¹ One ID at a time on a Mac with an SSD, where every write is a full disk
flush. Linux and Windows are usually faster. A batch costs about the same as
one ID, and in exclusive mode calls made at the same time share a write.

A call waits up to the **busy timeout** (`busyTimeoutMs`, default 5,000) while
the file is busy, counted from when the call was made: calls queued behind
others in the same process share it, they do not each get a new one. Need IDs faster? Take them in batches, or use
[block durability](./durability.md) in exclusive mode: it issues IDs from
memory and writes once per block. In shared mode, block collections are written
like strict ones, because a reserved block would be lost at every close.

## The file

- **Portable**: the format is the same on every OS. Copy the file while no
  handle has it open, and the copy carries on where the original stopped.
- **Private**: it is created readable by its owner only (mode 0600 on Linux and
  macOS). Permissions you set later are kept, for example to share it with a
  group.
- **Never replaced**: a shared handle remembers the file by its full path. If
  the file is deleted or moved while the app runs, calls fail with
  `DatabaseUnavailableError`; idbseq never creates an empty file in its place,
  whose counters would start over.
- **Checked**: every record carries a checksum. A damaged record is refused
  with an error instead of being read, so a counter cannot silently go back
  and issue IDs again. Restore the file from a backup. (The checksum catches
  accidents such as a bad disk or copy, not deliberate edits.)
- **Only idbseq files**: a redb file from another program is refused, not
  changed.
- **Local disks only**: keep the file on a disk of the machine. Network and
  some virtual filesystems (NFS, SMB, sshfs and other FUSE mounts) may not
  lock files, and then two processes could hold the file at once and issue
  the same IDs. Several machines need the [server](./server.md).
- **Moving to a server**: stop the app, copy the file to the server's data
  directory as `idbseq.redb`, and start `idbseqd`. Collections and counters
  carry over; create the admin as on any [first start](../operations.md#first-start).
- **Backups**: copy the file while it is closed.

## Looking inside a file

`idbseqd console` opens the [admin console](./console.md) on a file, in your
browser, without running a server:

```bash
idbseqd console app.idbseq
# idbseqd console: app.idbseq
#
# Open this link (it signs you in; keep it private):
#
# http://127.0.0.1:52817/ui/#/local?token=…&file=app.idbseq
```

Open the link to list, create and change collections, take IDs and copy
ready-to-paste `IdbSeqDatabase` code. Press Ctrl-C to stop.

- **The app keeps working.** The console opens the file for each request, like
  [shared mode](#modes), so an app in shared mode takes IDs in between, and no
  ID is ever issued twice. An app in exclusive mode holds the file, so the
  console waits for it and then shows `DATABASE_BUSY`; close the app's handle
  first.
- **This machine only.** It listens on `127.0.0.1` and accepts only the token in
  the link, which is new every time. There is no password, so anyone who can
  read the link can use the console until you stop it.
- **Nothing extra in the file.** The console adds no admin, keys or audit
  entries to the file; it shows collections and status only.
- `--port <n>` picks the port (default: any free one). `--create` makes a new
  file; without it a missing file is an error, so a typo does not create one.
- **Reloading the page signs you out**, as on a server: open the printed link
  again. It works until you stop the command.

`idbseqd` comes with the [server download](./server.md#_1-install); you do not
need to run a server to use it.

## What only the server does

API keys, the admin account, metrics and the audit log are server
features. A collection's `protected` flag is kept in the file but has no effect
until a server serves it. [Idempotency keys](./retries.md) work the same way
(1–255 visible ASCII characters): repeating a call with the same key within
24 hours returns the same IDs. Keys used embedded and keys sent to a server
that later serves the file are kept apart.
