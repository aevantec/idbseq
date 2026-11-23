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

/// Takes `TAKES_EACH` IDs from `name` on each of `TAKERS` threads while `change`
/// runs over and over on this one, until they are done. Returns every ID issued.
fn ids_taken_while(temp: &TempStore, name: &str, change: impl Fn(u32)) -> Vec<Id> {
    const TAKERS: usize = 8;
    const TAKES_EACH: usize = 60;
    let finished = std::sync::atomic::AtomicUsize::new(0);
    thread::scope(|scope| {
        let takers: Vec<_> = (0..TAKERS)
            .map(|_| {
                scope.spawn(|| {
                    let ids: Vec<Id> = (0..TAKES_EACH)
                        .map(|_| next(temp, name, 1).expect("advance").0)
                        .collect();
                    finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    ids
                })
            })
            .collect();
        let mut round = 0;
        while finished.load(std::sync::atomic::Ordering::Relaxed) < TAKERS {
            change(round);
            round += 1;
            thread::yield_now();
        }
        takers
            .into_iter()
            .flat_map(|taker| taker.join().expect("taker"))
            .collect()
    })
}

fn assert_unique(ids: &[Id]) {
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "{} IDs issued, {} unique",
        ids.len(),
        unique.len()
    );
}

#[test]
fn changing_settings_while_ids_are_taken_never_repeats_an_id() {
    let temp = TempStore::new();
    create(&temp, CollectionSpec::named("orders"));

    let ids = ids_taken_while(&temp, "orders", |round| {
        let patch = crate::collection::CollectionPatch {
            protected: Some(round % 2 == 0),
            ..Default::default()
        };
        temp.update("orders", &|c| Ok(c.apply(&patch)?))
            .expect("update");
    });

    assert_unique(&ids);
}

#[test]
fn switching_durability_while_ids_are_taken_never_repeats_an_id() {
    let temp = TempStore::new();
    create(&temp, CollectionSpec::named("orders"));

    let ids = ids_taken_while(&temp, "orders", |round| {
        let durability = if round % 2 == 0 {
            Durability::Block { size: count(50) }
        } else {
            Durability::Strict
        };
        let patch = crate::collection::CollectionPatch {
            durability: Some(durability),
            ..Default::default()
        };
        temp.update("orders", &|c| Ok(c.apply(&patch)?))
            .expect("update");
    });

    assert_unique(&ids);
}

#[test]
fn a_block_that_reaches_max_stays_on_the_step_grid_after_a_restart() {
    let temp = TempStore::new();
    create(
        &temp,
        CollectionSpec {
            step: NonZeroU64::new(10).expect("non-zero"),
            max: 100,
            durability: Durability::Block { size: count(1000) },
            ..CollectionSpec::named("grid")
        },
    );
    next(&temp, "grid", 1).expect("advance");
    let temp = temp.reopen();
    let more_room = crate::collection::CollectionPatch {
        max: Some(1000),
        ..Default::default()
    };

    temp.update("grid", &|c| Ok(c.apply(&more_room)?))
        .expect("update");

    // 1, 11, … 91 is the grid under the old max: the next ID is 101, not 110.
    assert_eq!(next(&temp, "grid", 1), Ok((101, 101)));
}

#[cfg(unix)]
#[test]
fn the_data_directory_and_file_are_private_to_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("data");
    std::fs::create_dir(&data).expect("create");
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let store = RedbStore::open(&data, GroupCommitConfig::default()).expect("open");

    assert_eq!(mode(&data), 0o700);
    assert_eq!(mode(&data.join(DATABASE_FILE)), 0o600);
    drop(store);
}

fn open_file(path: &std::path::Path, block_cache: BlockCache) -> RedbStore {
    RedbStore::open_file(
        path,
        GroupCommitConfig::default(),
        block_cache,
        IfMissing::Create,
    )
    .expect("open file")
}

fn block_spec(name: &str, size: u32) -> CollectionSpec {
    CollectionSpec {
        durability: Durability::Block { size: count(size) },
        ..CollectionSpec::named(name)
    }
}

#[test]
fn a_store_opens_any_file_path_and_continues_after_reopening() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("cards.idbseq");
    let store = open_file(&path, BlockCache::On);
    create(&store, CollectionSpec::named("cards"));
    next(&store, "cards", 3).expect("advance");
    drop(store);

    let store = open_file(&path, BlockCache::On);

    assert_eq!(next(&store, "cards", 1), Ok((4, 4)));
    assert!(path.is_file());
}

#[test]
fn a_file_held_by_another_store_is_refused_as_in_use() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("cards.idbseq");
    let _held = open_file(&path, BlockCache::On);

    let second = RedbStore::open_file(
        &path,
        GroupCommitConfig::default(),
        BlockCache::On,
        IfMissing::Create,
    );

    assert!(matches!(second, Err(OpenError::FileInUse(p)) if p == path));
}

#[test]
fn a_missing_file_is_refused_when_it_may_not_be_created() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("gone.idbseq");

    let opened = RedbStore::open_file(
        &path,
        GroupCommitConfig::default(),
        BlockCache::Off,
        IfMissing::Fail,
    );

    assert!(matches!(opened, Err(OpenError::Missing(p)) if p == path));
    assert!(!path.exists(), "nothing was created");
}

#[test]
fn another_programs_redb_file_is_refused_and_left_unchanged() {
    const OTHER: ::redb::TableDefinition<&str, u64> = ::redb::TableDefinition::new("users");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("other.redb");
    {
        let other = Database::create(&path).expect("other program's file");
        let txn = other.begin_write().expect("write");
        txn.open_table(OTHER)
            .expect("table")
            .insert("ada", 1)
            .expect("insert");
        txn.commit().expect("commit");
    }

    let opened = RedbStore::open_file(
        &path,
        GroupCommitConfig::default(),
        BlockCache::On,
        IfMissing::Create,
    );

    assert!(matches!(opened, Err(OpenError::NotIdbseq(_))));
    let other = Database::open(&path).expect("still a redb file");
    let read = other.begin_read().expect("read");
    let tables = read
        .list_tables()
        .expect("tables")
        .map(|table| ::redb::TableHandle::name(&table).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(tables, ["users"], "no idbseq tables were added");
}

#[cfg(unix)]
#[test]
fn a_new_file_is_private_and_an_existing_file_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("cards.idbseq");
    drop(open_file(&path, BlockCache::On));
    assert_eq!(mode(&path), 0o600);

    // Shared with a group on purpose (a web user and a cron job, say).
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).expect("chmod");
    drop(open_file(&path, BlockCache::Off));

    assert_eq!(mode(&path), 0o660);
}

#[test]
fn without_the_block_cache_closing_the_file_loses_no_ids() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("cards.idbseq");
    create(&open_file(&path, BlockCache::Off), block_spec("fast", 100));

    let ids = (0..3)
        .map(|_| next(&open_file(&path, BlockCache::Off), "fast", 1).expect("advance"))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec![(1, 1), (2, 2), (3, 3)]);
}

#[test]
fn without_the_block_cache_ids_continue_after_a_cached_reservation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("cards.idbseq");
    let cached = open_file(&path, BlockCache::On);
    create(&cached, block_spec("fast", 100));
    next(&cached, "fast", 7).expect("advance");
    drop(cached);

    let uncached = open_file(&path, BlockCache::Off);

    assert_eq!(next(&uncached, "fast", 1), Ok((101, 101)));
    assert_eq!(
        uncached.persisted("fast").map(|c| c.last_id()),
        Ok(Some(101))
    );
}
