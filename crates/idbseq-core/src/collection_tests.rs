use super::*;

fn count(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("non-zero")
}

fn step(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("non-zero")
}

fn create(spec: CollectionSpec) -> Collection {
    Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid spec")
}

fn orders() -> Collection {
    create(CollectionSpec::named("orders"))
}

fn issued(collection: &Collection, n: u32) -> Collection {
    collection.advance(count(n)).expect("advance").0
}

// --- create / validation -------------------------------------------------

#[test]
fn create_applies_documented_defaults() {
    let collection = orders();

    let spec = collection.spec();
    assert_eq!((spec.start, spec.step.get(), spec.max), (1, 1, Id::MAX));
    assert!(!spec.protected);
    assert_eq!(spec.durability, Durability::Strict);
    assert_eq!(collection.last_id(), None);
}

#[test]
fn create_reports_every_failing_field() {
    let spec = CollectionSpec {
        start: 10,
        max: 5,
        durability: Durability::Block {
            size: count(MAX_BLOCK_SIZE + 1),
        },
        ..CollectionSpec::named("Bad Name")
    };

    let errors = Collection::create(spec, SystemTime::UNIX_EPOCH).expect_err("invalid");

    let fields: Vec<&str> = errors.fields().keys().map(String::as_str).collect();
    assert_eq!(fields, ["durability", "max", "name"]);
}

#[test]
fn create_accepts_start_equal_to_max() {
    let collection = create(CollectionSpec {
        start: 7,
        max: 7,
        ..CollectionSpec::named("single")
    });

    assert_eq!(collection.next_id(), Some(7));
}

// --- advance / next_id -----------------------------------------------------

#[test]
fn next_id_previews_without_issuing() {
    let advanced = issued(&orders(), 3);

    assert_eq!(advanced.next_id(), Some(4));
    assert_eq!(advanced.next_id(), Some(4));
    assert_eq!(advanced.last_id(), Some(3));
}

#[test]
fn next_id_is_none_once_max_is_issued() {
    let exhausted = issued(
        &create(CollectionSpec {
            max: 3,
            ..CollectionSpec::named("small")
        }),
        3,
    );

    assert_eq!(exhausted.next_id(), None);
    assert_eq!(exhausted.advance(count(1)), Err(Exhausted));
}

#[test]
fn advance_returns_a_new_value_and_leaves_the_original_unchanged() {
    let original = orders();

    let (advanced, range) = original.advance(count(2)).expect("advance");

    assert_eq!(original.last_id(), None);
    assert_eq!(advanced.last_id(), Some(2));
    assert_eq!((range.first, range.last), (1, 2));
    assert_eq!(advanced.created_at(), original.created_at());
}

// --- apply (PATCH) ----------------------------------------------------------

#[test]
fn apply_changes_only_the_patched_fields() {
    let patch = CollectionPatch {
        max: Some(1000),
        protected: Some(true),
        ..CollectionPatch::default()
    };

    let patched = orders().apply(&patch).expect("valid patch");

    assert_eq!(patched.spec().max, 1000);
    assert!(patched.spec().protected);
    assert_eq!(patched.spec().start, 1);
}

#[test]
fn apply_allows_start_and_step_before_the_first_id() {
    let patch = CollectionPatch {
        start: Some(100),
        step: Some(step(5)),
        ..CollectionPatch::default()
    };

    let patched = orders().apply(&patch).expect("valid patch");

    assert_eq!(patched.next_id(), Some(100));
}

#[test]
fn apply_freezes_start_and_step_after_the_first_id_and_keeps_max_above_last_id() {
    let used = issued(&orders(), 10);
    let patch = CollectionPatch {
        start: Some(100),
        step: Some(step(5)),
        max: Some(9),
        ..CollectionPatch::default()
    };

    let errors = used.apply(&patch).expect_err("invalid patch");

    let fields: Vec<&str> = errors.fields().keys().map(String::as_str).collect();
    assert_eq!(fields, ["max", "start", "step"]);
}

#[test]
fn apply_accepts_unchanged_start_and_step_after_the_first_id() {
    let used = issued(&orders(), 1);
    let patch = CollectionPatch {
        start: Some(1),
        step: Some(step(1)),
        durability: Some(Durability::Block { size: count(100) }),
        ..CollectionPatch::default()
    };

    let patched = used.apply(&patch).expect("valid patch");

    assert_eq!(
        patched.spec().durability,
        Durability::Block { size: count(100) }
    );
    assert_eq!(patched.last_id(), Some(1));
}

// --- reset --------------------------------------------------------------------

#[test]
fn reset_to_start_by_default_needs_allow_rewind_once_ids_were_issued() {
    let used = issued(&orders(), 5);

    let refused = used.reset(None, false).expect_err("rewind not allowed");
    let (reset, outcome) = used.reset(None, true).expect("rewind allowed");

    assert_eq!(
        refused.fields().keys().collect::<Vec<_>>(),
        ["allow_rewind"]
    );
    assert_eq!(reset.last_id(), None);
    assert_eq!(
        outcome,
        ResetOutcome {
            previous_last_id: Some(5),
            last_id: None,
            next_id: Some(1),
        }
    );
}

#[test]
fn reset_forward_skips_ahead_without_allow_rewind() {
    let stepped = create(CollectionSpec {
        start: 1000,
        step: step(10),
        ..CollectionSpec::named("invoices")
    });

    let (reset, outcome) = issued(&stepped, 2)
        .reset(Some(2000), false)
        .expect("forward reset");

    assert_eq!(reset.next_id(), Some(2000));
    assert_eq!(
        (outcome.previous_last_id, outcome.last_id),
        (Some(1010), Some(1990))
    );
}

#[test]
fn reset_of_an_unused_collection_to_start_is_not_a_rewind() {
    let (_, outcome) = orders().reset(None, false).expect("nothing to rewind");

    assert_eq!(outcome.next_id, Some(1));
}

#[test]
fn reset_rejects_values_outside_the_range_or_off_the_step_grid() {
    let stepped = create(CollectionSpec {
        start: 10,
        step: step(5),
        max: 100,
        ..CollectionSpec::named("grid")
    });

    for value in [9, 101] {
        let errors = stepped.reset(Some(value), true).expect_err("out of range");
        assert_eq!(errors.fields()["value"], "must be within [start, max]");
    }
    let errors = stepped.reset(Some(12), true).expect_err("off grid");
    assert_eq!(
        errors.fields()["value"],
        "must be start plus a multiple of step"
    );
}

#[test]
fn reset_handles_steps_larger_than_the_id_range() {
    let huge = create(CollectionSpec {
        start: Id::MIN,
        step: step(u64::MAX),
        ..CollectionSpec::named("huge")
    });

    let (reset, outcome) = huge.reset(Some(Id::MAX), false).expect("on grid");

    assert_eq!(outcome.last_id, Some(Id::MIN));
    assert_eq!(reset.next_id(), Some(Id::MAX));
}

// --- serde ----------------------------------------------------------------------

#[test]
fn durability_serialises_with_a_mode_tag() {
    let block = Durability::Block { size: count(500) };

    let json = serde_json::to_string(&block).expect("serialise");
    let back: Durability = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(json, r#"{"mode":"block","size":500}"#);
    assert_eq!(back, block);
    assert_eq!(
        serde_json::to_string(&Durability::Strict).expect("serialise"),
        r#"{"mode":"strict"}"#
    );
}
