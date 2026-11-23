//! The embedded `Database` in one process: the API, reopening, copying the
//! file, and handing it to a server.
#![allow(clippy::expect_used)] // test code: a failed setup step should panic

use std::time::Duration;

use idbseq::{CollectionPatch, CollectionSpec, Database, Durability, Error, NextRequest, Options};
use idbseq_core::store::{GroupCommitConfig, RedbStore, SequenceStore, StoreError};
use tempfile::TempDir;

fn temp() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("app.idbseq");
    (dir, path)
}

fn block(name: &str, size: u32) -> CollectionSpec {
    CollectionSpec {
        durability: Durability::Block {
            size: size.try_into().expect("non-zero"),
        },
        ..CollectionSpec::named(name)
    }
}

#[test]
fn ids_increase_and_batches_are_contiguous() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("invoices"))
        .expect("create");

    let first = ids.next_id("invoices").expect("next");
    let batch = ids.next_ids("invoices", 10).expect("batch");
    let last = ids.last_id("invoices").expect("last");

    assert_eq!(first, 1);
    assert_eq!((batch.first, batch.last, batch.count.get()), (2, 11, 10));
    assert_eq!((last.last_id, last.next_id), (Some(11), Some(12)));
}

#[test]
fn collections_are_managed_like_on_the_server() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("a.one"))
        .expect("create");
    ids.create_collection(CollectionSpec::named("b.two"))
        .expect("create");
    ids.next_ids("a.one", 5).expect("batch");

    let page = ids
        .collections(None, None, Some("a.".into()))
        .expect("list");
    let updated = ids
        .update_collection(
            "a.one",
            &CollectionPatch {
                max: Some(100),
                ..CollectionPatch::default()
            },
        )
        .expect("update");
    let reset = ids
        .reset_collection("a.one", Some(50), false)
        .expect("reset");
    let deleted = ids.delete_collection("b.two").expect("delete");

    assert_eq!(
        page.items
            .iter()
            .map(idbseq::Collection::name)
            .collect::<Vec<_>>(),
        ["a.one"]
    );
    assert_eq!(updated.spec().max, 100);
    assert_eq!((reset.previous_last_id, reset.next_id), (Some(5), Some(50)));
    assert_eq!(deleted.name(), "b.two");
    assert!(matches!(
        ids.collection("b.two"),
        Err(Error::Store(StoreError::NotFound(name))) if name == "b.two"
    ));
}

#[test]
fn a_repeated_idempotency_key_returns_the_same_ids() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("orders"))
        .expect("create");
    let request = NextRequest {
        name: "orders".into(),
        count: Some(3),
        idempotency_key: Some("checkout-42".into()),
    };

    let first = ids.issue(&request).expect("issue");
    drop(ids);
    let again = Database::open(&path)
        .expect("reopen")
        .issue(&request)
        .expect("retry");

    assert_eq!(first.range, again.range);
    assert!(again.replayed);
}

#[test]
fn reopening_continues_the_sequence_in_both_modes() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("cards"))
        .expect("create");
    ids.create_collection(block("fast", 100)).expect("create");
    ids.next_ids("cards", 3).expect("batch");
    ids.next_ids("fast", 3).expect("batch");
    drop(ids);

    let shared = Database::open_with(&path, Options::shared()).expect("open shared");

    assert_eq!(shared.next_id("cards").expect("next"), 4);
    // The exclusive handle had reserved 1–100; the rest of the block is skipped.
    assert_eq!(shared.next_id("fast").expect("next"), 101);
    assert_eq!(shared.next_id("fast").expect("next"), 102);
}

#[test]
fn an_exclusive_handle_makes_others_wait_then_report_busy() {
    let (_dir, path) = temp();
    let _held = Database::open(&path).expect("open");
    let short = Options::default().busy_timeout(Duration::from_millis(50));

    let exclusive = Database::open_with(&path, short);
    let shared = Database::open_with(
        &path,
        Options::shared().busy_timeout(Duration::from_millis(50)),
    );

    assert!(
        matches!(exclusive, Err(Error::Busy { waited, .. }) if waited >= Duration::from_millis(50))
    );
    assert!(matches!(shared, Err(Error::Busy { .. })));
}

#[test]
fn a_shared_handle_lets_go_of_the_file_between_calls() {
    let (_dir, path) = temp();
    let shared = Database::open_with(&path, Options::shared()).expect("open shared");
    shared
        .create_collection(CollectionSpec::named("cards"))
        .expect("create");

    let exclusive = Database::open(&path).expect("the file is free");
    exclusive.next_id("cards").expect("next");
    drop(exclusive);

    assert_eq!(shared.next_id("cards").expect("next"), 2);
}

#[test]
fn a_missing_folder_is_an_open_error() {
    let (dir, _) = temp();

    let opened = Database::open(dir.path().join("no-such-folder").join("app.idbseq"));

    assert!(matches!(opened, Err(Error::Open(_))));
}

#[test]
fn a_copied_file_carries_on_where_the_original_stopped() {
    let (dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("cards"))
        .expect("create");
    ids.next_ids("cards", 41).expect("batch");
    drop(ids);
    let copy = dir.path().join("copy.idbseq");
    std::fs::copy(&path, &copy).expect("copy");

    assert_eq!(
        Database::open(&copy)
            .expect("open copy")
            .next_id("cards")
            .expect("next"),
        42
    );
}

#[test]
fn a_server_can_take_over_an_embedded_file() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("cards"))
        .expect("create");
    ids.next_ids("cards", 9).expect("batch");
    drop(ids);
    let data_dir = tempfile::tempdir().expect("data dir");
    std::fs::copy(&path, data_dir.path().join("idbseq.redb")).expect("copy");

    let server_store =
        RedbStore::open(data_dir.path(), GroupCommitConfig::default()).expect("server opens it");

    let collection = server_store.get("cards").expect("collection");
    assert_eq!(collection.last_id(), Some(9));
}

#[test]
fn threads_share_one_handle_without_repeating_an_id() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("strict"))
        .expect("create");
    ids.create_collection(block("fast", 64)).expect("create");

    let issued = std::thread::scope(|scope| {
        let workers = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    (0..100)
                        .flat_map(|_| {
                            [
                                ids.next_id("strict").expect("next"),
                                -ids.next_id("fast").expect("next"),
                            ]
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("worker"))
            .collect::<Vec<_>>()
    });

    let mut sorted = issued.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), issued.len(), "an ID was issued twice");
    assert_eq!(issued.len(), 1600);
}

#[test]
fn a_shared_file_deleted_while_open_is_reported_not_recreated() {
    let (_dir, path) = temp();
    let ids = Database::open_with(&path, Options::shared()).expect("open");
    ids.create_collection(CollectionSpec::named("cards"))
        .expect("create");
    assert_eq!(ids.next_id("cards").expect("next"), 1);

    std::fs::remove_file(&path).expect("delete");

    // An app that creates its collections when missing would get 1 again.
    let created = ids.create_collection(CollectionSpec::named("cards"));
    assert!(matches!(
        created,
        Err(Error::Open(idbseq::OpenError::Missing(_)))
    ));
    assert!(!path.exists());
}

#[test]
fn idempotency_keys_follow_the_server_rules_and_their_own_scope() {
    let (_dir, path) = temp();
    let ids = Database::open(&path).expect("open");
    ids.create_collection(CollectionSpec::named("cards"))
        .expect("create");
    let with_key = |key: &str| NextRequest {
        name: "cards".to_owned(),
        count: None,
        idempotency_key: Some(key.to_owned()),
    };

    for bad in [
        String::new(),
        "x".repeat(256),
        "ü ü".to_owned(),
        "tab\tkey".to_owned(),
    ] {
        let issued = ids.issue(&with_key(&bad));
        assert!(
            matches!(issued, Err(Error::Store(StoreError::Invalid(ref errors))) if errors.fields().contains_key("idempotency_key")),
            "{bad:?}: {issued:?}"
        );
    }
    let first = ids.issue(&with_key("k1")).expect("issue");
    let again = ids.issue(&with_key("k1")).expect("replay");

    assert_eq!(first.range, again.range);
    assert!(again.replayed);
    drop(ids);
    // Stored under the embedded scope, apart from the server's `admin:` and
    // `key:<id>:`: the server's admin sending `k1` gets new IDs, not these.
    let store = RedbStore::open_file(
        &path,
        GroupCommitConfig::default(),
        idbseq_core::store::BlockCache::On,
        idbseq_core::store::IfMissing::Fail,
    )
    .expect("reopen");
    let engine = idbseq_core::engine::Engine::new(
        std::sync::Arc::new(store),
        idbseq_core::engine::Limits::default(),
    );
    let as_server_admin = engine
        .next(&with_key("admin:k1"), std::time::SystemTime::now())
        .expect("next");
    assert!(!as_server_admin.replayed);
    assert_ne!(as_server_admin.range, first.range);
}
