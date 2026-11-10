//! The sequence engine: the operations the API exposes, with input limits,
//! over any [`SequenceStore`]. Authorization is not checked here (US-0024).

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::collection::{Collection, CollectionPatch, CollectionSpec, Id, ResetOutcome};
use crate::store::{
    AdvanceRequest, IdempotencyKey, Issued, Page, PageRequest, SequenceStore, StoreError, Updated,
};
use crate::validation::ValidationErrors;

/// Default and largest page size for listing collections.
pub const DEFAULT_PAGE_LIMIT: usize = 50;
pub const MAX_PAGE_LIMIT: usize = 500;

/// Default largest batch for one `next` call.
pub const DEFAULT_MAX_BATCH: u32 = 10_000;

/// Default lifetime of an idempotency key.
pub const DEFAULT_IDEMPOTENCY_TTL: Duration = Duration::from_hours(24);

/// Engine limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_batch: u32,
    pub idempotency_ttl: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_batch: DEFAULT_MAX_BATCH,
            idempotency_ttl: DEFAULT_IDEMPOTENCY_TTL,
        }
    }
}

/// `last_id` and a preview of the next ID, without reserving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastId {
    pub last_id: Option<Id>,
    pub next_id: Option<Id>,
}

/// A `next` call. `count: None` means a single ID (and the single-ID response shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextRequest {
    pub name: String,
    pub count: Option<u32>,
    /// Idempotency key already scoped to the caller, e.g. `"<key id>:<header>"`.
    pub idempotency_key: Option<String>,
}

/// The engine. Cheap to clone; share one per process.
#[derive(Clone)]
pub struct Engine {
    store: Arc<dyn SequenceStore>,
    limits: Limits,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl Engine {
    #[must_use]
    pub fn new(store: Arc<dyn SequenceStore>, limits: Limits) -> Self {
        Self { store, limits }
    }

    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Create a collection.
    ///
    /// # Errors
    ///
    /// `Invalid` with every failing field, or `AlreadyExists`.
    pub fn create(&self, spec: CollectionSpec, now: SystemTime) -> Result<Collection, StoreError> {
        self.store.create(Collection::create(spec, now)?)
    }

    /// Read one collection.
    ///
    /// # Errors
    ///
    /// `NotFound`.
    pub fn get(&self, name: &str) -> Result<Collection, StoreError> {
        self.store.get(name)
    }

    /// List collections sorted by name. `limit` defaults to 50 and must be 1–500.
    ///
    /// # Errors
    ///
    /// `Invalid` for a bad `limit`.
    pub fn list(
        &self,
        limit: Option<usize>,
        cursor: Option<String>,
        prefix: Option<String>,
    ) -> Result<Page, StoreError> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        let limit = NonZeroUsize::new(limit)
            .filter(|l| l.get() <= MAX_PAGE_LIMIT)
            .ok_or_else(|| ValidationErrors::field("limit", "must be between 1 and 500"))?;
        self.store.list(&PageRequest {
            limit,
            cursor,
            prefix,
        })
    }

    /// Change `max`, `protected`, `durability`, or (before the first ID) `start`/`step`.
    ///
    /// # Errors
    ///
    /// `NotFound` or `Invalid`.
    pub fn update(&self, name: &str, patch: &CollectionPatch) -> Result<Updated, StoreError> {
        self.store
            .update(name, &|current| Ok(current.apply(patch)?))
    }

    /// Delete a collection and its counter.
    ///
    /// # Errors
    ///
    /// `NotFound`.
    pub fn delete(&self, name: &str) -> Result<Collection, StoreError> {
        self.store.delete(name)
    }

    /// Move the counter so the next ID is `value` (default `start`).
    ///
    /// # Errors
    ///
    /// `NotFound`, or `Invalid` (out of range, off the step grid, rewind without `allow_rewind`).
    pub fn reset(
        &self,
        name: &str,
        value: Option<Id>,
        allow_rewind: bool,
    ) -> Result<ResetOutcome, StoreError> {
        let updated = self
            .store
            .update(name, &|current| Ok(current.reset(value, allow_rewind)?.0))?;
        Ok(ResetOutcome {
            previous_last_id: updated.before.last_id(),
            last_id: updated.after.last_id(),
            next_id: updated.after.next_id(),
        })
    }

    /// `last_id` and the next ID preview. Never changes the counter.
    ///
    /// # Errors
    ///
    /// `NotFound`.
    pub fn last(&self, name: &str) -> Result<LastId, StoreError> {
        let collection = self.store.get(name)?;
        Ok(LastId {
            last_id: collection.last_id(),
            next_id: collection.next_id(),
        })
    }

    /// Issue one ID or a batch, durably.
    ///
    /// # Errors
    ///
    /// `Invalid` for a count outside `1..=max_batch`, `NotFound`, `Exhausted`
    /// (nothing issued) or `IdempotencyMismatch`.
    pub fn next(&self, request: &NextRequest, now: SystemTime) -> Result<Issued, StoreError> {
        let count = self.batch_count(request.count)?;
        let idempotency = request.idempotency_key.as_ref().map(|key| IdempotencyKey {
            key: key.clone(),
            fingerprint: fingerprint(request),
            now,
            ttl: self.limits.idempotency_ttl,
        });
        self.store.advance(&AdvanceRequest {
            name: request.name.clone(),
            count,
            idempotency,
        })
    }

    /// Forget expired idempotency keys.
    ///
    /// # Errors
    ///
    /// `Backend`.
    pub fn purge_idempotency(&self, now: SystemTime) -> Result<usize, StoreError> {
        self.store.purge_idempotency(now)
    }

    fn batch_count(&self, count: Option<u32>) -> Result<NonZeroU32, StoreError> {
        let max = self.limits.max_batch;
        NonZeroU32::new(count.unwrap_or(1))
            .filter(|c| c.get() <= max)
            .ok_or_else(|| {
                ValidationErrors::field("count", format!("must be between 1 and {max}")).into()
            })
    }
}

/// What makes two `next` requests "the same" for idempotency: collection, count
/// and response shape (`count` absent vs present).
fn fingerprint(request: &NextRequest) -> String {
    match request.count {
        None => format!("{}|single", request.name),
        Some(count) => format!("{}|batch|{count}", request.name),
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
