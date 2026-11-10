use std::num::{NonZeroU32, NonZeroU64};
use std::ops::Deref;
use std::thread;
use std::time::SystemTime;

use tempfile::TempDir;

use super::*;
use crate::collection::{CollectionSpec, Durability, Id};

/// A store in its own temporary directory. Fields drop in order: store, then dir.
pub(crate) struct TempStore {
    store: RedbStore,
    dir: TempDir,
}

impl TempStore {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = RedbStore::open(dir.path(), GroupCommitConfig::default()).expect("open");
        Self { store, dir }
    }

    /// Close the store and open the same directory again, like a restart.
    pub fn reopen(self) -> Self {
        let Self { store, dir } = self;
        drop(store);
        let store = RedbStore::open(dir.path(), GroupCommitConfig::default()).expect("reopen");
        Self { store, dir }
    }
}

impl Deref for TempStore {
    type Target = RedbStore;
    fn deref(&self) -> &RedbStore {
        &self.store
    }
}

sequence_store_conformance!(TempStore::new);

fn count(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("non-zero")
}

fn create(store: &RedbStore, spec: CollectionSpec) {
    let collection = Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid");
    store.create(collection).expect("create");
}

fn next(store: &RedbStore, name: &str, n: u32) -> Result<(Id, Id), StoreError> {
    store
        .advance(&AdvanceRequest::new(name, count(n)))
        .map(|issued| (issued.range.first, issued.range.last))
}

#[test]
fn restart_continues_after_the_last_issued_id() {
    let temp = TempStore::new();
    create(&temp, CollectionSpec::named("orders"));
    next(&temp, "orders", 41).expect("advance");

    let temp = temp.reopen();

    assert_eq!(next(&temp, "orders", 1), Ok((42, 42)));
}

#[test]
fn a_second_open_of_the_same_directory_fails_clearly() {
    let temp = TempStore::new();

    let second = RedbStore::open(temp.dir.path(), GroupCommitConfig::default());

    let error = second.expect_err("directory is locked");
    assert!(matches!(error, OpenError::AlreadyOpen(_)));
    assert!(
        error
            .to_string()
            .contains("already in use by another idbseqd process")
    );
}

#[test]
fn the_schema_version_is_recorded_and_unknown_versions_are_refused() {
    let temp = TempStore::new();
    let db = temp.database();
    let read = db.begin_read().expect("read");
    let version = read
        .open_table(META)
        .expect("meta")
        .get(SCHEMA_KEY)
        .expect("get")
        .map(|v| v.value());
    assert_eq!(version, Some(SCHEMA_VERSION));
    drop(read);
    let write = db.begin_write().expect("write");
    write
        .open_table(META)
        .expect("meta")
        .insert(SCHEMA_KEY, 99)
        .expect("insert");
    write.commit().expect("commit");
    drop(db);
    let TempStore { store, dir } = temp;
    drop(store);

    let reopened = RedbStore::open(dir.path(), GroupCommitConfig::default());

    assert!(matches!(
        reopened,
        Err(OpenError::SchemaVersion {
            found: 99,
            expected: SCHEMA_VERSION
        })
    ));
}

#[test]
fn block_mode_writes_the_disk_once_per_block() {
    let temp = TempStore::new();
    create(
        &temp,
        CollectionSpec {
            durability: Durability::Block { size: count(100) },
            ..CollectionSpec::named("fast")
        },
    );

    for _ in 0..50 {
        next(&temp, "fast", 1).expect("advance");
    }

    assert_eq!(temp.persisted("fast").map(|c| c.last_id()), Ok(Some(100)));
    assert_eq!(temp.get("fast").map(|c| c.last_id()), Ok(Some(50)));
}

#[test]
fn block_mode_skips_the_unused_rest_of_a_block_after_a_restart() {
    let temp = TempStore::new();
    create(
        &temp,
        CollectionSpec {
            durability: Durability::Block { size: count(100) },
            ..CollectionSpec::named("fast")
        },
    );
    next(&temp, "fast", 7).expect("advance");

    let temp = temp.reopen();

    assert_eq!(next(&temp, "fast", 1), Ok((101, 101)));
}

#[test]
fn a_failed_group_commit_fails_the_request_and_issues_nothing() {
    let temp = TempStore::new();
    create(&temp, CollectionSpec::named("orders"));
    next(&temp, "orders", 3).expect("advance");
    temp.fail_next_commit();

    let failed = next(&temp, "orders", 5);
    let after = next(&temp, "orders", 1);

    assert!(matches!(failed, Err(StoreError::Backend(_))));
    assert_eq!(after, Ok((4, 4)));
}

#[test]
fn concurrent_requests_share_group_commits() {
    const THREADS: u32 = 32;
    const CALLS: u32 = 50;
    let temp = TempStore::new();
    for name in ["a", "b", "c", "d"] {
        create(&temp, CollectionSpec::named(name));
    }
    let store: &RedbStore = &temp;

    thread::scope(|scope| {
        for t in 0..THREADS {
            scope.spawn(move || {
                let name = ["a", "b", "c", "d"][(t % 4) as usize];
                for _ in 0..CALLS {
                    next(store, name, 1).expect("advance");
                }
            });
        }
    });

    let per_collection = Id::from(THREADS / 4 * CALLS);
    for name in ["a", "b", "c", "d"] {
        assert_eq!(
            temp.get(name).map(|c| c.last_id()),
            Ok(Some(per_collection))
        );
    }
    let requests = u64::from(THREADS * CALLS);
    assert!(
        temp.group_commits() < requests,
        "expected grouping: {} commits for {requests} requests",
        temp.group_commits()
    );
}

#[test]
fn switching_a_block_collection_to_strict_keeps_the_sequence_unique() {
    let temp = TempStore::new();
    create(
        &temp,
        CollectionSpec {
            durability: Durability::Block { size: count(100) },
            ..CollectionSpec::named("fast")
        },
    );
    next(&temp, "fast", 5).expect("advance");
    let strict = crate::collection::CollectionPatch {
        durability: Some(Durability::Strict),
        ..Default::default()
    };

    temp.update("fast", &|c| Ok(c.apply(&strict)?))
        .expect("update");
    let after_switch = next(&temp, "fast", 1);
    let temp = temp.reopen();

    assert_eq!(after_switch, Ok((6, 6)));
    assert_eq!(next(&temp, "fast", 1), Ok((7, 7)));
}

#[test]
fn ids_near_the_top_of_the_range_are_exhausted_cleanly_in_block_mode() {
    let temp = TempStore::new();
    create(
        &temp,
        CollectionSpec {
            start: Id::MAX - 2,
            step: NonZeroU64::MIN,
            durability: Durability::Block { size: count(1000) },
            ..CollectionSpec::named("edge")
        },
    );

    let all = next(&temp, "edge", 3);
    let more = next(&temp, "edge", 1);

    assert_eq!(all, Ok((Id::MAX - 2, Id::MAX)));
    assert_eq!(more, Err(StoreError::Exhausted("edge".into())));
}

#[test]
fn reads_are_not_blocked_while_a_writer_holds_the_database() {
    let temp = TempStore::new();
    create(&temp, CollectionSpec::named("orders"));
    next(&temp, "orders", 3).expect("advance");
    let db = temp.database();
    let long_write = db.begin_write().expect("writer");

    let read = temp.get("orders").map(|c| c.last_id());
    let listed = temp
        .list(&crate::store::PageRequest {
            limit: std::num::NonZeroUsize::MIN,
            cursor: None,
            prefix: None,
        })
        .map(|p| p.total);

    drop(long_write);
    assert_eq!(read, Ok(Some(3)));
    assert_eq!(listed, Ok(1));
}
