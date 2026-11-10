//! US-0010: uniqueness under concurrency, against the durable redb store.
#![allow(clippy::expect_used)] // test code: a failed setup step should panic

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

use idbseq_core::collection::{CollectionSpec, Durability, Id};
use idbseq_core::engine::{Engine, Limits, NextRequest};
use idbseq_core::store::{GroupCommitConfig, RedbStore};

const TASKS: usize = 64;
const IDS_PER_TASK: u32 = 10_000;
const COLLECTIONS: [&str; 4] = ["orders", "invoices", "tickets", "fast"];

fn engine(dir: &std::path::Path) -> Engine {
    let store = RedbStore::open(dir, GroupCommitConfig::default()).expect("open");
    Engine::new(Arc::new(store), Limits::default())
}

/// Batch sizes cycling 1..=250, so tasks mix single IDs and batches.
fn batch_sizes(task: usize) -> impl Iterator<Item = u32> {
    let mut remaining = IDS_PER_TASK;
    let mut size = u32::try_from(task % 250).expect("small") + 1;
    std::iter::from_fn(move || {
        let take = size.min(remaining);
        remaining -= take;
        size = size % 250 + 1;
        (take > 0).then_some(take)
    })
}

#[test]
fn sixty_four_tasks_issue_unique_increasing_ids_across_collections() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine(dir.path());
    for name in COLLECTIONS {
        let durability = if name == "fast" {
            Durability::Block {
                size: std::num::NonZeroU32::new(1000).expect("non-zero"),
            }
        } else {
            Durability::Strict
        };
        let spec = CollectionSpec {
            durability,
            ..CollectionSpec::named(name)
        };
        engine.create(spec, SystemTime::now()).expect("create");
    }

    let per_task: Vec<(usize, Vec<Id>)> = thread::scope(|scope| {
        let workers: Vec<_> = (0..TASKS)
            .map(|task| {
                let engine = &engine;
                scope.spawn(move || {
                    let name = COLLECTIONS[task % COLLECTIONS.len()];
                    let ids = batch_sizes(task)
                        .flat_map(|count| {
                            let request = NextRequest {
                                name: name.into(),
                                count: Some(count),
                                idempotency_key: None,
                            };
                            let range = engine
                                .next(&request, SystemTime::now())
                                .expect("next")
                                .range;
                            range.first..=range.last
                        })
                        .collect::<Vec<Id>>();
                    (task, ids)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("task panicked"))
            .collect()
    });

    let mut by_collection: HashMap<&str, Vec<Id>> = HashMap::new();
    for (task, ids) in per_task {
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "task {task} saw IDs out of order"
        );
        by_collection
            .entry(COLLECTIONS[task % COLLECTIONS.len()])
            .or_default()
            .extend(ids);
    }
    for (name, mut ids) in by_collection {
        ids.sort_unstable();
        let expected: Vec<Id> = (1..=Id::try_from(ids.len()).expect("fits")).collect();
        assert_eq!(
            ids.len(),
            TASKS / COLLECTIONS.len() * IDS_PER_TASK as usize,
            "{name}"
        );
        assert_eq!(ids, expected, "{name}: duplicates or gaps");
    }
}
