use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::*;
use crate::collection::DEFAULT_MAX;
use crate::store::InMemoryStore;

fn engine() -> Engine {
    Engine::new(Arc::new(InMemoryStore::new()), Limits::default())
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn with(engine: &Engine, spec: CollectionSpec) -> Collection {
    engine.create(spec, now()).expect("create")
}

fn single(name: &str) -> NextRequest {
    NextRequest {
        name: name.into(),
        count: None,
        idempotency_key: None,
    }
}

fn batch(name: &str, count: u32) -> NextRequest {
    NextRequest {
        count: Some(count),
        ..single(name)
    }
}

fn first_last(engine: &Engine, request: &NextRequest) -> Result<(Id, Id), StoreError> {
    engine
        .next(request, now())
        .map(|i| (i.range.first, i.range.last))
}

fn field_of(error: StoreError) -> Vec<String> {
    match error {
        StoreError::Invalid(errors) => errors.fields().keys().cloned().collect(),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

// US-0007 ---------------------------------------------------------------------

#[test]
fn first_next_returns_start_and_later_calls_add_step() {
    let engine = engine();
    with(
        &engine,
        CollectionSpec {
            start: 1000,
            step: std::num::NonZeroU64::new(10).expect("non-zero"),
            ..CollectionSpec::named("invoices")
        },
    );

    let ids: Vec<Id> = (0..3)
        .map(|_| first_last(&engine, &single("invoices")).expect("next").0)
        .collect();

    assert_eq!(ids, [1000, 1010, 1020]);
}

#[test]
fn next_past_max_fails_with_exhausted_and_does_not_move_the_counter() {
    let engine = engine();
    with(
        &engine,
        CollectionSpec {
            max: 2,
            ..CollectionSpec::named("tiny")
        },
    );
    first_last(&engine, &single("tiny")).expect("1");
    first_last(&engine, &single("tiny")).expect("2");

    let exhausted = first_last(&engine, &single("tiny"));

    assert_eq!(exhausted, Err(StoreError::Exhausted("tiny".into())));
    assert_eq!(engine.last("tiny").map(|l| l.last_id), Ok(Some(2)));
}

#[test]
fn next_on_an_unknown_collection_fails_with_not_found() {
    assert_eq!(
        first_last(&engine(), &single("missing")),
        Err(StoreError::NotFound("missing".into()))
    );
}

// US-0008 ---------------------------------------------------------------------

#[test]
fn batches_are_contiguous_and_limited_to_max_batch() {
    let engine = Engine::new(
        Arc::new(InMemoryStore::new()),
        Limits {
            max_batch: 100,
            ..Limits::default()
        },
    );
    with(&engine, CollectionSpec::named("orders"));

    let ok = first_last(&engine, &batch("orders", 100));
    let too_big = engine
        .next(&batch("orders", 101), now())
        .expect_err("over limit");
    let zero = engine.next(&batch("orders", 0), now()).expect_err("zero");

    assert_eq!(ok, Ok((1, 100)));
    assert_eq!(field_of(too_big), ["count"]);
    assert_eq!(field_of(zero), ["count"]);
}

#[test]
fn a_batch_that_would_pass_max_fails_as_a_whole() {
    let engine = engine();
    with(
        &engine,
        CollectionSpec {
            max: 10,
            ..CollectionSpec::named("orders")
        },
    );

    let result = first_last(&engine, &batch("orders", 11));

    assert_eq!(result, Err(StoreError::Exhausted("orders".into())));
    assert_eq!(engine.last("orders").map(|l| l.last_id), Ok(None));
}

#[test]
fn the_default_batch_limit_is_ten_thousand() {
    assert_eq!(Limits::default().max_batch, 10_000);
}

// US-0009 ---------------------------------------------------------------------

#[test]
fn last_previews_the_next_id_without_reserving_it() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    let before = engine.last("orders").expect("last");
    first_last(&engine, &batch("orders", 5)).expect("next");

    let after = engine.last("orders").expect("last");
    let again = engine.last("orders").expect("last");

    assert_eq!((before.last_id, before.next_id), (None, Some(1)));
    assert_eq!((after.last_id, after.next_id), (Some(5), Some(6)));
    assert_eq!(again, after);
}

#[test]
fn last_of_an_exhausted_collection_has_no_next_id() {
    let engine = engine();
    with(
        &engine,
        CollectionSpec {
            max: 1,
            ..CollectionSpec::named("one")
        },
    );
    first_last(&engine, &single("one")).expect("next");

    assert_eq!(engine.last("one").map(|l| l.next_id), Ok(None));
}

// US-0011 ---------------------------------------------------------------------

#[test]
fn reset_defaults_to_start_and_reports_the_previous_last_id() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    first_last(&engine, &batch("orders", 9)).expect("next");

    let refused = engine.reset("orders", None, false).expect_err("rewind");
    let outcome = engine.reset("orders", None, true).expect("reset");

    assert_eq!(field_of(refused), ["allow_rewind"]);
    assert_eq!(outcome.previous_last_id, Some(9));
    assert_eq!((outcome.last_id, outcome.next_id), (None, Some(1)));
    assert_eq!(first_last(&engine, &single("orders")), Ok((1, 1)));
}

#[test]
fn reset_to_an_explicit_value_skips_ahead() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    first_last(&engine, &batch("orders", 9)).expect("next");

    let outcome = engine.reset("orders", Some(500), false).expect("reset");

    assert_eq!(outcome.next_id, Some(500));
    assert_eq!(first_last(&engine, &single("orders")), Ok((500, 500)));
}

#[test]
fn reset_of_an_unknown_collection_fails_with_not_found() {
    assert_eq!(
        engine().reset("missing", None, true),
        Err(StoreError::NotFound("missing".into()))
    );
}

// US-0012 ---------------------------------------------------------------------

#[test]
fn delete_removes_the_collection_and_a_new_one_starts_from_its_own_start() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    first_last(&engine, &batch("orders", 9)).expect("next");

    let deleted = engine.delete("orders").expect("delete");
    let gone = engine.get("orders");
    with(
        &engine,
        CollectionSpec {
            start: 100,
            ..CollectionSpec::named("orders")
        },
    );

    assert_eq!(deleted.last_id(), Some(9));
    assert_eq!(gone, Err(StoreError::NotFound("orders".into())));
    assert_eq!(first_last(&engine, &single("orders")), Ok((100, 100)));
    assert_eq!(
        engine.delete("nope"),
        Err(StoreError::NotFound("nope".into()))
    );
}

// US-0013 ---------------------------------------------------------------------

#[test]
fn list_defaults_to_fifty_and_caps_at_five_hundred() {
    let engine = engine();
    for i in 0..60 {
        with(&engine, CollectionSpec::named(format!("c{i:02}")));
    }

    let default_page = engine.list(None, None, None).expect("list");
    let too_big = engine.list(Some(501), None, None).expect_err("limit");
    let zero = engine.list(Some(0), None, None).expect_err("limit");

    assert_eq!(default_page.items.len(), 50);
    assert_eq!(default_page.total, 60);
    assert_eq!(default_page.next_cursor.as_deref(), Some("c49"));
    assert_eq!(field_of(too_big), ["limit"]);
    assert_eq!(field_of(zero), ["limit"]);
}

#[test]
fn list_items_carry_every_field_the_api_shows() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    first_last(&engine, &single("orders")).expect("next");

    let page = engine
        .list(Some(10), None, Some("ord".into()))
        .expect("list");

    let item = &page.items[0];
    assert_eq!(
        (item.name(), item.last_id(), item.next_id()),
        ("orders", Some(1), Some(2))
    );
    assert_eq!((item.spec().step.get(), item.spec().max), (1, DEFAULT_MAX));
    assert_eq!(item.created_at(), now());
}

// update + idempotency --------------------------------------------------------------

#[test]
fn update_validates_the_patch() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    first_last(&engine, &batch("orders", 5)).expect("next");
    let patch = CollectionPatch {
        max: Some(4),
        ..CollectionPatch::default()
    };

    let error = engine.update("orders", &patch).expect_err("below last_id");

    assert_eq!(field_of(error), ["max"]);
}

#[test]
fn an_idempotent_retry_returns_the_original_ids() {
    let engine = engine();
    with(&engine, CollectionSpec::named("orders"));
    let request = NextRequest {
        idempotency_key: Some("key-1:retry".into()),
        ..batch("orders", 3)
    };

    let first = engine.next(&request, now()).expect("first");
    let retry = engine.next(&request, now()).expect("retry");
    let same_key_single = engine.next(
        &NextRequest {
            count: None,
            ..request.clone()
        },
        now(),
    );

    assert_eq!((retry.range, retry.replayed), (first.range, true));
    assert_eq!(same_key_single, Err(StoreError::IdempotencyMismatch));
    assert_eq!(
        engine.purge_idempotency(now() + DEFAULT_IDEMPOTENCY_TTL),
        Ok(1)
    );
}
