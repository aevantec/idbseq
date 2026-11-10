//! Throughput and latency report for `SequenceStore::advance` (US-0038).
//!
//! Criterion (`advance.rs`) gives stable means; this harness runs each scenario
//! for a fixed time and reports requests/s, IDs/s and p50/p99/max latency per
//! request, plus the ratios the US-0039 and US-0035 criteria are judged on.
//! The output is Markdown, ready for `benches/baseline.md`.
//!
//! ```sh
//! cargo bench -p idbseq-core --bench load            # 3 s per scenario
//! IDBSEQ_BENCH_SECS=10 cargo bench -p idbseq-core --bench load
//! ```

#![allow(clippy::expect_used)] // bench setup: a failed step should abort the run
#![allow(clippy::cast_precision_loss)] // reporting rates as f64 is fine

mod common;

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::time::{Duration, Instant};

use common::{Backend, Fixture, Sample, Stop, drive, percentile};

const DEFAULT_SECS: u64 = 3;
const FSYNC_PROBES: usize = 100;
const PROBE_BYTES: usize = 4096;

struct Row {
    backend: Backend,
    collections: usize,
    callers: usize,
    count: u32,
    sample: Sample,
}

impl Row {
    fn requests_per_sec(&self) -> f64 {
        self.sample.requests as f64 / self.sample.elapsed.as_secs_f64()
    }

    fn ids_per_sec(&self) -> f64 {
        self.sample.ids as f64 / self.sample.elapsed.as_secs_f64()
    }
}

fn micros(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1e6)
}

fn rate(value: f64) -> String {
    let rounded = value.round();
    if rounded >= 1e6 {
        format!("{:.2}M", rounded / 1e6)
    } else if rounded >= 1e3 {
        format!("{:.1}k", rounded / 1e3)
    } else {
        format!("{rounded:.0}")
    }
}

/// Median latency of `write 4 KiB + File::sync_data` on a temp file: what one
/// durable commit costs on this disk, independent of redb.
fn fsync_probe() -> Duration {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dir.path().join("probe"))
        .expect("probe file");
    let block = vec![0x5a_u8; PROBE_BYTES];
    let mut times: Vec<Duration> = (0..FSYNC_PROBES)
        .map(|_| {
            file.seek(SeekFrom::Start(0)).expect("seek");
            file.write_all(&block).expect("write");
            let began = Instant::now();
            file.sync_data().expect("sync_data");
            began.elapsed()
        })
        .collect();
    times.sort();
    percentile(&times, 0.5)
}

fn run(
    backend: Backend,
    collections: usize,
    callers: usize,
    count: u32,
    duration: Duration,
) -> Row {
    let fixture = Fixture::new(backend, collections);
    let mut sample = drive(
        &fixture.store,
        &fixture.names,
        callers,
        count,
        Stop::After(duration),
    );
    sample.latencies.sort();
    Row {
        backend,
        collections,
        callers,
        count,
        sample,
    }
}

fn print_rows(rows: &[Row]) {
    println!(
        "| backend | collections | callers | IDs/request | requests/s | IDs/s | p50 µs | p99 µs | max µs |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for row in rows {
        let lat = &row.sample.latencies;
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.backend.label(),
            row.collections,
            row.callers,
            row.count,
            rate(row.requests_per_sec()),
            rate(row.ids_per_sec()),
            micros(percentile(lat, 0.50)),
            micros(percentile(lat, 0.99)),
            micros(lat.last().copied().unwrap_or_default()),
        );
    }
}

fn find(
    rows: &[Row],
    backend: Backend,
    collections: usize,
    callers: usize,
    count: u32,
) -> Option<&Row> {
    rows.iter().find(|r| {
        r.backend == backend
            && r.collections == collections
            && r.callers == callers
            && r.count == count
    })
}

fn print_ratio(rows: &[Row], label: &str, faster: Backend, slower: Backend, callers: usize) {
    if let (Some(a), Some(b)) = (
        find(rows, faster, 1, callers, 1),
        find(rows, slower, 1, callers, 1),
    ) {
        println!(
            "- {label}, {callers} callers: {:.1}× ({} vs {} requests/s; p99 {} µs vs {} µs)",
            a.requests_per_sec() / b.requests_per_sec(),
            rate(a.requests_per_sec()),
            rate(b.requests_per_sec()),
            micros(percentile(&a.sample.latencies, 0.99)),
            micros(percentile(&b.sample.latencies, 0.99)),
        );
    }
}

fn main() {
    let secs = std::env::var("IDBSEQ_BENCH_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SECS);
    let duration = Duration::from_secs(secs);

    println!(
        "fsync probe (write 4 KiB + File::sync_data), median: {} µs\n",
        micros(fsync_probe())
    );

    let mut rows = Vec::new();
    for backend in Backend::ALL {
        for callers in [1, 16, 64] {
            rows.push(run(backend, 1, callers, 1, duration));
        }
        rows.push(run(backend, 64, 64, 1, duration));
        rows.push(run(backend, 1, 64, 100, duration));
    }
    println!("Each scenario ran for {secs} s.\n");
    print_rows(&rows);

    println!("\nRatios (one hot collection, 1 ID per request):\n");
    for callers in [1, 16, 64] {
        print_ratio(
            &rows,
            "group commit vs one commit per request",
            Backend::RedbGroup,
            Backend::RedbSingle,
            callers,
        );
    }
    for callers in [1, 16, 64] {
        print_ratio(
            &rows,
            "block-1000 vs strict (group commit)",
            Backend::RedbBlock,
            Backend::RedbGroup,
            callers,
        );
    }
    for callers in [1, 16, 64] {
        print_ratio(
            &rows,
            "block-1000 vs strict (one commit per request)",
            Backend::RedbBlock,
            Backend::RedbSingle,
            callers,
        );
    }
}
