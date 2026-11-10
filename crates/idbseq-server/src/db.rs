//! Server tables in the same redb file as the sequences: admin, sessions,
//! API keys and the audit log. Values are JSON.

use std::sync::Arc;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub const ADMIN: TableDefinition<&str, &[u8]> = TableDefinition::new("admin");
pub const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
pub const API_KEYS: TableDefinition<&str, &[u8]> = TableDefinition::new("api_keys");
pub const AUDIT: TableDefinition<u64, &[u8]> = TableDefinition::new("audit");

/// A storage failure in a server table.
#[derive(Debug, thiserror::Error)]
#[error("server table error: {0}")]
pub struct DbError(String);

#[allow(clippy::needless_pass_by_value)] // used as `map_err(db_err)`
pub fn db_err(error: impl std::fmt::Display) -> DbError {
    DbError(error.to_string())
}

impl From<DbError> for crate::error::ApiError {
    fn from(error: DbError) -> Self {
        Self::internal(&error)
    }
}

/// Create the server tables if they do not exist yet.
///
/// # Errors
///
/// Storage failure.
pub fn init(db: &Database) -> Result<(), DbError> {
    let txn = db.begin_write().map_err(db_err)?;
    txn.open_table(ADMIN).map_err(db_err)?;
    txn.open_table(SESSIONS).map_err(db_err)?;
    txn.open_table(API_KEYS).map_err(db_err)?;
    txn.open_table(AUDIT).map_err(db_err)?;
    txn.commit().map_err(db_err)
}

/// Read one JSON value.
///
/// # Errors
///
/// Storage or decoding failure.
pub fn get<T: DeserializeOwned>(
    db: &Database,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Option<T>, DbError> {
    let txn = db.begin_read().map_err(db_err)?;
    let table = txn.open_table(table).map_err(db_err)?;
    let value = table.get(key).map_err(db_err)?;
    value
        .map(|guard| serde_json::from_slice(guard.value()).map_err(db_err))
        .transpose()
}

/// Every JSON value of a table, in key order.
///
/// # Errors
///
/// Storage or decoding failure.
pub fn all<T: DeserializeOwned>(
    db: &Database,
    table: TableDefinition<&str, &[u8]>,
) -> Result<Vec<T>, DbError> {
    let txn = db.begin_read().map_err(db_err)?;
    let table = txn.open_table(table).map_err(db_err)?;
    table
        .iter()
        .map_err(db_err)?
        .map(|entry| {
            let (_, value) = entry.map_err(db_err)?;
            serde_json::from_slice(value.value()).map_err(db_err)
        })
        .collect()
}

/// Write several JSON values and remove keys in one durable transaction.
///
/// # Errors
///
/// Storage or encoding failure.
pub fn write<T: Serialize>(
    db: &Database,
    table: TableDefinition<&str, &[u8]>,
    puts: &[(&str, &T)],
    removes: &[&str],
) -> Result<(), DbError> {
    let txn = db.begin_write().map_err(db_err)?;
    {
        let mut table = txn.open_table(table).map_err(db_err)?;
        for (key, value) in puts {
            let bytes = serde_json::to_vec(value).map_err(db_err)?;
            table.insert(*key, bytes.as_slice()).map_err(db_err)?;
        }
        for key in removes {
            table.remove(*key).map_err(db_err)?;
        }
    }
    txn.commit().map_err(db_err)
}

/// Shared handle type.
pub type Db = Arc<Database>;
