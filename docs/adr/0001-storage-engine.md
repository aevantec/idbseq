# ADR-0001: Embedded redb on a single node

- Status: Accepted (provisional; owner may revisit)
- Date: 2026-11-10
- Settled by: ISSUE-0004, ISSUE-0007

## Context

idbseq must never issue an ID twice, including across crashes, and should be one
binary with no external database. v1 does not need high availability.

## Decision

- Store collections, counters, keys, sessions, idempotency records and the audit log
  in one embedded [redb](https://github.com/cberner/redb) file in the data directory.
- Run a single `idbseqd` per data directory; the directory is locked so a second
  process exits with a clear error.
- Keep sequence storage behind the `SequenceStore` trait with a shared
  conformance suite, so another backend (e.g. Postgres) can be added later.
- redb allows one write transaction at a time, so concurrent ID requests are
  **group-committed**: requests waiting for the writer share one transaction and one
  fsync (US-0039). Collections can opt into **block reservation** (US-0035).

## Consequences

- Durability is one fsync per group of requests; throughput depends on the disk's
  fsync latency (see US-0038 benchmarks).
- No HA: availability is the process's and the host's. Backups (US-0033) are the
  recovery path.

## Alternatives considered

- **Postgres / external database:** HA story, but an extra service to run and a network
  round trip per ID.
- **Raft cluster:** HA without a database, but a large project in itself.
- **sled / RocksDB:** sled is unmaintained; RocksDB brings a C++ toolchain.
