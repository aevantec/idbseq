//! Shared benchmark setup: store fixtures and a multi-threaded load driver.
//!
//! Used by the criterion bench (`advance.rs`) and the latency report (`load.rs`).

#![allow(dead_code)] // each bench uses a different subset
#![allow(clippy::expect_used)] // bench setup: a failed step should abort the run

use std::num::NonZeroU32;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use idbseq_core::collection::{Collection, CollectionSpec, Durability};
use idbseq_core::store::{
    AdvanceRequest, GroupCommitConfig, InMemoryStore, RedbStore, SequenceStore,
};

/// Block size used for the `block` durability scenarios.
pub const BLOCK_SIZE: u32 = 1_000;

/// Which store and durability a scenario runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// `InMemoryStore`: no I/O at all; the engine's ceiling.
    Memory,
    /// `RedbStore`, strict collections, group commit on (default config).
    RedbGroup,
    /// `RedbStore`, strict collections, `max_batch = 1`: one commit (and
    /// fsync) per request — the baseline group commit is measured against.
    RedbSingle,
    /// `RedbStore`, `block` collections of [`BLOCK_SIZE`], group commit on.
    RedbBlock,
}

impl Backend {
    pub const ALL: [Self; 4] = [
        Self::Memory,
        Self::RedbGroup,
        Self::RedbSingle,
        Self::RedbBlock,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::RedbGroup => "redb-strict-group",
            Self::RedbSingle => "redb-strict-single-commit",
            Self::RedbBlock => "redb-block-1000",
        }
    }

    fn durability(self) -> Durability {
        match self {
            Self::RedbBlock => Durability::Block {
                size: NonZeroU32::new(BLOCK_SIZE).expect("non-zero block size"),
            },
            _ => Durability::Strict,
        }
    }
}

/// A store with `collections` collections, in a temp dir for redb.
pub struct Fixture {
    pub store: Arc<dyn SequenceStore>,
    pub names: Vec<String>,
    _dir: Option<tempfile::TempDir>,
}

impl Fixture {
    #[must_use]
    pub fn new(backend: Backend, collections: usize) -> Self {
        let (store, dir): (Arc<dyn SequenceStore>, _) = match backend {
            Backend::Memory => (Arc::new(InMemoryStore::new()), None),
            Backend::RedbGroup | Backend::RedbBlock => open_redb(GroupCommitConfig::default()),
            Backend::RedbSingle => open_redb(GroupCommitConfig {
                max_batch: 1,
                max_wait: Duration::ZERO,
            }),
        };
        let names: Vec<String> = (0..collections).map(|i| format!("bench-{i:03}")).collect();
        for name in &names {
            let spec = CollectionSpec {
                durability: backend.durability(),
                ..CollectionSpec::named(name.clone())
            };
            let collection = Collection::create(spec, SystemTime::now()).expect("valid spec");
            store.create(collection).expect("create collection");
        }
        Self {
            store,
            names,
            _dir: dir,
        }
    }
}

fn open_redb(config: GroupCommitConfig) -> (Arc<dyn SequenceStore>, Option<tempfile::TempDir>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = RedbStore::open(dir.path(), config).expect("open redb store");
    (Arc::new(store), Some(dir))
}

/// When a load run stops.
#[derive(Debug, Clone, Copy)]
pub enum Stop {
    /// Each caller issues this many requests.
    OpsPerCaller(u64),
    /// Callers keep going until this much time has passed.
    After(Duration),
}

/// What a load run measured.
#[derive(Debug, Clone)]
pub struct Sample {
    pub requests: u64,
    pub ids: u64,
    pub elapsed: Duration,
    /// Per-request latency, unsorted.
    pub latencies: Vec<Duration>,
}

/// Runs `callers` threads against the store; caller `i` uses collection
/// `names[i % names.len()]` and asks for `count` IDs per request.
#[must_use]
pub fn drive(
    store: &Arc<dyn SequenceStore>,
    names: &[String],
    callers: usize,
    count: u32,
    stop: Stop,
) -> Sample {
    let count = NonZeroU32::new(count).expect("count >= 1");
    let start = Arc::new(Barrier::new(callers + 1));
    let (latencies, elapsed) = thread::scope(|scope| {
        let workers: Vec<_> = (0..callers)
            .map(|caller| {
                let store = Arc::clone(store);
                let start = Arc::clone(&start);
                let request = AdvanceRequest::new(names[caller % names.len()].clone(), count);
                scope.spawn(move || caller_loop(store.as_ref(), &request, &start, stop))
            })
            .collect();
        start.wait();
        let began = Instant::now();
        let latencies: Vec<Duration> = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("caller thread"))
            .collect();
        (latencies, began.elapsed())
    });
    let requests = latencies.len() as u64;
    Sample {
        requests,
        ids: requests * u64::from(count.get()),
        elapsed,
        latencies,
    }
}

fn caller_loop(
    store: &dyn SequenceStore,
    request: &AdvanceRequest,
    start: &Barrier,
    stop: Stop,
) -> Vec<Duration> {
    start.wait();
    let began = Instant::now();
    let mut latencies = Vec::new();
    loop {
        let done = match stop {
            Stop::OpsPerCaller(ops) => latencies.len() as u64 >= ops,
            Stop::After(limit) => began.elapsed() >= limit,
        };
        if done {
            return latencies;
        }
        let sent = Instant::now();
        store.advance(request).expect("advance");
        latencies.push(sent.elapsed());
    }
}

/// The `p`-th percentile (0 < p <= 1) of `sorted`, nearest-rank method.
#[must_use]
pub fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )] // rank is within 1..=len
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}
