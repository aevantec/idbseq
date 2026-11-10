//! Conformance suite every [`SequenceStore`] backend must pass.
//!
//! In a backend's test module:
//!
//! ```ignore
//! sequence_store_conformance!(|| Box::new(MyStore::new_for_tests()));
//! ```
//!
//! The factory is called once per test and must return a new, empty store behind
//! any pointer that derefs to it (`Box`, a guard that owns a temp directory, …).

use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};
use std::thread;
use std::time::{Duration, SystemTime};

use super::{AdvanceRequest, IdempotencyKey, Issued, PageRequest, SequenceStore, StoreError};
use crate::collection::{Collection, CollectionPatch, CollectionSpec, Durability, Id, IssuedRange};
use crate::validation::ValidationErrors;

const THREADS: u32 = 8;
const CALLS_PER_THREAD: u32 = 250;

/// Generates one `#[test]` per conformance check for a store factory.
macro_rules! sequence_store_conformance {
    ($factory:expr) => {
        sequence_store_conformance!(@tests $factory;
            create_then_get_returns_the_collection,
            create_with_a_taken_name_fails_with_already_exists,
            get_unknown_fails_with_not_found,
            delete_removes_the_collection_and_returns_it,
            delete_unknown_fails_with_not_found,
            recreated_collection_starts_again_from_start,
            first_advance_returns_start,
            advance_continues_by_step,
            batch_advance_returns_a_contiguous_range,
            advance_up_to_max_succeeds,
            advance_past_max_fails_and_does_not_move_the_counter,
            advance_past_the_id_range_fails_with_exhausted,
            advance_unknown_fails_with_not_found,
            advance_leaves_other_collections_untouched,
            concurrent_advances_never_overlap,
            list_is_sorted_by_name_and_paginated,
            list_filters_by_prefix_and_counts_the_total,
            list_of_an_empty_store_is_empty,
            update_replaces_the_collection_and_returns_before_and_after,
            failed_update_leaves_the_collection_unchanged,
            update_unknown_fails_with_not_found,
            idempotent_retry_replays_the_original_range,
            reused_idempotency_key_for_another_request_fails,
            expired_idempotency_key_issues_new_ids,
            purge_removes_only_expired_idempotency_records,
            block_durability_issues_contiguous_ids_and_reports_the_last_issued,
            reset_of_a_block_collection_moves_the_counter,
            deleted_block_collection_starts_again_from_start,
            idempotent_retry_on_a_block_collection_replays_the_original_range,
        );
    };
    (@tests $factory:expr; $($check:ident),+ $(,)?) => {
        $(
            #[test]
            fn $check() {
                $crate::store::conformance::$check(&*($factory)());
            }
        )+
    };
}

fn count(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("count must be non-zero")
}

fn step(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("step must be non-zero")
}

fn advance(store: &dyn SequenceStore, name: &str, n: u32) -> Result<IssuedRange, StoreError> {
    store
        .advance(&AdvanceRequest::new(name, count(n)))
        .map(|issued| issued.range)
}

fn keyed(name: &str, n: u32, key: &str, fingerprint: &str, now: SystemTime) -> AdvanceRequest {
    AdvanceRequest {
        idempotency: Some(IdempotencyKey {
            key: key.to_owned(),
            fingerprint: fingerprint.to_owned(),
            now,
            ttl: Duration::from_secs(60),
        }),
        ..AdvanceRequest::new(name, count(n))
    }
}

fn block(size: u32) -> Durability {
    Durability::Block { size: count(size) }
}

fn collection(spec: CollectionSpec) -> Collection {
    Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid spec")
}

fn create(store: &dyn SequenceStore, spec: CollectionSpec) -> Collection {
    store
        .create(collection(spec))
        .expect("create should succeed")
}

fn page(limit: usize, cursor: Option<&str>, prefix: Option<&str>) -> PageRequest {
    PageRequest {
        limit: NonZeroUsize::new(limit).expect("limit must be non-zero"),
        cursor: cursor.map(str::to_owned),
        prefix: prefix.map(str::to_owned),
    }
}

fn names(store: &dyn SequenceStore, request: &PageRequest) -> Vec<String> {
    let page = store.list(request).expect("list should succeed");
    page.items.iter().map(|c| c.name().to_owned()).collect()
}

pub fn create_then_get_returns_the_collection(store: &dyn SequenceStore) {
    let spec = CollectionSpec {
        start: 1000,
        step: step(10),
        protected: true,
        ..CollectionSpec::named("invoices")
    };

    let created = create(store, spec.clone());
    let fetched = store.get("invoices").expect("get should succeed");

    assert_eq!(created, collection(spec));
    assert_eq!(fetched, created);
    assert_eq!(fetched.last_id(), None);
}

pub fn create_with_a_taken_name_fails_with_already_exists(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));

    let result = store.create(collection(CollectionSpec {
        start: 50,
        ..CollectionSpec::named("orders")
    }));

    assert_eq!(result, Err(StoreError::AlreadyExists("orders".into())));
    assert_eq!(store.get("orders").map(|c| c.spec().start), Ok(1));
}

pub fn get_unknown_fails_with_not_found(store: &dyn SequenceStore) {
    assert_eq!(
        store.get("missing"),
        Err(StoreError::NotFound("missing".into()))
    );
}

pub fn delete_removes_the_collection_and_returns_it(store: &dyn SequenceStore) {
    let created = create(store, CollectionSpec::named("orders"));

    let deleted = store.delete("orders");

    assert_eq!(deleted, Ok(created));
    assert_eq!(
        store.get("orders"),
        Err(StoreError::NotFound("orders".into()))
    );
}

pub fn delete_unknown_fails_with_not_found(store: &dyn SequenceStore) {
    assert_eq!(
        store.delete("missing"),
        Err(StoreError::NotFound("missing".into()))
    );
}

pub fn recreated_collection_starts_again_from_start(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    advance(store, "orders", 5).expect("advance should succeed");
    store.delete("orders").expect("delete should succeed");
    create(store, CollectionSpec::named("orders"));

    let range = advance(store, "orders", 1);

    assert_eq!(range.map(|r| r.first), Ok(1));
}

pub fn first_advance_returns_start(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            start: 1000,
            ..CollectionSpec::named("invoices")
        },
    );

    let range = advance(store, "invoices", 1).expect("advance should succeed");

    assert_eq!((range.first, range.last), (1000, 1000));
    assert_eq!(store.get("invoices").map(|c| c.last_id()), Ok(Some(1000)));
}

pub fn advance_continues_by_step(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            start: 1000,
            step: step(10),
            ..CollectionSpec::named("invoices")
        },
    );

    let issued: Vec<Id> = (0..3)
        .map(|_| {
            advance(store, "invoices", 1)
                .expect("advance should succeed")
                .first
        })
        .collect();

    assert_eq!(issued, vec![1000, 1010, 1020]);
}

pub fn batch_advance_returns_a_contiguous_range(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            step: step(5),
            ..CollectionSpec::named("orders")
        },
    );
    advance(store, "orders", 1).expect("advance should succeed");

    let range = advance(store, "orders", 100).expect("advance should succeed");

    assert_eq!((range.first, range.last), (6, 501));
    assert_eq!((range.count.get(), range.step.get()), (100, 5));
    assert_eq!(store.get("orders").map(|c| c.last_id()), Ok(Some(501)));
}

pub fn advance_up_to_max_succeeds(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            max: 10,
            ..CollectionSpec::named("tickets")
        },
    );

    let range = advance(store, "tickets", 10).expect("advance should succeed");

    assert_eq!((range.first, range.last), (1, 10));
}

pub fn advance_past_max_fails_and_does_not_move_the_counter(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            max: 10,
            ..CollectionSpec::named("tickets")
        },
    );
    advance(store, "tickets", 8).expect("advance should succeed");

    let too_many = advance(store, "tickets", 3);
    let remaining = advance(store, "tickets", 2);

    assert_eq!(too_many, Err(StoreError::Exhausted("tickets".into())));
    assert_eq!(remaining.map(|r| (r.first, r.last)), Ok((9, 10)));
    assert_eq!(
        advance(store, "tickets", 1),
        Err(StoreError::Exhausted("tickets".into()))
    );
}

pub fn advance_past_the_id_range_fails_with_exhausted(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            start: Id::MAX - 1,
            step: step(u64::MAX),
            ..CollectionSpec::named("edge")
        },
    );

    let overflowing_batch = advance(store, "edge", 2);
    let first = advance(store, "edge", 1);
    let overflowing_next = advance(store, "edge", 1);

    assert_eq!(overflowing_batch, Err(StoreError::Exhausted("edge".into())));
    assert_eq!(first.map(|r| r.first), Ok(Id::MAX - 1));
    assert_eq!(overflowing_next, Err(StoreError::Exhausted("edge".into())));
}

pub fn advance_unknown_fails_with_not_found(store: &dyn SequenceStore) {
    assert_eq!(
        advance(store, "missing", 1),
        Err(StoreError::NotFound("missing".into()))
    );
}

pub fn advance_leaves_other_collections_untouched(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    create(store, CollectionSpec::named("invoices"));

    advance(store, "orders", 10).expect("advance should succeed");

    assert_eq!(store.get("invoices").map(|c| c.last_id()), Ok(None));
}

pub fn concurrent_advances_never_overlap(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));

    let mut issued: Vec<Id> = thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                scope.spawn(|| {
                    (0..CALLS_PER_THREAD)
                        .flat_map(|_| {
                            let range = advance(store, "orders", 2).expect("advance");
                            [range.first, range.last]
                        })
                        .collect::<Vec<Id>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("worker panicked"))
            .collect()
    });
    issued.sort_unstable();

    let expected: Vec<Id> = (1..=Id::from(THREADS * CALLS_PER_THREAD * 2)).collect();
    assert_eq!(issued, expected);
}

pub fn list_is_sorted_by_name_and_paginated(store: &dyn SequenceStore) {
    for name in ["orders", "invoices", "tickets", "accounts", "users"] {
        create(store, CollectionSpec::named(name));
    }

    let first = store
        .list(&page(2, None, None))
        .expect("list should succeed");
    let second = store
        .list(&page(2, first.next_cursor.as_deref(), None))
        .expect("list should succeed");
    let third = store
        .list(&page(2, second.next_cursor.as_deref(), None))
        .expect("list should succeed");

    let all: Vec<&str> = [&first, &second, &third]
        .iter()
        .flat_map(|p| p.items.iter().map(Collection::name))
        .collect();
    assert_eq!(all, ["accounts", "invoices", "orders", "tickets", "users"]);
    assert_eq!((first.total, second.total, third.total), (5, 5, 5));
    assert_eq!(first.next_cursor.as_deref(), Some("invoices"));
    assert_eq!(third.next_cursor, None);
}

pub fn list_filters_by_prefix_and_counts_the_total(store: &dyn SequenceStore) {
    for name in ["eu.orders", "eu.invoices", "us.orders", "eu.tickets", "eux"] {
        create(store, CollectionSpec::named(name));
    }

    let first = store
        .list(&page(2, None, Some("eu.")))
        .expect("list should succeed");
    let rest = names(store, &page(2, first.next_cursor.as_deref(), Some("eu.")));

    assert_eq!(first.total, 3);
    assert_eq!(
        first.items.iter().map(Collection::name).collect::<Vec<_>>(),
        ["eu.invoices", "eu.orders"]
    );
    assert_eq!(rest, ["eu.tickets"]);
}

pub fn list_of_an_empty_store_is_empty(store: &dyn SequenceStore) {
    let page = store
        .list(&page(10, None, None))
        .expect("list should succeed");

    assert_eq!(page.items, Vec::new());
    assert_eq!((page.total, page.next_cursor), (0, None));
}

pub fn update_replaces_the_collection_and_returns_before_and_after(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    advance(store, "orders", 3).expect("advance should succeed");
    let patch = CollectionPatch {
        max: Some(100),
        ..CollectionPatch::default()
    };

    let updated = store
        .update("orders", &|c| Ok(c.apply(&patch)?))
        .expect("update should succeed");

    assert_eq!(updated.before.spec().max, Id::MAX);
    assert_eq!(updated.after.spec().max, 100);
    assert_eq!(updated.after.last_id(), Some(3));
    assert_eq!(store.get("orders"), Ok(updated.after));
}

pub fn failed_update_leaves_the_collection_unchanged(store: &dyn SequenceStore) {
    let created = create(store, CollectionSpec::named("orders"));

    let result = store.update("orders", &|_| {
        Err(ValidationErrors::field("max", "nope").into())
    });

    assert!(matches!(result, Err(StoreError::Invalid(_))));
    assert_eq!(store.get("orders"), Ok(created));
}

pub fn update_unknown_fails_with_not_found(store: &dyn SequenceStore) {
    let result = store.update("missing", &|c| Ok(c.clone()));

    assert_eq!(result, Err(StoreError::NotFound("missing".into())));
}

pub fn idempotent_retry_replays_the_original_range(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let request = keyed("orders", 5, "key-1:abc", "orders/5", now);

    let first = store.advance(&request).expect("first call");
    let retry = store.advance(&request).expect("retry");
    let next = advance(store, "orders", 1).expect("advance should succeed");

    assert_eq!(
        (first.range.first, first.range.last, first.replayed),
        (1, 5, false)
    );
    assert_eq!(
        retry,
        Issued {
            replayed: true,
            ..first
        }
    );
    assert_eq!(next.first, 6);
}

pub fn reused_idempotency_key_for_another_request_fails(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    store
        .advance(&keyed("orders", 5, "key-1:abc", "orders/5", now))
        .expect("first call");

    let reused = store.advance(&keyed("orders", 2, "key-1:abc", "orders/2", now));

    assert_eq!(reused, Err(StoreError::IdempotencyMismatch));
    assert_eq!(store.get("orders").map(|c| c.last_id()), Ok(Some(5)));
}

pub fn expired_idempotency_key_issues_new_ids(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let later = then + Duration::from_secs(61);
    store
        .advance(&keyed("orders", 1, "key-1:abc", "orders/1", then))
        .expect("first call");

    let after_expiry = store
        .advance(&keyed("orders", 1, "key-1:abc", "orders/1", later))
        .expect("fresh call");

    assert_eq!(
        (after_expiry.range.first, after_expiry.replayed),
        (2, false)
    );
}

pub fn purge_removes_only_expired_idempotency_records(store: &dyn SequenceStore) {
    create(store, CollectionSpec::named("orders"));
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let later = then + Duration::from_secs(30);
    store
        .advance(&keyed("orders", 1, "old", "orders/1", then))
        .expect("old call");
    store
        .advance(&keyed("orders", 1, "new", "orders/1", later))
        .expect("new call");

    let purged = store.purge_idempotency(then + Duration::from_secs(61));
    let replay_new = store
        .advance(&keyed("orders", 1, "new", "orders/1", later))
        .expect("replay");

    assert_eq!(purged, Ok(1));
    assert!(replay_new.replayed);
}

pub fn block_durability_issues_contiguous_ids_and_reports_the_last_issued(
    store: &dyn SequenceStore,
) {
    create(
        store,
        CollectionSpec {
            durability: block(10),
            ..CollectionSpec::named("fast")
        },
    );

    let ranges: Vec<(Id, Id)> = [1, 3, 8, 1]
        .into_iter()
        .map(|n| {
            advance(store, "fast", n)
                .map(|r| (r.first, r.last))
                .expect("advance")
        })
        .collect();

    assert_eq!(ranges, [(1, 1), (2, 4), (5, 12), (13, 13)]);
    assert_eq!(store.get("fast").map(|c| c.last_id()), Ok(Some(13)));
    let page = store
        .list(&page(10, None, None))
        .expect("list should succeed");
    assert_eq!(page.items[0].last_id(), Some(13));
}

pub fn reset_of_a_block_collection_moves_the_counter(store: &dyn SequenceStore) {
    create(
        store,
        CollectionSpec {
            durability: block(100),
            ..CollectionSpec::named("fast")
        },
    );
    advance(store, "fast", 5).expect("advance should succeed");

    let updated = store
        .update("fast", &|c| Ok(c.reset(Some(50), false)?.0))
        .expect("reset should succeed");
    let next = advance(store, "fast", 1).expect("advance should succeed");

    assert_eq!(updated.before.last_id(), Some(5));
    assert_eq!(next.first, 50);
}

pub fn deleted_block_collection_starts_again_from_start(store: &dyn SequenceStore) {
    let spec = CollectionSpec {
        durability: block(100),
        ..CollectionSpec::named("fast")
    };
    create(store, spec.clone());
    advance(store, "fast", 5).expect("advance should succeed");

    let deleted = store.delete("fast").expect("delete should succeed");
    create(store, spec);
    let next = advance(store, "fast", 1).expect("advance should succeed");

    assert_eq!(deleted.last_id(), Some(5));
    assert_eq!(next.first, 1);
}

pub fn idempotent_retry_on_a_block_collection_replays_the_original_range(
    store: &dyn SequenceStore,
) {
    create(
        store,
        CollectionSpec {
            durability: block(10),
            ..CollectionSpec::named("fast")
        },
    );
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let request = keyed("fast", 3, "key-1:xyz", "fast/3", now);

    let first = store.advance(&request).expect("first call");
    let retry = store.advance(&request).expect("retry");
    let mismatch = store.advance(&keyed("fast", 4, "key-1:xyz", "fast/4", now));
    let next = advance(store, "fast", 1).expect("advance should succeed");

    assert_eq!((first.range.first, first.range.last), (1, 3));
    assert_eq!(
        retry,
        Issued {
            replayed: true,
            ..first
        }
    );
    assert_eq!(mismatch, Err(StoreError::IdempotencyMismatch));
    assert_eq!(next.first, 4);
}
