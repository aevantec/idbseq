//! Durable [`SequenceStore`] on an embedded [redb](https://docs.rs/redb) file.
//!
//! - Strict collections: concurrent `advance` calls are group-committed — they
//!   share one write transaction and one fsync (see [`committer`]).
//! - Block collections: IDs come from an in-memory reservation, and the disk is
//!   written once per block (see [`block`]).
//! - Reads use MVCC read transactions and never wait for writers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ::redb::{Database, DatabaseError, ReadableDatabase, ReadableTable, TableDefinition};

use super::{
    AdvanceRequest, Change, Issued, Page, PageRequest, SequenceStore, StoreError, Updated, page_of,
};
use crate::collection::Collection;

mod block;
mod codec;
mod committer;

use block::Blocks;
use codec::{backend, decode, encode};
use committer::{Committer, Outcome};

/// File name of the database inside the data directory.
pub const DATABASE_FILE: &str = "idbseq.redb";

/// Version of the on-disk layout. Bump it and add a migration when tables change.
pub const SCHEMA_VERSION: u64 = 1;

/// Store metadata (schema version).
pub const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
/// Collections as JSON, keyed by name.
pub const COLLECTIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("collections");
/// Idempotency records as JSON, keyed by scoped key.
pub const IDEMPOTENCY: TableDefinition<&str, &[u8]> = TableDefinition::new("idempotency");
/// Key of the schema version in [`META`].
pub const SCHEMA_KEY: &str = "schema_version";

/// Group-commit tuning for strict collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupCommitConfig {
    /// Most requests in one transaction.
    pub max_batch: usize,
    /// How long the committer waits for more requests after the first. Zero means
    /// it only takes requests that are already queued, so a lone caller never waits.
    pub max_wait: Duration,
}

impl Default for GroupCommitConfig {
    fn default() -> Self {
        Self {
            max_batch: 1024,
            max_wait: Duration::ZERO,
        }
    }
}

/// Why a [`RedbStore`] could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("data directory {0} is already in use by another idbseqd process")]
    AlreadyOpen(PathBuf),
    #[error("data file schema version {found} is not supported (expected {expected})")]
    SchemaVersion { found: u64, expected: u64 },
    #[error("cannot open data directory {path}: {reason}")]
    Io { path: PathBuf, reason: String },
}

/// A [`SequenceStore`] backed by one redb file in a data directory.
pub struct RedbStore {
    db: Arc<Database>,
    committer: Committer,
    blocks: Blocks,
}

impl std::fmt::Debug for RedbStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedbStore").finish_non_exhaustive()
    }
}

impl RedbStore {
    /// Open (or create) the store in `data_dir`. Only one process may hold it.
    ///
    /// # Errors
    ///
    /// [`OpenError::AlreadyOpen`] if another process holds the directory,
    /// [`OpenError::SchemaVersion`] for an unknown file format, or [`OpenError::Io`].
    pub fn open(data_dir: &Path, config: GroupCommitConfig) -> Result<Self, OpenError> {
        let io = |reason: String| OpenError::Io {
            path: data_dir.to_path_buf(),
            reason,
        };
        std::fs::create_dir_all(data_dir).map_err(|e| io(e.to_string()))?;
        let db = Database::create(data_dir.join(DATABASE_FILE)).map_err(|e| match e {
            DatabaseError::DatabaseAlreadyOpen => OpenError::AlreadyOpen(data_dir.to_path_buf()),
            other => io(other.to_string()),
        })?;
        init_schema(&db).map_err(|e| match e {
            InitError::Version(found) => OpenError::SchemaVersion {
                found,
                expected: SCHEMA_VERSION,
            },
            InitError::Backend(reason) => io(reason),
        })?;
        let db = Arc::new(db);
        Ok(Self {
            committer: Committer::start(Arc::clone(&db), config),
            blocks: Blocks::default(),
            db,
        })
    }

    /// The underlying database, for other tables kept in the same file (keys,
    /// sessions, audit log). Never write to the tables of this module.
    #[must_use]
    pub fn database(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }

    /// The collection as stored on disk (for block collections: the reservation mark).
    fn persisted(&self, name: &str) -> Result<Collection, StoreError> {
        let txn = self.db.begin_read().map_err(backend)?;
        let table = txn.open_table(COLLECTIONS).map_err(backend)?;
        let bytes = table.get(name).map_err(backend)?;
        bytes
            .map(|guard| decode(guard.value()))
            .transpose()?
            .ok_or_else(|| StoreError::NotFound(name.to_owned()))
    }

    /// Group transactions committed for strict collections so far.
    #[must_use]
    pub fn group_commits(&self) -> u64 {
        self.committer.commits()
    }

    #[cfg(test)]
    fn fail_next_commit(&self) {
        self.committer.fail_next_commit();
    }
}

enum InitError {
    Version(u64),
    Backend(String),
}

fn init_schema(db: &Database) -> Result<(), InitError> {
    let err = |e: &dyn std::fmt::Display| InitError::Backend(e.to_string());
    let txn = db.begin_write().map_err(|e| err(&e))?;
    {
        let mut meta = txn.open_table(META).map_err(|e| err(&e))?;
        let found = meta
            .get(SCHEMA_KEY)
            .map_err(|e| err(&e))?
            .map(|v| v.value());
        match found {
            Some(SCHEMA_VERSION) => {}
            Some(other) => return Err(InitError::Version(other)),
            None => {
                meta.insert(SCHEMA_KEY, SCHEMA_VERSION)
                    .map_err(|e| err(&e))?;
            }
        }
        txn.open_table(COLLECTIONS).map_err(|e| err(&e))?;
        txn.open_table(IDEMPOTENCY).map_err(|e| err(&e))?;
    }
    txn.commit().map_err(|e| err(&e))
}

impl SequenceStore for RedbStore {
    fn create(&self, collection: Collection) -> Result<Collection, StoreError> {
        let txn = self.db.begin_write().map_err(backend)?;
        {
            let mut table = txn.open_table(COLLECTIONS).map_err(backend)?;
            if table.get(collection.name()).map_err(backend)?.is_some() {
                return Err(StoreError::AlreadyExists(collection.name().to_owned()));
            }
            table
                .insert(collection.name(), encode(&collection)?.as_slice())
                .map_err(backend)?;
        }
        txn.commit().map_err(backend)?;
        Ok(collection)
    }

    fn get(&self, name: &str) -> Result<Collection, StoreError> {
        Ok(self.blocks.overlay(self.persisted(name)?))
    }

    fn list(&self, request: &PageRequest) -> Result<Page, StoreError> {
        let txn = self.db.begin_read().map_err(backend)?;
        let table = txn.open_table(COLLECTIONS).map_err(backend)?;
        let names: Vec<String> = table
            .iter()
            .map_err(backend)?
            .map(|entry| {
                entry
                    .map(|(key, _)| key.value().to_owned())
                    .map_err(backend)
            })
            .collect::<Result<_, _>>()?;
        let (selected, next_cursor, total) = page_of(names.iter().map(String::as_str), request);
        let items = selected
            .into_iter()
            .map(|name| {
                let bytes = table.get(name).map_err(backend)?;
                let guard = bytes.ok_or_else(|| StoreError::NotFound(name.to_owned()))?;
                decode(guard.value()).map(|c| self.blocks.overlay(c))
            })
            .collect::<Result<_, _>>()?;
        Ok(Page {
            items,
            next_cursor,
            total,
        })
    }

    fn delete(&self, name: &str) -> Result<Collection, StoreError> {
        self.blocks.with_lock(|cache| {
            let before = Blocks::overlay_with(cache, self.persisted(name)?);
            let txn = self.db.begin_write().map_err(backend)?;
            txn.open_table(COLLECTIONS)
                .map_err(backend)?
                .remove(name)
                .map_err(backend)?;
            txn.commit().map_err(backend)?;
            cache.remove(name);
            Ok(before)
        })
    }

    fn update(&self, name: &str, change: Change<'_>) -> Result<Updated, StoreError> {
        self.blocks.with_lock(|cache| {
            let before = Blocks::overlay_with(cache, self.persisted(name)?);
            let after = change(&before)?;
            let txn = self.db.begin_write().map_err(backend)?;
            txn.open_table(COLLECTIONS)
                .map_err(backend)?
                .insert(name, encode(&after)?.as_slice())
                .map_err(backend)?;
            txn.commit().map_err(backend)?;
            // The new on-disk value is exactly what was issued; drop any reservation.
            cache.remove(name);
            Ok(Updated { before, after })
        })
    }

    fn advance(&self, request: &AdvanceRequest) -> Result<Issued, StoreError> {
        match self.committer.submit(request.clone())? {
            Outcome::Issued(issued) => Ok(issued),
            Outcome::BlockMode => self.blocks.advance(&self.db, request),
        }
    }

    fn purge_idempotency(&self, now: SystemTime) -> Result<usize, StoreError> {
        let txn = self.db.begin_write().map_err(backend)?;
        let purged = {
            let mut table = txn.open_table(IDEMPOTENCY).map_err(backend)?;
            let expired: Vec<String> = table
                .iter()
                .map_err(backend)?
                .filter_map(|entry| {
                    let (key, value) = entry.ok()?;
                    let record: super::idempotency::Record = decode(value.value()).ok()?;
                    record.is_expired(now).then(|| key.value().to_owned())
                })
                .collect();
            for key in &expired {
                table.remove(key.as_str()).map_err(backend)?;
            }
            expired.len()
        };
        txn.commit().map_err(backend)?;
        Ok(purged)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
