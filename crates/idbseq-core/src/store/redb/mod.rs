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

use ::redb::{
    Database, DatabaseError, ReadableDatabase, ReadableTable, StorageError, TableDefinition,
};

use super::{
    AdvanceRequest, Change, Issued, Page, PageRequest, SequenceStore, StoreError, Updated, page_of,
};
use crate::collection::Collection;

mod block;
mod codec;
mod committer;
mod shared;

use block::Blocks;
use codec::{backend, decode, encode};
use committer::{Committer, Outcome};
pub use shared::{DEFAULT_BUSY_TIMEOUT, SharedFileStore, open_waiting};

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

/// Whether block collections hand out reserved IDs from memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockCache {
    /// Reserve a block on disk and issue its IDs from memory. For a process that
    /// keeps the file open.
    #[default]
    On,
    /// Write every ID to disk, as for a strict collection. For a file opened
    /// briefly by several processes in turn: a cached block would be lost at
    /// every close.
    Off,
}

/// What [`RedbStore::open_file`] does when the file does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfMissing {
    /// Create it, readable by its owner only.
    Create,
    /// Fail with [`OpenError::Missing`]. For every open after the first, so a
    /// file deleted or moved away is never silently replaced by an empty one
    /// (whose counters would start over).
    Fail,
}

/// Why a [`RedbStore`] could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("data directory {0} is already in use by another idbseqd process")]
    AlreadyOpen(PathBuf),
    #[error("{0} is open elsewhere")]
    FileInUse(PathBuf),
    #[error("{0} does not exist (was it deleted or moved?)")]
    Missing(PathBuf),
    #[error("{0} is not an idbseq file")]
    NotIdbseq(PathBuf),
    #[error("data file schema version {found} is not supported (expected {expected})")]
    SchemaVersion { found: u64, expected: u64 },
    #[error("cannot open {path}: {reason}")]
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
        let io = |e: std::io::Error| OpenError::Io {
            path: data_dir.to_path_buf(),
            reason: e.to_string(),
        };
        std::fs::create_dir_all(data_dir).map_err(io)?;
        make_private(data_dir).map_err(io)?;
        Self::open_file(
            &data_dir.join(DATABASE_FILE),
            config,
            BlockCache::On,
            IfMissing::Create,
        )
        .map_err(|e| match e {
            OpenError::FileInUse(_) => OpenError::AlreadyOpen(data_dir.to_path_buf()),
            other => other,
        })
    }

    /// Open the store in one file, whose folder must exist, creating it if
    /// `if_missing` says so. Only one process may hold the file; it is portable
    /// between machines.
    ///
    /// # Errors
    ///
    /// [`OpenError::FileInUse`] if another process (or another store in this
    /// process) holds the file, [`OpenError::Missing`], [`OpenError::NotIdbseq`]
    /// for another program's redb file, [`OpenError::SchemaVersion`] for an
    /// unknown file format, or [`OpenError::Io`].
    pub fn open_file(
        file: &Path,
        config: GroupCommitConfig,
        block_cache: BlockCache,
        if_missing: IfMissing,
    ) -> Result<Self, OpenError> {
        let io = |reason: String| OpenError::Io {
            path: file.to_path_buf(),
            reason,
        };
        let opened = match if_missing {
            IfMissing::Create => {
                create_private(file).map_err(|e| io(e.to_string()))?;
                Database::create(file)
            }
            IfMissing::Fail => Database::open(file),
        };
        let db = opened.map_err(|e| match e {
            DatabaseError::DatabaseAlreadyOpen => OpenError::FileInUse(file.to_path_buf()),
            DatabaseError::Storage(StorageError::Io(ref error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                OpenError::Missing(file.to_path_buf())
            }
            other => io(other.to_string()),
        })?;
        init_schema(&db).map_err(|e| match e {
            InitError::Foreign => OpenError::NotIdbseq(file.to_path_buf()),
            InitError::Version(found) => OpenError::SchemaVersion {
                found,
                expected: SCHEMA_VERSION,
            },
            InitError::Backend(reason) => io(reason),
        })?;
        let db = Arc::new(db);
        Ok(Self {
            committer: Committer::start(Arc::clone(&db), config, block_cache == BlockCache::On),
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
    /// Only for reads: to change a collection, read it with [`stored_in`] inside
    /// the write transaction.
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
    /// No idbseq schema, but tables of some other program.
    Foreign,
    Version(u64),
    Backend(String),
}

fn init_schema(db: &Database) -> Result<(), InitError> {
    let err = |e: &dyn std::fmt::Display| InitError::Backend(e.to_string());
    let txn = db.begin_write().map_err(|e| err(&e))?;
    // A file is set up only while it has no tables at all: new or empty.
    let is_blank = txn.list_tables().map_err(|e| err(&e))?.next().is_none();
    {
        let mut meta = txn.open_table(META).map_err(|e| err(&e))?;
        let found = meta
            .get(SCHEMA_KEY)
            .map_err(|e| err(&e))?
            .map(|v| v.value());
        match found {
            Some(SCHEMA_VERSION) => {}
            Some(other) => return Err(InitError::Version(other)),
            None if !is_blank => return Err(InitError::Foreign),
            None => {
                meta.insert(SCHEMA_KEY, SCHEMA_VERSION)
                    .map_err(|e| err(&e))?;
            }
        }
        if found.is_some() {
            // Already set up: the tables exist. Skip the commit and its disk
            // sync, which every open in shared mode would otherwise pay.
            drop(meta);
            return txn.abort().map_err(|e| err(&e));
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

    // `delete` and `update` read the collection inside their own write
    // transaction. redb runs one writer at a time, so the committer cannot issue
    // IDs between the read and the write (which would otherwise be overwritten
    // with the stale counter and issued again).

    fn delete(&self, name: &str) -> Result<Collection, StoreError> {
        self.blocks.with_lock(|cache| {
            let txn = self.db.begin_write().map_err(backend)?;
            let before = {
                let mut table = txn.open_table(COLLECTIONS).map_err(backend)?;
                let before = Blocks::overlay_with(cache, stored_in(&table, name)?);
                table.remove(name).map_err(backend)?;
                before
            };
            txn.commit().map_err(backend)?;
            cache.remove(name);
            Ok(before)
        })
    }

    fn update(&self, name: &str, change: Change<'_>) -> Result<Updated, StoreError> {
        self.blocks.with_lock(|cache| {
            let txn = self.db.begin_write().map_err(backend)?;
            let updated = {
                let mut table = txn.open_table(COLLECTIONS).map_err(backend)?;
                let before = Blocks::overlay_with(cache, stored_in(&table, name)?);
                let after = change(&before)?;
                table
                    .insert(name, encode(&after)?.as_slice())
                    .map_err(backend)?;
                Updated { before, after }
            };
            txn.commit().map_err(backend)?;
            // The new on-disk value is exactly what was issued; drop any reservation.
            cache.remove(name);
            Ok(updated)
        })
    }

    fn advance(&self, request: &AdvanceRequest) -> Result<Issued, StoreError> {
        if let Outcome::Issued(issued) = self.committer.submit(request.clone())? {
            return Ok(issued);
        }
        self.blocks.with_lock(|cache| {
            if let Some(issued) = Blocks::advance(cache, &self.db, request)? {
                return Ok(issued);
            }
            // Switched to strict since the committer looked. Durability changes
            // take this lock, so it stays strict while the committer issues it.
            match self.committer.submit(request.clone())? {
                Outcome::Issued(issued) => Ok(issued),
                Outcome::BlockMode => Err(StoreError::Backend(
                    "collection changed durability while it was locked".to_owned(),
                )),
            }
        })
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

/// Make `path` readable only by its owner: `0700` for a directory, `0600` for a
/// file. The data holds the admin password hash and every API key's hash.
///
/// A path owned by someone else (for example a volume prepared by an operator)
/// is left as it is: that is the operator's choice, and the process can still
/// use it.
///
/// # Errors
///
/// Any failure other than not owning the path.
pub fn make_private(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        match std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
            other => other,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Create `file` readable by its owner only (mode 0600 on Unix), unless it
/// exists. Created with that mode, so it is never readable by others, not even
/// briefly; an existing file keeps the permissions its owner chose.
fn create_private(file: &Path) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(file) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// A collection record as stored (checksum checked), for tools that read the
/// file directly, such as restore.
///
/// # Errors
///
/// The record is damaged or not a collection.
pub fn decode_collection(bytes: &[u8]) -> Result<Collection, StoreError> {
    decode(bytes)
}

/// The collection in `table`, read inside the caller's write transaction.
fn stored_in(table: &::redb::Table<'_, &str, &[u8]>, name: &str) -> Result<Collection, StoreError> {
    table
        .get(name)
        .map_err(backend)?
        .map(|guard| decode(guard.value()))
        .transpose()?
        .ok_or_else(|| StoreError::NotFound(name.to_owned()))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
