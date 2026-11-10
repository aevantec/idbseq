# ADR-0004: Gaps allowed, duplicates never

- Status: Accepted (provisional; owner may revisit)
- Date: 2026-11-10
- Settled by: ISSUE-0001

## Context

A sequence can skip numbers when a client times out after the server issued an ID,
when a batch is reserved but not used, or when block reservation is on and the
server crashes. Preventing all gaps needs a reserve → confirm protocol.

## Decision

idbseq guarantees **uniqueness and increasing order**, not gaplessness:

- Every ID in `strict` mode is committed to disk before it is returned.
- A lost response burns that number. Clients that retry send an `Idempotency-Key`
  so the retry returns the original IDs (US-0020).
- In `block` mode the unused rest of a block is skipped after a crash.

## Consequences

- Simple, fast, and safe for primary keys and references.
- Not suitable on its own for legally gapless numbering (e.g. invoice numbers in some
  jurisdictions); do that inside the consumer's own database transaction.

## Alternatives considered

- **Gapless two-phase protocol:** much slower and more complex, and a client that
  never confirms still blocks the sequence.
