# ADR-0005: Throughput and latency targets per durability mode

- Status: Accepted (2026-11-10, by the owner; numbers from one laptop, to be refreshed with a Linux server run)
- Date: 2026-11-10
- Settled by: US-0038

## Context

The early aspiration was 500k–1M requests/s on one node. US-0038 asked for the
target to be set from measurements on real hardware instead. The measurements
are in [`benches/baseline.md`](../../benches/baseline.md); the method is in
[docs/benchmarks.md](../benchmarks.md).

What the numbers show on the reference laptop:

- **Durability cost dominates everything.** redb makes a commit durable with
  `File::sync_data`, which on macOS is `fcntl(F_FULLFSYNC)`: about 3.3–3.9 ms
  median, 11.7 ms p99 on this SSD. A strict commit costs about 5 ms, so one
  caller gets about 180–200 requests/s whatever the engine does.
- **Group commit** (US-0039) shares one commit between all queued requests:
  6.8k requests/s at 64 callers versus 185 with one commit per request (36.7×).
  Each request still waits for one or two commits, so p50 is about 9 ms and p99
  about 23 ms.
- **Block mode** (US-0035, size 1000) serves from memory between reservations:
  66k requests/s with 1 caller and about 100k with 16–64. p99 grows with
  callers (5.9 ms at 64) because every caller waits for a block refill.
- **Batches** amortise everything: strict group commit with `count = 100` gives
  668k IDs/s at 64 callers.
- **The in-memory store** (no durability) reaches 2–10M requests/s, which is the
  ceiling of the engine and locking, not of idbseq as deployed.
- **HTTP** adds little to strict mode, where the commit dominates: 6.5k
  requests/s at 64 callers, p99 28 ms. Block mode reaches about 39k requests/s
  over loopback, with the load generator sharing the same 10 cores.

The 500k–1M requests/s aspiration is not reachable with durable commits on this
hardware, nor by a single HTTP server process here. It is reachable in IDs/s
with batches.

## Decision

**Reference hardware** (the "laptop" of US-0017): Apple M1 Pro, 10 cores, 16 GB,
internal Apple SSD, APFS, macOS. Durable commits use `F_FULLFSYNC`. Targets for
a Linux server reference are not set until that machine is measured.

Targets on the reference hardware. Each is a floor for throughput and a
ceiling for latency. A change that misses one is a regression unless this ADR
is revised.

| Mode, load | Requests/s | IDs/s | p50 | p99 |
| --- | ---: | ---: | ---: | ---: |
| strict, 1 caller (store) | ≥ 150 | ≥ 150 | ≤ 8 ms | ≤ 20 ms |
| strict + group commit, 64 callers (store) | ≥ 5,000 | ≥ 5,000 | ≤ 15 ms | ≤ 30 ms |
| strict + group commit, 64 callers, `count = 100` (store) | ≥ 5,000 | ≥ 500,000 | ≤ 15 ms | ≤ 30 ms |
| block (size 1000), 1 caller (store) | ≥ 50,000 | ≥ 50,000 | ≤ 0.1 ms | ≤ 1 ms |
| block (size 1000), 16–64 callers (store) | ≥ 80,000 | ≥ 80,000 | ≤ 1 ms | ≤ 10 ms |
| HTTP `POST …/next`, strict, 64 callers | ≥ 5,000 | ≥ 5,000 | ≤ 15 ms | ≤ 30 ms |
| HTTP `POST …/next`, block (1000), 16–64 callers | ≥ 30,000 | ≥ 30,000 | ≤ 2 ms | ≤ 10 ms |

How the modes relate to the target:

- **strict** is the default and the correctness baseline. Its latency is bounded
  below by one durable commit, so its budget is stated in multiples of the
  disk's sync time: on the reference SSD, p99 ≤ 30 ms is about six to eight
  `F_FULLFSYNC` calls.
- **Group commit** is what makes strict mode scale with callers. It is always
  on (`max_batch = 1024`, `max_wait_us = 0`). The one-commit-per-request
  configuration (`max_batch = 1`) exists only as the benchmark baseline.
- **block** is the route to high request rates. It trades a possible gap after a
  crash for one commit per block instead of per group (ADR-0004). Choose a
  block size several times `count × concurrent callers`. With
  `count = 100`, 64 callers and size 1000, block mode was 4× slower than strict.
- **Batches** (`?count=n`) are the route to high ID rates in any mode.

US-0017's "p99 under 5 ms on a laptop with durable commits" is met in block mode
up to 16 callers but **cannot be met in strict mode** on the reference laptop:
a single `F_FULLFSYNC` already has a p99 above 5 ms. For strict mode, this ADR
replaces that budget with the strict rows above. Whether to accept that, or to
measure again on Linux hardware, is for the owner to decide.

## Consequences

- Regressions are judged against concrete numbers. Re-measure with
  `cargo bench` and compare with `benches/baseline.md`.
- Strict-mode latency targets do not carry over to other disks. On Linux,
  `fdatasync` on NVMe with power-loss protection should be much faster; such a
  machine needs its own row in the baseline before it gets targets.
- The strict targets were set after measuring, with headroom of about 30% on
  throughput and latency. They show what the design delivers. Nothing outside
  the measurements forced them.
- The documentation must tell operators that high request rates need `block`
  mode or batches, and how to size blocks.

## Alternatives considered

- **Adopt the 500k–1M requests/s aspiration.** Rejected: it is about 70× the
  measured durable rate and 10× block mode over HTTP on this hardware. It would
  be a target nobody could test against.
- **Keep the 5 ms p99 for strict mode by disabling `F_FULLFSYNC`** (plain
  `fsync` on macOS): rejected. That trades the durability guarantee of
  ADR-0004 for a number, and plain `fsync` on macOS does not guarantee the data
  reached stable storage.
- **Set targets only for a Linux server:** deferred. No such machine was
  measured, and US-0017 names a laptop.
