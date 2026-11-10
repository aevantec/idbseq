//! Consistent snapshots and restore (US-0033).
//!
//! A backup is a new redb file holding a copy of every table, taken inside one
//! read transaction, so it is consistent even while the server keeps issuing IDs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use idbseq_core::collection::{Collection, Id};
use idbseq_core::store::redb::{
    COLLECTIONS, DATABASE_FILE, IDEMPOTENCY, META, SCHEMA_KEY, SCHEMA_VERSION,
};
use redb::{
    Database, DatabaseError, ReadTransaction, ReadableDatabase, ReadableTable, TableDefinition,
    WriteTransaction,
};

use crate::db::{ADMIN, API_KEYS, AUDIT, SESSIONS};

const STR_TABLES: [TableDefinition<&str, &[u8]>; 5] =
    [COLLECTIONS, IDEMPOTENCY, ADMIN, SESSIONS, API_KEYS];

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{0} already exists; choose a new backup path")]
    Exists(PathBuf),
    #[error("{0} is not an idbseq backup: {1}")]
    NotABackup(PathBuf, String),
    #[error("the data directory is in use; stop idbseqd before restoring")]
    InUse,
    #[error("backup is behind the current data for: {}. Restoring would reissue IDs; pass --force and then skip ahead with `reset --value` (see docs/operations.md)", .0.join(", "))]
    Behind(Vec<String>),
    #[error("backup failed: {0}")]
    Storage(String),
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(storage)`
fn storage(error: impl std::fmt::Display) -> BackupError {
    BackupError::Storage(error.to_string())
}

/// Write a consistent copy of `db` to a new file at `dest`.
///
/// # Errors
///
/// `dest` exists, or a storage failure.
pub fn snapshot(db: &Database, dest: &Path) -> Result<(), BackupError> {
    if dest.exists() {
        return Err(BackupError::Exists(dest.to_path_buf()));
    }
    let read = db.begin_read().map_err(storage)?;
    let target = Database::create(dest).map_err(storage)?;
    let write = target.begin_write().map_err(storage)?;
    copy_tables(&read, &write)?;
    write.commit().map_err(storage)
}

fn copy_tables(read: &ReadTransaction, write: &WriteTransaction) -> Result<(), BackupError> {
    {
        let source = read.open_table(META).map_err(storage)?;
        let mut target = write.open_table(META).map_err(storage)?;
        for row in source.iter().map_err(storage)? {
            let (k, v) = row.map_err(storage)?;
            target.insert(k.value(), v.value()).map_err(storage)?;
        }
    }
    for definition in STR_TABLES {
        let Ok(source) = read.open_table(definition) else {
            continue; // table never created in this database
        };
        let mut target = write.open_table(definition).map_err(storage)?;
        for row in source.iter().map_err(storage)? {
            let (k, v) = row.map_err(storage)?;
            target.insert(k.value(), v.value()).map_err(storage)?;
        }
    }
    if let Ok(source) = read.open_table(AUDIT) {
        let mut target = write.open_table(AUDIT).map_err(storage)?;
        for row in source.iter().map_err(storage)? {
            let (k, v) = row.map_err(storage)?;
            target.insert(k.value(), v.value()).map_err(storage)?;
        }
    }
    Ok(())
}

/// `last_id` per collection name.
fn counters(db: &Database) -> Result<BTreeMap<String, Option<Id>>, BackupError> {
    let read = db.begin_read().map_err(storage)?;
    let Ok(table) = read.open_table(COLLECTIONS) else {
        return Ok(BTreeMap::new());
    };
    table
        .iter()
        .map_err(storage)?
        .map(|row| {
            let (k, v) = row.map_err(storage)?;
            let collection: Collection = serde_json::from_slice(v.value()).map_err(storage)?;
            Ok((k.value().to_owned(), collection.last_id()))
        })
        .collect()
}

/// Collections whose counter in `backup` is behind `current` (or missing from it).
fn behind(
    current: &BTreeMap<String, Option<Id>>,
    backup: &BTreeMap<String, Option<Id>>,
) -> Vec<String> {
    current
        .iter()
        .filter(|(name, last)| match backup.get(*name) {
            None => last.is_some(),
            Some(backed_up) => backed_up < *last,
        })
        .map(|(name, _)| name.clone())
        .collect()
}

fn check_backup(path: &Path) -> Result<Database, BackupError> {
    let not_backup = |reason: String| BackupError::NotABackup(path.to_path_buf(), reason);
    let db = Database::open(path).map_err(|e| not_backup(e.to_string()))?;
    let version = {
        let read = db.begin_read().map_err(storage)?;
        let meta = read
            .open_table(META)
            .map_err(|e| not_backup(e.to_string()))?;
        let value = meta.get(SCHEMA_KEY).map_err(storage)?;
        value.map(|v| v.value())
    };
    match version {
        Some(SCHEMA_VERSION) => Ok(db),
        other => Err(not_backup(format!(
            "schema version {other:?}, expected {SCHEMA_VERSION}"
        ))),
    }
}

/// Replace the data in `data_dir` with `backup`. The server must be stopped.
/// Refuses if any counter would move backwards, unless `force`. The previous
/// file is kept as `idbseq.redb.pre-restore-<unix seconds>`.
///
/// # Errors
///
/// Not a backup, data dir in use, counters behind (without `force`), or I/O.
pub fn restore(
    data_dir: &Path,
    backup: &Path,
    force: bool,
) -> Result<Option<PathBuf>, BackupError> {
    let backup_db = check_backup(backup)?;
    let backup_counters = counters(&backup_db)?;
    drop(backup_db);
    let current_path = data_dir.join(DATABASE_FILE);
    let kept = if current_path.exists() {
        let current = Database::open(&current_path).map_err(|e| match e {
            DatabaseError::DatabaseAlreadyOpen => BackupError::InUse,
            other => storage(other),
        })?;
        let lagging = behind(&counters(&current)?, &backup_counters);
        drop(current);
        if !lagging.is_empty() && !force {
            return Err(BackupError::Behind(lagging));
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let kept = data_dir.join(format!("{DATABASE_FILE}.pre-restore-{stamp}"));
        std::fs::rename(&current_path, &kept).map_err(storage)?;
        Some(kept)
    } else {
        std::fs::create_dir_all(data_dir).map_err(storage)?;
        None
    };
    std::fs::copy(backup, &current_path).map_err(storage)?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behind_flags_lower_and_missing_counters() {
        let current: BTreeMap<String, Option<Id>> = [
            ("a".into(), Some(10)),
            ("b".into(), Some(5)),
            ("c".into(), None),
            ("d".into(), Some(1)),
        ]
        .into();
        let backup: BTreeMap<String, Option<Id>> = [
            ("a".into(), Some(10)),
            ("b".into(), Some(4)),
            ("c".into(), None),
        ]
        .into();

        assert_eq!(behind(&current, &backup), ["b", "d"]);
    }
}
