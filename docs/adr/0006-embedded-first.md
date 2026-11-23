# ADR-0006: Embedded first, the server optional

- Status: Accepted
- Date: 2026-11-18
- Settled by: US-0052

## Context

idbseq was planned as an embeddable ID sequence generator, but was built
server first: `idbseqd` over HTTP, with the engine in `idbseq-core`. Most uses
are one app on one machine (a desktop app, a CLI tool, one service), where
running and securing a server is more work than the problem. The owner set the
direction on 2026-11-18: idbseq's main purpose is to be portable and embeddable,
with no server.

## Decision

- idbseq is first a **library**: the `idbseq` Rust crate and `IdbSeqDatabase`
  in the `idbseq` npm package open one file and issue IDs in the app's process.
- The file is the unit of portability: one format on every OS, copied while
  closed, and readable by `idbseqd` as its data file.
- Two modes: **exclusive** (a handle holds the file; fastest) and **shared**
  (each call opens, writes and closes the file, so processes on one machine
  take turns, waiting up to a busy timeout).
- The **server** is how several machines share one sequence. It keeps API keys,
  the admin, the console and the HTTP API; the docs present it second.
- Bindings speak the HTTP API's JSON (`idbseq::json`), so every language gets
  the same field names, error codes and client types for both.

## Consequences

- Easier: no install or credentials for the common case; offline use; one code
  path from prototype to server (`Sequences` in Node.js).
- Harder: native modules must be built and published per platform (US-0053);
  each new language needs a binding.
- Watch for: copies of one file count independently, so the docs must keep
  saying that several machines need the server. Shared mode costs a file open
  and close per call (about 25 ms on a Mac).

## Alternatives considered

- **Server only.** Every user runs and secures a process for what is often one
  app's counter.
- **A separate embedded product.** Two engines would drift; one engine in both
  keeps the guarantees identical.
- **SQLite as the store.** It would give multi-process access for free, but
  redb is already the tested store, and a second backend doubles the work.
