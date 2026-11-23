//! A store that opens its file for each call, so several processes can take
//! turns on one file (embedded shared mode, `idbseqd console`).
//!
//! redb lets one process hold a file at a time. Each call here waits for the
//! file, opens it, runs in one transaction and closes it. Block collections
//! are written like strict ones ([`BlockCache::Off`]): a cached block would be
//! lost at every close.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use super::{BlockCache, GroupCommitConfig, IfMissing, OpenError, RedbStore};
use crate::collection::Collection;
use crate::store::{
    AdvanceRequest, Change, Issued, Page, PageRequest, SequenceStore, StoreError, Updated,
};

/// How long a call waits for a file another process holds, by default.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const FIRST_RETRY_DELAY: Duration = Duration::from_millis(1);
const MAX_RETRY_DELAY: Duration = Duration::from_millis(20);

/// Open (or create) `path` with [`RedbStore::open_file`], retrying for up to
/// `busy_timeout` while another process holds it.
///
/// # Errors
///
/// [`StoreError::Busy`] when the timeout passes, [`StoreError::Unavailable`]
/// when the file cannot be opened at all.
pub fn open_waiting(
    path: &Path,
    block_cache: BlockCache,
    busy_timeout: Duration,
) -> Result<RedbStore, StoreError> {
    let deadline = Deadline::new(path, busy_timeout);
    open_until(&deadline, block_cache, IfMissing::Create)
}

/// When a call stops waiting, measured from when it started.
struct Deadline<'a> {
    path: &'a Path,
    started: Instant,
    timeout: Duration,
}

impl<'a> Deadline<'a> {
    fn new(path: &'a Path, timeout: Duration) -> Self {
        Self {
            path,
            started: Instant::now(),
            timeout,
        }
    }

    /// Time left, or [`StoreError::Busy`] once there is none.
    fn left(&self) -> Result<Duration, StoreError> {
        let waited = self.started.elapsed();
        self.timeout
            .checked_sub(waited)
            .filter(|left| !left.is_zero())
            .ok_or_else(|| StoreError::Busy {
                path: self.path.to_path_buf(),
                waited,
            })
    }
}

fn open_until(
    deadline: &Deadline<'_>,
    block_cache: BlockCache,
    if_missing: IfMissing,
) -> Result<RedbStore, StoreError> {
    let mut delay = FIRST_RETRY_DELAY;
    loop {
        match RedbStore::open_file(
            deadline.path,
            GroupCommitConfig::default(),
            block_cache,
            if_missing,
        ) {
            Ok(store) => return Ok(store),
            Err(OpenError::FileInUse(_)) => {
                thread::sleep(delay.min(deadline.left()?));
                delay = (delay * 2).min(MAX_RETRY_DELAY);
            }
            Err(other) => return Err(StoreError::Unavailable(other)),
        }
    }
}

/// One call at a time through a store and its clones, first come first
/// served: each call takes a ticket and is woken when its number is up. (A
/// plain lock lets the thread that just finished take it again before the one
/// it woke, which can starve a caller past its busy timeout.)
#[derive(Debug, Default)]
struct Turn {
    queue: Mutex<Queue>,
    moved: Condvar,
}

#[derive(Debug, Default)]
struct Queue {
    next_ticket: u64,
    serving: u64,
    /// Tickets whose callers gave up waiting; skipped when their turn comes.
    abandoned: BTreeSet<u64>,
}

impl Queue {
    fn advance(&mut self) {
        self.serving += 1;
        while self.abandoned.remove(&self.serving) {
            self.serving += 1;
        }
    }
}

impl Turn {
    fn take(&self, deadline: &Deadline<'_>) -> Result<TurnGuard<'_>, StoreError> {
        let mut queue = self.lock();
        let ticket = queue.next_ticket;
        queue.next_ticket += 1;
        while queue.serving != ticket {
            let left = match deadline.left() {
                Ok(left) => left,
                Err(busy) => {
                    queue.abandoned.insert(ticket);
                    return Err(busy);
                }
            };
            queue = self
                .moved
                .wait_timeout(queue, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        Ok(TurnGuard(self))
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct TurnGuard<'a>(&'a Turn);

impl Drop for TurnGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().advance();
        self.0.moved.notify_all();
    }
}

/// A [`SequenceStore`] that holds its file only during a call.
///
/// The file must already exist (create it with [`open_waiting`]): a file that
/// was deleted or moved is reported as [`OpenError::Missing`], never replaced
/// by an empty one whose counters would start over. The path is made absolute
/// when the store is created, so a later change of working directory cannot
/// point it at another file.
///
/// Calls through one store (and its clones) take turns; the busy timeout
/// covers a call's whole wait, for its turn and for other processes.
#[derive(Debug, Clone)]
pub struct SharedFileStore {
    path: PathBuf,
    busy_timeout: Duration,
    turn: Arc<Turn>,
}

impl SharedFileStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, busy_timeout: Duration) -> Self {
        let path = path.into();
        Self {
            path: std::path::absolute(&path).unwrap_or(path),
            busy_timeout,
            turn: Arc::default(),
        }
    }

    /// The file, as an absolute path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Open the file, run `call`, close the file (when the store drops).
    fn with<T>(
        &self,
        call: impl FnOnce(&RedbStore) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let deadline = Deadline::new(&self.path, self.busy_timeout);
        let _turn = self.turn.take(&deadline)?;
        let store = open_until(&deadline, BlockCache::Off, IfMissing::Fail)?;
        call(&store)
    }
}

impl SequenceStore for SharedFileStore {
    fn create(&self, collection: Collection) -> Result<Collection, StoreError> {
        self.with(|store| store.create(collection))
    }

    fn get(&self, name: &str) -> Result<Collection, StoreError> {
        self.with(|store| store.get(name))
    }

    fn list(&self, request: &PageRequest) -> Result<Page, StoreError> {
        self.with(|store| store.list(request))
    }

    fn delete(&self, name: &str) -> Result<Collection, StoreError> {
        self.with(|store| store.delete(name))
    }

    fn update(&self, name: &str, change: Change<'_>) -> Result<Updated, StoreError> {
        self.with(|store| store.update(name, change))
    }

    fn advance(&self, request: &AdvanceRequest) -> Result<Issued, StoreError> {
        self.with(|store| store.advance(request))
    }

    fn purge_idempotency(&self, now: SystemTime) -> Result<usize, StoreError> {
        self.with(|store| store.purge_idempotency(now))
    }
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
