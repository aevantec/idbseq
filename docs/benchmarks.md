# Benchmarks

idbseq ships three benchmarks. None of them run during `cargo test`; they are
`[[bench]]` targets and run only with `cargo bench`. Results for the reference
machine are in [`benches/baseline.md`](../benches/baseline.md), and the targets
derived from them are in [ADR-0005](adr/0005-throughput-target.md).

| Bench | What it measures | Time |
| --- | --- | --- |
| `idbseq-core` / `advance` (criterion) | `SequenceStore::advance` throughput: single IDs, batches of 100, one hot collection vs 64 collections, at 1/16/64 callers | ~2 min |
| `idbseq-core` / `load` | Same stores for a fixed time each: requests/s, IDs/s, p50/p99/max latency, and the group-commit and block-mode ratios | ~1 min (3 s per scenario) |
| `idbseq-server` / `http_next` | `POST /v1/collections/{name}/next` over HTTP at 1/16/64 callers, strict and block collections: requests/s, p50/p99/max | ~20 s |

Every store scenario runs against four backends:

- `memory` is `InMemoryStore`: no I/O, the engine's ceiling.
- `redb-strict-group` is `RedbStore` with strict collections and default group
  commit.
- `redb-strict-single-commit` is `RedbStore` with `max_batch = 1`, one commit
  and fsync per request. It is the baseline for group commit.
- `redb-block-1000` is `RedbStore` with block collections of size 1000.

redb runs in a fresh temp directory with real syncs. On macOS that means
`F_FULLFSYNC`; see the hardware notes in the baseline.

## Running

```sh
# Criterion; HTML reports in target/criterion/
cargo bench -p idbseq-core --bench advance
cargo bench -p idbseq-core --bench advance -- single/redb   # filter by name

# Latency report and ratios, as Markdown
cargo bench -p idbseq-core --bench load
IDBSEQ_BENCH_SECS=10 cargo bench -p idbseq-core --bench load

# HTTP, against an in-process server on loopback
cargo bench -p idbseq-server --bench http_next
```

`cargo bench` with no arguments runs all three.

To compare against a previous criterion run on the same machine:

```sh
cargo bench -p idbseq-core --bench advance -- --save-baseline before
# … change code …
cargo bench -p idbseq-core --bench advance -- --baseline before
```

For a fair run:

- plug the laptop in;
- close other heavy programs;
- use the same disk each time (the temp directory is on the system disk; set
  `TMPDIR` to measure another one);
- run each command twice and keep the second result.

## Load generator on a separate machine

`http_next` measures the server and the load generator together when both run
in one process. To measure only the server, run `idbseqd` on the machine under
test and the bench on another machine on the same network.

On the server:

```sh
# Rate limiting must be off (0) or above the expected rate, or requests get 429.
cat > idbseq.toml <<'EOF'
bind = "0.0.0.0:7878"
data_dir = "/var/lib/idbseq-bench"

[rate_limit]
per_second = 0
EOF
IDBSEQ_CONFIG=idbseq.toml IDBSEQ_ADMIN_PASSWORD='bench admin password' idbseqd
```

Configuration is described in [operations.md](operations.md). Use a fresh data
directory on the disk you want to measure, and do not expose the server beyond
the test network: the bench logs in as admin.

On the load machine, from a checkout of this repository:

```sh
IDBSEQ_BENCH_URL=http://<server-ip>:7878 \
IDBSEQ_BENCH_ADMIN_PASSWORD='bench admin password' \
IDBSEQ_BENCH_SECS=10 \
  cargo bench -p idbseq-server --bench http_next
```

The bench logs in, creates a `generate` key and two collections (`strict` and
`block` with size 1000) with unique `bench-<timestamp>-…` names, runs 1, 16 and
64 callers against each, then deletes the collections and revokes the key.

Things to record with the results:

- both machines' CPU and memory;
- the server's disk and filesystem;
- the network: link speed and round-trip time (`ping`);
- whether the server ran other load.

Network round-trip time adds directly to p50, so compare remote and loopback
runs only in throughput and in the latency above the round-trip time.

Other HTTP load tools work too, for example `oha`:

```sh
oha -z 30s -c 64 -m POST -H "Authorization: Bearer $IDBSEQ_KEY" \
  http://<server-ip>:7878/v1/collections/<name>/next
```

## Updating the baseline

When the reference machine or the engine changes on purpose:

1. Run `load` and `http_next` with `IDBSEQ_BENCH_SECS=5`, and run `advance`.
2. Replace the tables in `benches/baseline.md`, and update the date, the commit
   and the hardware notes.
3. If a target in ADR-0005 no longer holds, say so in the same change. Either
   fix the regression or write a new ADR that supersedes it.
