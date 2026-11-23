//! The single admin account: password, sessions and login lockout (US-0021, US-0022).

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use serde::{Deserialize, Serialize};

use super::lockout::{Locked, Lockouts};
use super::secrets::{constant_time_eq, random_token, sha256_hex};
use crate::db::{self, ADMIN, Db, DbError, SESSIONS};

/// The admin's username.
pub const ADMIN_USERNAME: &str = "admin";
/// Shortest allowed admin password, in characters.
pub const MIN_PASSWORD_LEN: usize = 8;
pub use super::lockout::{
    LOCKOUT_DURATION, LOCKOUT_WINDOW, MAX_FAILED_LOGINS, MAX_FAILED_LOGINS_GLOBAL,
};
/// A session can be refreshed, but never past this long after sign-in.
pub const MAX_SESSION_LIFETIME: Duration = Duration::from_hours(12);

const ADMIN_KEY: &str = "admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdminRecord {
    password_hash: String,
    updated_at: SystemTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SessionRecord {
    expires_at: SystemTime,
    /// When the session began, for [`MAX_SESSION_LIFETIME`]. Absent in
    /// records written before sessions could be refreshed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started_at: Option<SystemTime>,
}

/// Why an admin operation failed.
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("no admin account exists yet; run `idbseqd admin init`")]
    NotInitialized,
    #[error("an admin account already exists")]
    AlreadyInitialized,
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    WeakPassword,
    #[error("invalid username or password")]
    InvalidCredentials,
    /// Refused without checking the password: too many recent failures. Answered
    /// like wrong credentials.
    #[error("invalid username or password")]
    Locked,
    #[error(transparent)]
    Storage(#[from] DbError),
    #[error("password hashing failed: {0}")]
    Hash(String),
}

/// Admin account and session state. Sessions are cached in memory and persisted
/// as SHA-256 hashes of the token.
pub struct AdminAuth {
    db: Db,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    lockouts: Mutex<Lockouts>,
    session_ttl: Duration,
}

impl std::fmt::Debug for AdminAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminAuth").finish_non_exhaustive()
    }
}

/// A new session: the token (shown once) and when it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token: String,
    pub expires_at: SystemTime,
}

fn hash_password(password: &str) -> Result<String, AdminError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| AdminError::Hash(e.to_string()))
}

fn check_strength(password: &str) -> Result<(), AdminError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AdminError::WeakPassword);
    }
    Ok(())
}

impl AdminAuth {
    /// Load persisted sessions. Expired ones are dropped lazily.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn load(db: Db, session_ttl: Duration) -> Result<Self, DbError> {
        let sessions = load_sessions(&db)?;
        Ok(Self {
            db,
            sessions: RwLock::new(sessions),
            lockouts: Mutex::new(Lockouts::default()),
            session_ttl,
        })
    }

    fn record(&self) -> Result<Option<AdminRecord>, DbError> {
        db::get(&self.db, ADMIN, ADMIN_KEY)
    }

    /// Whether `admin init` has been run.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn exists(&self) -> Result<bool, DbError> {
        Ok(self.record()?.is_some())
    }

    /// Create the admin account. Fails if one exists.
    ///
    /// # Errors
    ///
    /// `AlreadyInitialized`, `WeakPassword`, storage or hashing failure.
    pub fn init(&self, password: &str, now: SystemTime) -> Result<(), AdminError> {
        if self.exists()? {
            return Err(AdminError::AlreadyInitialized);
        }
        self.store_password(password, now)
    }

    /// Set a new password (local recovery or a signed-in change) and revoke every session.
    ///
    /// # Errors
    ///
    /// `WeakPassword`, storage or hashing failure.
    pub fn set_password(&self, password: &str, now: SystemTime) -> Result<(), AdminError> {
        self.store_password(password, now)?;
        self.revoke_all_sessions()?;
        // A new password makes earlier guesses moot, and a local reset is the way
        // back in for an admin locked out from every address.
        self.lockouts().clear();
        Ok(())
    }

    fn store_password(&self, password: &str, now: SystemTime) -> Result<(), AdminError> {
        check_strength(password)?;
        let record = AdminRecord {
            password_hash: hash_password(password)?,
            updated_at: now,
        };
        db::write(&self.db, ADMIN, &[(ADMIN_KEY, &record)], &[])?;
        Ok(())
    }

    /// Whether `password` is the admin password. Slow (Argon2id): call off the async runtime.
    ///
    /// # Errors
    ///
    /// `NotInitialized` or storage failure.
    pub fn verify_password(&self, password: &str) -> Result<bool, AdminError> {
        let record = self.record()?.ok_or(AdminError::NotInitialized)?;
        let parsed = argon2::password_hash::phc::PasswordHash::new(&record.password_hash)
            .map_err(|e| AdminError::Hash(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }

    /// Sign in from `client` (see [`super::lockout::client_key`]). Wrong
    /// credentials and a lockout look the same to the caller.
    ///
    /// # Errors
    ///
    /// `InvalidCredentials`, `Locked`, `NotInitialized` or storage failure.
    pub fn login(
        &self,
        client: &str,
        username: &str,
        password: &str,
        now: SystemTime,
    ) -> Result<Session, AdminError> {
        self.lockouts()
            .begin(client, now)
            .map_err(|Locked| AdminError::Locked)?;
        let password_ok = self.verify_password(password)?;
        let username_ok = constant_time_eq(username, ADMIN_USERNAME);
        if !(password_ok && username_ok) {
            return Err(AdminError::InvalidCredentials);
        }
        self.lockouts().succeeded(client, now);
        self.start_session(now)
    }

    fn start_session(&self, now: SystemTime) -> Result<Session, AdminError> {
        let token = random_token();
        let record = SessionRecord {
            expires_at: now + self.session_ttl,
            started_at: Some(now),
        };
        let hash = sha256_hex(&token);
        db::write(&self.db, SESSIONS, &[(hash.as_str(), &record)], &[])?;
        self.sessions_mut().insert(hash, record);
        Ok(Session {
            token,
            expires_at: record.expires_at,
        })
    }

    /// The session's hash if `token` is a live session.
    #[must_use]
    pub fn session(&self, token: &str, now: SystemTime) -> Option<String> {
        let hash = sha256_hex(token);
        let sessions = self.sessions.read().unwrap_or_else(PoisonError::into_inner);
        sessions
            .get(&hash)
            .filter(|s| s.expires_at > now)
            .map(|_| hash)
    }

    /// Revoke one session.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn logout(&self, session_hash: &str) -> Result<(), DbError> {
        // Held across the write so a concurrent refresh cannot write it back.
        let mut sessions = self.sessions_mut();
        db::write::<SessionRecord>(&self.db, SESSIONS, &[], &[session_hash])?;
        sessions.remove(session_hash);
        Ok(())
    }

    /// Extend a live session to a full TTL from `now`, but never past
    /// [`MAX_SESSION_LIFETIME`] after sign-in. Returns the new expiry, or
    /// `None` if the session is unknown or has expired.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn refresh(
        &self,
        session_hash: &str,
        now: SystemTime,
    ) -> Result<Option<SystemTime>, DbError> {
        let mut sessions = self.sessions_mut();
        let Some(current) = sessions
            .get(session_hash)
            .filter(|s| s.expires_at > now)
            .copied()
        else {
            return Ok(None);
        };
        let started = current
            .started_at
            .or_else(|| current.expires_at.checked_sub(self.session_ttl))
            .unwrap_or(now);
        let limit = started + MAX_SESSION_LIFETIME.max(self.session_ttl);
        let updated = SessionRecord {
            expires_at: (now + self.session_ttl).min(limit).max(current.expires_at),
            started_at: Some(started),
        };
        // Held across the write so it cannot race a logout or password change.
        db::write(&self.db, SESSIONS, &[(session_hash, &updated)], &[])?;
        sessions.insert(session_hash.to_owned(), updated);
        Ok(Some(updated.expires_at))
    }

    /// Revoke every session (password change).
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn revoke_all_sessions(&self) -> Result<(), DbError> {
        let hashes: Vec<String> = self.sessions_mut().drain().map(|(hash, _)| hash).collect();
        let persisted = load_sessions(&self.db)?;
        let all: Vec<&str> = hashes
            .iter()
            .map(String::as_str)
            .chain(persisted.keys().map(String::as_str))
            .collect();
        db::write::<SessionRecord>(&self.db, SESSIONS, &[], &all)
    }

    /// Remove expired sessions from memory and disk.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn purge_expired(&self, now: SystemTime) -> Result<usize, DbError> {
        let expired: Vec<String> = {
            let mut sessions = self.sessions_mut();
            let expired: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| s.expires_at <= now)
                .map(|(hash, _)| hash.clone())
                .collect();
            sessions.retain(|_, s| s.expires_at > now);
            expired
        };
        let keys: Vec<&str> = expired.iter().map(String::as_str).collect();
        db::write::<SessionRecord>(&self.db, SESSIONS, &[], &keys)?;
        Ok(expired.len())
    }

    fn sessions_mut(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, SessionRecord>> {
        self.sessions
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lockouts(&self) -> std::sync::MutexGuard<'_, Lockouts> {
        self.lockouts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn load_sessions(db: &Db) -> Result<HashMap<String, SessionRecord>, DbError> {
    use redb::{ReadableDatabase, ReadableTable};
    let txn = db.begin_read().map_err(crate::db::db_err)?;
    let table = txn.open_table(SESSIONS).map_err(crate::db::db_err)?;
    table
        .iter()
        .map_err(crate::db::db_err)?
        .map(|entry| {
            let (key, value) = entry.map_err(crate::db::db_err)?;
            let record: SessionRecord =
                serde_json::from_slice(value.value()).map_err(crate::db::db_err)?;
            Ok((key.value().to_owned(), record))
        })
        .collect()
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
