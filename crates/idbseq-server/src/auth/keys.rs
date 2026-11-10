//! Client API keys `idb_<id>_<secret>` (US-0023). Only a SHA-256 of the secret is
//! stored. Keys are cached in memory; `last_used_at` is buffered and flushed
//! periodically so authenticating never needs a disk write.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError, RwLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::secrets::{constant_time_eq, random_id, random_token, sha256_hex};
use crate::db::{self, API_KEYS, Db, DbError};

pub const KEY_PREFIX: &str = "idb";
pub const MAX_KEY_NAME_LEN: usize = 64;

/// What a key may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Generate,
    Manage,
}

/// A stored API key (never contains the secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub name: String,
    pub scopes: Vec<Scope>,
    /// Exact names or prefixes ending in `*`. Empty allows every collection.
    pub collections: Vec<String>,
    secret_hash: String,
    pub created_at: SystemTime,
    pub last_used_at: Option<SystemTime>,
}

impl KeyRecord {
    #[must_use]
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// Whether the allow-list admits `collection`.
    #[must_use]
    pub fn allows(&self, collection: &str) -> bool {
        self.collections.is_empty()
            || self
                .collections
                .iter()
                .any(|pattern| match pattern.strip_suffix('*') {
                    Some(prefix) => collection.starts_with(prefix),
                    None => pattern == collection,
                })
    }
}

/// A new key's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewKey {
    pub name: String,
    pub scopes: Vec<Scope>,
    pub collections: Vec<String>,
}

impl NewKey {
    /// Every rule this request breaks.
    #[must_use]
    pub fn problems(&self) -> idbseq_core::validation::ValidationErrors {
        let name_len = self.name.chars().count();
        let bad_pattern = self.collections.iter().any(|p| {
            let base = p.strip_suffix('*').unwrap_or(p);
            base.contains('*')
                || (p == base && idbseq_core::validation::name_problem(base).is_some())
        });
        idbseq_core::validation::ValidationErrors::default()
            .check(
                name_len == 0 || name_len > MAX_KEY_NAME_LEN,
                "name",
                "must be 1-64 characters",
            )
            .check(
                self.scopes.is_empty(),
                "scopes",
                "must contain at least one of read, generate, manage",
            )
            .check(
                bad_pattern,
                "collections",
                "each entry must be a collection name or a prefix ending in '*'",
            )
    }
}

pub struct KeyStore {
    db: Db,
    keys: RwLock<HashMap<String, KeyRecord>>,
    used: Mutex<HashMap<String, SystemTime>>,
}

impl std::fmt::Debug for KeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyStore").finish_non_exhaustive()
    }
}

/// Why a key operation failed.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error(transparent)]
    Invalid(#[from] idbseq_core::validation::ValidationErrors),
    #[error("API key `{0}` not found")]
    NotFound(String),
    #[error(transparent)]
    Storage(#[from] DbError),
}

impl KeyStore {
    /// Load every key into memory.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn load(db: Db) -> Result<Self, DbError> {
        let records: Vec<KeyRecord> = db::all(&db, API_KEYS)?;
        let keys = records.into_iter().map(|k| (k.id.clone(), k)).collect();
        Ok(Self {
            db,
            keys: RwLock::new(keys),
            used: Mutex::new(HashMap::new()),
        })
    }

    /// Create a key. Returns the record and the full key, which is never shown again.
    ///
    /// # Errors
    ///
    /// `Invalid` or storage failure.
    pub fn create(&self, new: NewKey, now: SystemTime) -> Result<(KeyRecord, String), KeyError> {
        new.problems().into_result(())?;
        let id = random_id();
        let secret = random_token();
        let mut scopes = new.scopes;
        scopes.sort_by_key(|s| *s as u8);
        scopes.dedup();
        let record = KeyRecord {
            id: id.clone(),
            name: new.name,
            scopes,
            collections: new.collections,
            secret_hash: sha256_hex(&secret),
            created_at: now,
            last_used_at: None,
        };
        db::write(&self.db, API_KEYS, &[(id.as_str(), &record)], &[])?;
        self.keys_mut().insert(id.clone(), record.clone());
        Ok((record, format!("{KEY_PREFIX}_{id}_{secret}")))
    }

    /// Every key, oldest first, with the latest `last_used_at`.
    #[must_use]
    pub fn list(&self) -> Vec<KeyRecord> {
        let used = self.used();
        let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
        let mut records: Vec<KeyRecord> = keys
            .values()
            .map(|k| KeyRecord {
                last_used_at: used.get(&k.id).copied().or(k.last_used_at),
                ..k.clone()
            })
            .collect();
        records.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        records
    }

    /// Revoke a key immediately.
    ///
    /// # Errors
    ///
    /// `NotFound` or storage failure.
    pub fn revoke(&self, id: &str) -> Result<KeyRecord, KeyError> {
        let record = self
            .keys
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .ok_or_else(|| KeyError::NotFound(id.to_owned()))?;
        db::write::<KeyRecord>(&self.db, API_KEYS, &[], &[id])?;
        self.keys_mut().remove(id);
        self.used().remove(id);
        Ok(record)
    }

    /// The key for a bearer token `idb_<id>_<secret>`, recording its use.
    #[must_use]
    pub fn authenticate(&self, token: &str, now: SystemTime) -> Option<KeyRecord> {
        let mut parts = token.splitn(3, '_');
        let (prefix, id, secret) = (parts.next()?, parts.next()?, parts.next()?);
        if prefix != KEY_PREFIX {
            return None;
        }
        let record = self
            .keys
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()?;
        if !constant_time_eq(&sha256_hex(secret), &record.secret_hash) {
            return None;
        }
        self.used().insert(id.to_owned(), now);
        Some(record)
    }

    /// Persist buffered `last_used_at` values. Returns how many keys were written.
    ///
    /// # Errors
    ///
    /// Storage failure (the buffer is kept for the next attempt).
    pub fn flush_last_used(&self) -> Result<usize, DbError> {
        let pending: HashMap<String, SystemTime> = self.used().clone();
        if pending.is_empty() {
            return Ok(0);
        }
        let updated: Vec<KeyRecord> = {
            let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
            pending
                .iter()
                .filter_map(|(id, at)| {
                    keys.get(id).map(|k| KeyRecord {
                        last_used_at: Some(*at),
                        ..k.clone()
                    })
                })
                .collect()
        };
        let written = updated.len();
        {
            let puts: Vec<(&str, &KeyRecord)> =
                updated.iter().map(|k| (k.id.as_str(), k)).collect();
            db::write(&self.db, API_KEYS, &puts, &[])?;
        }
        let mut keys = self.keys_mut();
        let mut used = self.used();
        for record in updated {
            if used.get(&record.id) == record.last_used_at.as_ref() {
                used.remove(&record.id);
            }
            keys.insert(record.id.clone(), record);
        }
        Ok(written)
    }

    fn keys_mut(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, KeyRecord>> {
        self.keys.write().unwrap_or_else(PoisonError::into_inner)
    }

    fn used(&self) -> std::sync::MutexGuard<'_, HashMap<String, SystemTime>> {
        self.used.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
