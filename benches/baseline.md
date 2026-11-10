# Benchmark baseline

Reference numbers for US-0038. Compare a new run against these tables to spot
regressions. Re-run and replace this file (keeping the hardware notes) when the
reference machine or the engine changes on purpose. How to run:
[docs/benchmarks.md](../docs/benchmarks.md). Targets derived from these
numbers: [ADR-0005](../docs/adr/0005-throughput-target.md).

- Date: 2026-11-10, code at `bc776b3` plus the benchmark harness
- Built with `cargo bench` (release profile), Rust 1.99.1

## Hardware

| | |
| --- | --- |
| Machine | MacBook Pro 14" 2021 (MacBookPro18,1), **on battery** during the run |
| CPU | Apple M1 Pro, 10 cores (8 performance + 2 efficiency) |
| Memory | 16 GB |
| Disk | Internal Apple SSD AP0512R (NVMe over Apple Fabric), APFS |
| OS | macOS 27.2 |

### fsync behaviour

redb makes a commit durable with `File::sync_data`. On macOS, Rust's standard
library implements that with `fcntl(F_FULLFSYNC)`, which flushes the drive's
write cache, not with plain `fsync()`, which on macOS only hands data to the
drive. Measured on this machine (4 KiB write, then sync, 100 runs):

| call | p50 | p99 |
| --- | ---: | ---: |
| `File::sync_data` (Rust std, used by redb) | 3.29 ms | — |
| `fcntl(F_FULLFSYNC)` (Python `fcntl`) | 3.9 ms | 11.7 ms |
| plain `fsync()` (Python `os.fsync`) | 0.07 ms | 1.1 ms |

So every durable redb commit here costs one real cache flush, about 3–5 ms.
On Linux, `fdatasync` on an NVMe drive with power-loss protection is usually far
cheaper; these numbers are **not** a prediction for a Linux server.

## Store level (`cargo bench -p idbseq-core --bench load`, 5 s per scenario)

`IDBSEQ_BENCH_SECS=5`. Latency is per `advance` call as seen by the caller.
`redb-strict-single-commit` is `GroupCommitConfig { max_batch: 1 }`: one
transaction and one sync per request, the baseline for group commit.

| backend | collections | callers | IDs/request | requests/s | IDs/s | p50 µs | p99 µs | max µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| memory | 1 | 1 | 1 | 9.58M | 9.58M | 0.0 | 0.1 | 694.8 |
| memory | 1 | 16 | 1 | 1.94M | 1.94M | 0.4 | 98.7 | 10972.5 |
| memory | 1 | 64 | 1 | 2.05M | 2.05M | 0.3 | 404.3 | 208825.8 |
| memory | 64 | 64 | 1 | 1.95M | 1.95M | 0.5 | 276.7 | 273515.4 |
| memory | 1 | 64 | 100 | 2.03M | 202.55M | 0.4 | 219.7 | 300926.9 |
| redb-strict-group | 1 | 1 | 1 | 183 | 183 | 4948.8 | 14964.0 | 40016.2 |
| redb-strict-group | 1 | 16 | 1 | 1.6k | 1.6k | 9832.7 | 22857.5 | 29836.6 |
| redb-strict-group | 1 | 64 | 1 | 6.8k | 6.8k | 9432.9 | 22586.8 | 31625.0 |
| redb-strict-group | 64 | 64 | 1 | 7.2k | 7.2k | 8573.3 | 25191.5 | 39081.2 |
| redb-strict-group | 1 | 64 | 100 | 6.7k | 668.1k | 9408.3 | 24027.4 | 48466.1 |
| redb-strict-single-commit | 1 | 1 | 1 | 165 | 165 | 5137.0 | 15804.5 | 17202.4 |
| redb-strict-single-commit | 1 | 16 | 1 | 196 | 196 | 80741.6 | 107650.3 | 116598.0 |
| redb-strict-single-commit | 1 | 64 | 1 | 185 | 185 | 337612.1 | 441515.3 | 445208.1 |
| redb-strict-single-commit | 64 | 64 | 1 | 215 | 215 | 292158.0 | 355252.8 | 364575.9 |
| redb-strict-single-commit | 1 | 64 | 100 | 182 | 18.2k | 348941.8 | 404055.0 | 407560.1 |
| redb-block-1000 | 1 | 1 | 1 | 66.0k | 66.0k | 10.6 | 18.9 | 12981.3 |
| redb-block-1000 | 1 | 16 | 1 | 102.7k | 102.7k | 72.0 | 4241.4 | 11311.8 |
| redb-block-1000 | 1 | 64 | 1 | 100.3k | 100.3k | 134.4 | 5896.6 | 18298.1 |
| redb-block-1000 | 64 | 64 | 1 | 82.1k | 82.1k | 378.1 | 7618.4 | 364379.6 |
| redb-block-1000 | 1 | 64 | 100 | 1.6k | 164.7k | 32927.3 | 218679.1 | 580099.2 |

Notes:

- 64 caller threads on 10 cores: the long `max` values for `memory` are
  scheduling stalls, not store work.
- `redb-block-1000` with `count = 100` refills a block every 10 requests, and a
  refill is a durable commit that every caller of that collection waits for.
  With 64 callers this is **slower than strict group commit** (1.6k vs 6.7k
  requests/s). Block size should be many times `count × callers`.

## Store level, criterion (`cargo bench -p idbseq-core --bench advance`)

Mean throughput with criterion's 95% interval, 10 samples of 2 s (elements are
requests, or IDs in the `batch` group).

| group / backend / scenario | throughput |
| --- | ---: |
| single / memory / 1, 16, 64 callers | 10.6M, 6.88M, 5.36M /s |
| single / redb-strict-group / 1, 16, 64 callers | 205, 1.94k, 6.82k /s |
| single / redb-strict-single-commit / 1, 16, 64 callers | 216, 214, 206 /s |
| single / redb-block-1000 / 1, 16, 64 callers | 54.2k, 95.7k, 96.9k /s |
| batch (IDs) / memory / 1, 16, 64 callers | 1.25G, 233M, 240M /s |
| batch (IDs) / redb-strict-group / 1, 16, 64 callers | 23.9k, 232k, 1.21M /s |
| batch (IDs) / redb-strict-single-commit / 1, 16, 64 callers | 19.8k, 18.7k, 18.8k /s |
| batch (IDs) / redb-block-1000 / 1, 16, 64 callers | 170k, 161k, 159k /s |
| collections / memory / hot, many (64 callers) | 2.18M, 1.76M /s |
| collections / redb-strict-group / hot, many | 7.37k, 8.09k /s |
| collections / redb-strict-single-commit / hot, many | 180, 214 /s |
| collections / redb-block-1000 / hot, many | 93.3k, 76.7k /s |

## HTTP (`cargo bench -p idbseq-server --bench http_next`, 5 s per scenario)

In-process `idbseqd` router on a loopback port, temp data dir, rate limiting
off, default group commit (`max_batch = 1024`, `max_wait_us = 0`). The load
generator (blocking reqwest, one keep-alive connection per caller) runs **in
the same process and on the same 10 cores**, so these are conservative.

| collection | callers | requests/s | p50 ms | p99 ms | max ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| strict | 1 | 165 | 5.26 | 16.57 | 27.25 |
| strict | 16 | 1592 | 9.17 | 22.95 | 34.44 |
| strict | 64 | 6462 | 8.70 | 28.06 | 40.41 |
| block-1000 | 1 | 9938 | 0.09 | 0.17 | 5.16 |
| block-1000 | 16 | 37250 | 0.36 | 3.53 | 7.53 |
| block-1000 | 64 | 39277 | 1.39 | 5.62 | 13.57 |

An earlier 3 s run measured block-1000 with 1 caller at only 1,336 requests/s
(p99 5.6 ms); the 5 s run above did not repeat that. Treat single-caller HTTP
numbers on a laptop as noisy.

## Acceptance criteria, as measured

| Card | Criterion | Measured | Verdict |
| --- | --- | --- | --- |
| US-0039 | Group commit ≥ 10× one commit per request at 64 callers, within the p99 budget | 6.8k vs 185 requests/s = **36.7×** (criterion: 6.82k vs 206 = 33×). p99 22.6 ms vs 441 ms; ADR-0005 budget for strict at 64 callers is p99 ≤ 30 ms | **Met** (the budget was set after measuring, see ADR-0005) |
| US-0035 | Block ≥ 10× strict | 1 ID/request vs strict group commit: **362×** (1 caller), **64×** (16), **14.8×** (64). Against one commit per request: 400–543× | **Met** for single-ID requests. **Not met** for `count = 100` with block size 1000 at 64 callers (0.25×) |
| US-0017 | `next` p99 < 5 ms on a laptop with durable commits | strict: p99 **16.6 ms** (1 caller), 23.0 ms (16), 28.1 ms (64); block-1000: p99 0.17 ms (1), 3.5 ms (16), 5.6 ms (64) | **Not met in strict mode** on this Mac: one `F_FULLFSYNC` alone has a 3.9 ms median and 11.7 ms p99. Met in block mode up to 16 callers |
