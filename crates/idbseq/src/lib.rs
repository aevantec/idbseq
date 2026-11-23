//! Embedded ID sequences in one portable file, like SQLite for counters.
//!
//! A [`Database`] is one file holding named collections. Each collection hands
//! out increasing IDs and never the same one twice, even across crashes and
//! restarts. It needs no server and no network; the file can be copied to
//! another machine or served later by `idbseqd`.
//!
//! ```
//! use idbseq::{CollectionSpec, Database};
//!
//! # fn main() -> Result<(), idbseq::Error> {
//! # let dir = tempfile::tempdir().expect("temp dir");
//! # let path = dir.path().join("app.idbseq");
//! let ids = Database::open(&path)?;
//! ids.create_collection(CollectionSpec::named("invoices"))?;
//! assert_eq!(ids.next_id("invoices")?, 1);
//! assert_eq!(ids.next_id("invoices")?, 2);
//! # Ok(())
//! # }
//! ```
//!
//! By default the handle holds the file ([`Mode::Exclusive`]). To let several
//! processes take turns on one file, open it with [`Options::shared`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use idbseq_core::engine::{Engine, Limits};
use idbseq_core::store::{BlockCache, SequenceStore, SharedFileStore, StoreError, open_waiting};
use idbseq_core::validation::idempotency_key_problem;

mod error;
pub mod json;
mod open;

pub use error::Error;
pub use idbseq_core::collection::{
    Collection, CollectionPatch, CollectionSpec, Durability, Id, IssuedRange, ResetOutcome,
};
pub use idbseq_core::engine::{DEFAULT_MAX_BATCH, LastId, NextRequest};
pub use idbseq_core::store::{Issued, OpenError, Page};
pub use idbseq_core::validation::ValidationErrors;
pub use open::{DEFAULT_BUSY_TIMEOUT, Mode, Options};

/// Prefix of idempotency keys used through an embedded [`Database`].
const EMBEDDED_KEY_SCOPE: &str = "embedded:";

/// An open idbseq file. Cheap to share between threads (`&Database` is enough).
#[derive(Debug)]
pub struct Database {
    path: PathBuf,
    options: Options,
    /// On a store that holds the file ([`Mode::Exclusive`]) or opens it for
    /// each call ([`Mode::Shared`]).
    engine: Engine,
}

impl Database {
    /// Open or create the file at `path`, holding it until the handle is dropped.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if another process holds the file for
    /// [`DEFAULT_BUSY_TIMEOUT`], [`Error::Open`] if it cannot be opened or created.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(path, Options::default())
    }

    /// Open or create the file at `path` with [`Options`]. Its folder must exist.
    ///
    /// # Errors
    ///
    /// As for [`Database::open`].
    pub fn open_with(path: impl AsRef<Path>, options: Options) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let store: Arc<dyn SequenceStore> = match options.mode {
            Mode::Exclusive => Arc::new(open_waiting(&path, BlockCache::On, options.busy_timeout)?),
            Mode::Shared => {
                // Create the file (or check it) now: later calls never create
                // one, so a file deleted or moved away is reported, not
                // silently replaced by an empty one whose counters start over.
                drop(open_waiting(&path, BlockCache::Off, options.busy_timeout)?);
                Arc::new(SharedFileStore::new(&path, options.busy_timeout))
            }
        };
        let engine = Engine::new(store, Limits::default());
        // Forget retry keys older than a day. In shared mode this also checks
        // that the file can be opened.
        engine.purge_idempotency(SystemTime::now())?;
        Ok(Self {
            path,
            options,
            engine,
        })
    }

    /// The file this handle uses.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The options it was opened with.
    #[must_use]
    pub fn options(&self) -> Options {
        self.options
    }

    /// Create a collection.
    ///
    /// # Errors
    ///
    /// `AlreadyExists`, `Invalid` (name, `start`, `step`, `max`), or as for opening.
    pub fn create_collection(&self, spec: CollectionSpec) -> Result<Collection, Error> {
        self.with_engine(|engine| engine.create(spec, SystemTime::now()))
    }

    /// One collection, with its last and next ID.
    ///
    /// # Errors
    ///
    /// `NotFound`, or as for opening.
    pub fn collection(&self, name: &str) -> Result<Collection, Error> {
        self.with_engine(|engine| engine.get(name))
    }

    /// Collections sorted by name, a page at a time. `limit` defaults to 50
    /// and may be 1–500; pass the page's `next_cursor` as `cursor` for the next.
    ///
    /// # Errors
    ///
    /// `Invalid` for a bad `limit`, or as for opening.
    pub fn collections(
        &self,
        limit: Option<usize>,
        cursor: Option<String>,
        prefix: Option<String>,
    ) -> Result<Page, Error> {
        self.with_engine(|engine| engine.list(limit, cursor, prefix))
    }

    /// Change `max`, `durability`, `protected`, or (before the first ID) `start`
    /// and `step`.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Invalid`, or as for opening.
    pub fn update_collection(
        &self,
        name: &str,
        patch: &CollectionPatch,
    ) -> Result<Collection, Error> {
        self.with_engine(|engine| engine.update(name, patch).map(|updated| updated.after))
    }

    /// Move the counter so the next ID is `value` (default: the collection's
    /// `start`). Moving it backwards needs `allow_rewind`, and can repeat IDs.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Invalid`, or as for opening.
    pub fn reset_collection(
        &self,
        name: &str,
        value: Option<Id>,
        allow_rewind: bool,
    ) -> Result<ResetOutcome, Error> {
        self.with_engine(|engine| engine.reset(name, value, allow_rewind))
    }

    /// Delete a collection and its counter.
    ///
    /// # Errors
    ///
    /// `NotFound`, or as for opening.
    pub fn delete_collection(&self, name: &str) -> Result<Collection, Error> {
        self.with_engine(|engine| engine.delete(name))
    }

    /// The last issued ID and the next one, without issuing anything.
    ///
    /// # Errors
    ///
    /// `NotFound`, or as for opening.
    pub fn last_id(&self, name: &str) -> Result<LastId, Error> {
        self.with_engine(|engine| engine.last(name))
    }

    /// Issue the next ID. It is on disk before it is returned.
    ///
    /// # Errors
    ///
    /// `NotFound`, `Exhausted`, or as for opening.
    pub fn next_id(&self, name: &str) -> Result<Id, Error> {
        self.issue(&NextRequest {
            name: name.to_owned(),
            count: None,
            idempotency_key: None,
        })
        .map(|issued| issued.range.first)
    }

    /// Issue `count` IDs at once (1 to [`DEFAULT_MAX_BATCH`]), all or none.
    ///
    /// # Errors
    ///
    /// `Invalid` for a bad count, `NotFound`, `Exhausted`, or as for opening.
    pub fn next_ids(&self, name: &str, count: u32) -> Result<IssuedRange, Error> {
        self.issue(&NextRequest {
            name: name.to_owned(),
            count: Some(count),
            idempotency_key: None,
        })
        .map(|issued| issued.range)
    }

    /// Issue IDs with an optional idempotency key: repeating a request with the
    /// same key within a day returns the same IDs (`replayed` is then `true`).
    ///
    /// # Errors
    ///
    /// `IdempotencyMismatch` if the key was used for a different request, or as
    /// for [`Database::next_ids`].
    pub fn issue(&self, request: &NextRequest) -> Result<Issued, Error> {
        let Some(key) = &request.idempotency_key else {
            return self.with_engine(|engine| engine.next(request, SystemTime::now()));
        };
        if let Some(problem) = idempotency_key_problem(key) {
            return Err(
                StoreError::Invalid(ValidationErrors::field("idempotency_key", problem)).into(),
            );
        }
        // Scoped like the server scopes its keys (`admin:`, `key:<id>:`), so a
        // file later served by idbseqd never mixes the two.
        let scoped = NextRequest {
            idempotency_key: Some(format!("{EMBEDDED_KEY_SCOPE}{key}")),
            ..request.clone()
        };
        self.with_engine(|engine| engine.next(&scoped, SystemTime::now()))
    }

    fn with_engine<T>(
        &self,
        call: impl FnOnce(&Engine) -> Result<T, StoreError>,
    ) -> Result<T, Error> {
        Ok(call(&self.engine)?)
    }
}
