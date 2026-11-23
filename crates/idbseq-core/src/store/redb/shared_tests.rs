use std::num::NonZeroU32;
use std::time::{Duration, SystemTime};

use super::*;
use crate::collection::{CollectionSpec, Durability};

/// A new idbseq file in its own temporary directory. A shared store needs an
/// existing file; `open_waiting` creates one, as the first open of an app does.
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("shared.idbseq");
    drop(open_waiting(&path, BlockCache::Off, DEFAULT_BUSY_TIMEOUT).expect("create"));
    (dir, path)
}

fn next(store: &dyn SequenceStore, name: &str) -> Result<i64, StoreError> {
    let one = NonZeroU32::new(1).expect("non-zero");
    store
        .advance(&AdvanceRequest::new(name, one))
        .map(|issued| issued.range.first)
}

/// A shared store on a file in its own temporary directory.
struct TempShared {
    store: SharedFileStore,
    _dir: tempfile::TempDir,
}

impl TempShared {
    fn new() -> Self {
        let (dir, path) = temp();
        Self {
            store: SharedFileStore::new(path, DEFAULT_BUSY_TIMEOUT),
            _dir: dir,
        }
    }
}

impl std::ops::Deref for TempShared {
    type Target = SharedFileStore;
    fn deref(&self) -> &SharedFileStore {
        &self.store
    }
}

sequence_store_conformance!(TempShared::new, without_heavy_concurrency);

#[test]
fn two_stores_on_one_file_take_turns_without_repeating_an_id() {
    let (_dir, path) = temp();
    let a = SharedFileStore::new(&path, DEFAULT_BUSY_TIMEOUT);
    let b = SharedFileStore::new(&path, DEFAULT_BUSY_TIMEOUT);
    let spec = CollectionSpec {
        durability: Durability::Block {
            size: NonZeroU32::new(100).expect("non-zero"),
        },
        ..CollectionSpec::named("cards")
    };
    a.create(Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid"))
        .expect("create");

    let ids = [next(&a, "cards"), next(&b, "cards"), next(&a, "cards")];

    assert_eq!(ids, [Ok(1), Ok(2), Ok(3)]);
}

#[test]
fn a_held_file_is_busy_after_the_timeout() {
    let (_dir, path) = temp();
    let _held = open_waiting(&path, BlockCache::On, DEFAULT_BUSY_TIMEOUT).expect("hold");
    let store = SharedFileStore::new(&path, Duration::from_millis(30));

    let result = store.get("cards");

    assert!(
        matches!(result, Err(StoreError::Busy { waited, .. }) if waited >= Duration::from_millis(30))
    );
}

#[test]
fn a_missing_file_is_unavailable_and_never_created() {
    let (dir, _) = temp();
    let path = dir.path().join("never-created.idbseq");
    let store = SharedFileStore::new(&path, DEFAULT_BUSY_TIMEOUT);

    assert!(matches!(
        store.get("cards"),
        Err(StoreError::Unavailable(OpenError::Missing(_)))
    ));
    assert!(!path.exists());
}

#[test]
fn a_file_deleted_while_in_use_is_not_replaced_by_an_empty_one() {
    let temp = TempShared::new();
    temp.create(
        Collection::create(CollectionSpec::named("cards"), SystemTime::UNIX_EPOCH).expect("valid"),
    )
    .expect("create");
    assert_eq!(next(&*temp, "cards"), Ok(1));

    std::fs::remove_file(temp.path()).expect("delete");

    // An app that creates its collections when they are missing would
    // otherwise get ID 1 again from a fresh, empty file.
    assert!(matches!(
        temp.create(
            Collection::create(CollectionSpec::named("cards"), SystemTime::UNIX_EPOCH)
                .expect("valid")
        ),
        Err(StoreError::Unavailable(OpenError::Missing(_)))
    ));
    assert!(!temp.path().exists());
}

#[test]
fn a_relative_path_is_fixed_when_the_store_is_made() {
    let store = SharedFileStore::new("ids.idbseq", DEFAULT_BUSY_TIMEOUT);

    assert!(store.path().is_absolute());
    assert!(store.path().ends_with("ids.idbseq"));
}

#[test]
fn the_busy_timeout_covers_the_wait_for_a_turn_too() {
    let (_dir, path) = temp();
    let _held = open_waiting(&path, BlockCache::On, DEFAULT_BUSY_TIMEOUT).expect("hold");
    let timeout = Duration::from_millis(200);
    let store = SharedFileStore::new(&path, timeout);

    let waits = std::thread::scope(|scope| {
        let callers = (0..3)
            .map(|_| {
                scope.spawn(|| {
                    let started = std::time::Instant::now();
                    let result = store.get("cards");
                    (
                        matches!(result, Err(StoreError::Busy { .. })),
                        started.elapsed(),
                    )
                })
            })
            .collect::<Vec<_>>();
        callers
            .into_iter()
            .map(|caller| caller.join().expect("caller"))
            .collect::<Vec<_>>()
    });

    // Queued callers do not each get a fresh timeout (200, 400, 600 ms).
    for (is_busy, waited) in waits {
        assert!(is_busy);
        assert!(waited < timeout + Duration::from_millis(150), "{waited:?}");
    }
}

#[test]
fn threads_sharing_a_store_never_repeat_an_id() {
    let temp = TempShared::new();
    temp.create(
        Collection::create(CollectionSpec::named("cards"), SystemTime::UNIX_EPOCH).expect("valid"),
    )
    .expect("create");

    let mut ids = std::thread::scope(|scope| {
        let workers = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    (0..25)
                        .map(|_| next(&*temp, "cards").expect("advance"))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("worker"))
            .collect::<Vec<_>>()
    });

    ids.sort_unstable();
    assert_eq!(ids, (1..=100).collect::<Vec<_>>());
}
