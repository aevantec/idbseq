//! Sequence storage behind the [`SequenceStore`] trait.
//!
//! Every backend must pass the shared conformance suite in `conformance`
//! (run with the `sequence_store_conformance!` macro).

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::{Duration, SystemTime};

use crate::collection::{Collection, IssuedRange};
use crate::validation::ValidationErrors;

#[cfg(test)]
#[macro_use]
pub(crate) mod conformance;

mod idempotency;
mod memory;
pub mod redb;

pub use memory::InMemoryStore;
pub use redb::{
    BlockCache, DEFAULT_BUSY_TIMEOUT, GroupCommitConfig, IfMissing, OpenError, RedbStore,
    SharedFileStore, decode_collection, open_waiting,
};

/// A read-modify-write step for [`SequenceStore::update`].
pub type Change<'a> = &'a dyn Fn(&Collection) -> Result<Collection, StoreError>;

/// Storage for collections and their counters.
///
/// Implementations must make each method atomic: a concurrent caller sees the
/// state before or after a call, never in between.
pub trait SequenceStore: Send + Sync {
    /// Store a new collection.
    ///
    /// # Errors
    ///
    /// [`StoreError::AlreadyExists`] if a collection with that name exists.
    fn create(&self, collection: Collection) -> Result<Collection, StoreError>;

    /// Read one collection.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`] if there is no collection with that name.
    fn get(&self, name: &str) -> Result<Collection, StoreError>;

    /// List collections sorted by name, one page at a time.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] if the backend cannot be read.
    fn list(&self, request: &PageRequest) -> Result<Page, StoreError>;

    /// Remove a collection and its counter. Returns the removed collection.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`] if there is no collection with that name.
    fn delete(&self, name: &str) -> Result<Collection, StoreError>;

    /// Atomically replace a collection with `change(current)`. The change must keep
    /// the name. Returns the collection before and after.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or whatever `change` returns; then nothing changes.
    fn update(&self, name: &str, change: Change<'_>) -> Result<Updated, StoreError>;

    /// Issue the next `count` IDs of a collection as one atomic, durable step:
    /// either all of them are issued or none are.
    ///
    /// With an idempotency key, a repeat of the same request (same key and
    /// fingerprint, not expired) returns the original range without issuing IDs.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], [`StoreError::Exhausted`] (the counter does not
    /// move) or [`StoreError::IdempotencyMismatch`].
    fn advance(&self, request: &AdvanceRequest) -> Result<Issued, StoreError>;

    /// Forget idempotency records that expired before `now`. Returns how many.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] if the backend cannot be written.
    fn purge_idempotency(&self, now: SystemTime) -> Result<usize, StoreError>;
}

/// A collection before and after [`SequenceStore::update`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updated {
    pub before: Collection,
    pub after: Collection,
}

/// One [`SequenceStore::advance`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceRequest {
    pub name: String,
    pub count: NonZeroU32,
    pub idempotency: Option<IdempotencyKey>,
}

impl AdvanceRequest {
    /// A request without an idempotency key.
    #[must_use]
    pub fn new(name: impl Into<String>, count: NonZeroU32) -> Self {
        Self {
            name: name.into(),
            count,
            idempotency: None,
        }
    }
}

/// Makes an advance replayable. `key` must already be scoped to the caller
/// (e.g. `"<api key id>:<header value>"`); `fingerprint` describes the request
/// (collection, count, response shape) so a reused key with a different request
/// is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub key: String,
    pub fingerprint: String,
    pub now: SystemTime,
    pub ttl: Duration,
}

/// The result of [`SequenceStore::advance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Issued {
    pub range: IssuedRange,
    /// `true` when this is a replay of an earlier request with the same key.
    pub replayed: bool,
}

/// One page of a [`SequenceStore::list`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRequest {
    /// Maximum number of items to return.
    pub limit: NonZeroUsize,
    /// Return only names after this one (the previous page's `next_cursor`).
    pub cursor: Option<String>,
    /// Return only names that start with this prefix.
    pub prefix: Option<String>,
}

/// A page of collections, sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<Collection>,
    /// Pass as `cursor` to get the next page; `None` on the last page.
    pub next_cursor: Option<String>,
    /// Number of collections matching `prefix`, across all pages.
    pub total: usize,
}

/// Why a [`SequenceStore`] call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("collection `{0}` not found")]
    NotFound(String),
    #[error("collection `{0}` already exists")]
    AlreadyExists(String),
    #[error("collection `{0}` is exhausted")]
    Exhausted(String),
    #[error("idempotency key was already used for a different request")]
    IdempotencyMismatch,
    #[error(transparent)]
    Invalid(#[from] ValidationErrors),
    #[error("storage backend failed: {0}")]
    Backend(String),
    /// Another process held the file for the whole busy timeout ([`SharedFileStore`]).
    #[error("{} stayed in use by another process for {waited:?}", path.display())]
    Busy {
        path: std::path::PathBuf,
        waited: std::time::Duration,
    },
    /// The file could not be opened ([`SharedFileStore`]).
    #[error(transparent)]
    Unavailable(redb::OpenError),
}

/// Select the names of a sorted map for one page. Shared by every backend.
fn page_of<'a>(
    names: impl Iterator<Item = &'a str> + Clone,
    request: &PageRequest,
) -> (Vec<&'a str>, Option<String>, usize) {
    let prefix = request.prefix.as_deref().unwrap_or_default();
    let matching = names
        .skip_while(move |n| *n < prefix)
        .take_while(move |n| n.starts_with(prefix));
    let total = matching.clone().count();
    let mut after_cursor =
        matching.skip_while(|n| request.cursor.as_deref().is_some_and(|c| *n <= c));
    let items: Vec<&str> = after_cursor.by_ref().take(request.limit.get()).collect();
    let next_cursor = after_cursor
        .next()
        .and(items.last())
        .map(|last| (*last).to_owned());
    (items, next_cursor, total)
}
