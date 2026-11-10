//! Criterion benchmarks for `SequenceStore::advance` (US-0038).
//!
//! Groups:
//! - `single`: one ID per request, one hot collection, 1/16/64 callers.
//! - `batch`: `count = 100` per request, one hot collection, 1/16/64 callers
//!   (throughput is reported in IDs).
//! - `collections`: 64 callers on one hot collection vs 64 collections.
//!
//! Every group runs against the in-memory store and redb (real fsync, temp
//! dir) in strict mode with group commit, strict with one commit per request,
//! and block mode. Run with `cargo bench -p idbseq-core --bench advance`.

#![allow(clippy::expect_used)] // bench setup: a failed step should abort the run
#![allow(clippy::cast_precision_loss)] // scaling request counts as f64 is fine

mod common;

use std::time::Duration;

use common::{Backend, Fixture, Stop, drive};
use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};

const CALLERS: [usize; 3] = [1, 16, 64];
const BATCH: u32 = 100;
const MANY_COLLECTIONS: usize = 64;

/// Times `iters` requests spread over `callers` threads.
fn timed(fixture: &Fixture, callers: usize, count: u32, iters: u64) -> Duration {
    let callers_u64 = callers as u64;
    let per_caller = iters.div_ceil(callers_u64).max(1);
    let sample = drive(
        &fixture.store,
        &fixture.names,
        callers,
        count,
        Stop::OpsPerCaller(per_caller),
    );
    // Scale to exactly `iters` requests; the last round may have overshot.
    sample
        .elapsed
        .mul_f64(iters as f64 / sample.requests as f64)
}

fn configure<'a>(
    c: &'a mut Criterion,
    name: &str,
) -> criterion::BenchmarkGroup<'a, criterion::measurement::WallTime> {
    let mut group = c.benchmark_group(name);
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
    group
}

fn bench_scenario(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    backend: Backend,
    collections: usize,
    callers: usize,
    count: u32,
    parameter: String,
) {
    let fixture = Fixture::new(backend, collections);
    group.bench_with_input(
        BenchmarkId::new(backend.label(), parameter),
        &callers,
        |b, &callers| {
            b.iter_custom(|iters| timed(&fixture, callers, count, iters));
        },
    );
}

fn single(c: &mut Criterion) {
    let mut group = configure(c, "single");
    group.throughput(Throughput::Elements(1));
    for backend in Backend::ALL {
        for callers in CALLERS {
            bench_scenario(
                &mut group,
                backend,
                1,
                callers,
                1,
                format!("{callers}-callers"),
            );
        }
    }
    group.finish();
}

fn batch(c: &mut Criterion) {
    let mut group = configure(c, "batch");
    group.throughput(Throughput::Elements(u64::from(BATCH)));
    for backend in Backend::ALL {
        for callers in CALLERS {
            bench_scenario(
                &mut group,
                backend,
                1,
                callers,
                BATCH,
                format!("count-{BATCH}/{callers}-callers"),
            );
        }
    }
    group.finish();
}

fn collections(c: &mut Criterion) {
    let mut group = configure(c, "collections");
    group.throughput(Throughput::Elements(1));
    for backend in Backend::ALL {
        for (label, collections) in [("hot", 1), ("many", MANY_COLLECTIONS)] {
            bench_scenario(
                &mut group,
                backend,
                collections,
                64,
                1,
                format!("{label}/64-callers"),
            );
        }
    }
    group.finish();
}

criterion_group!(benches, single, batch, collections);
criterion_main!(benches);
