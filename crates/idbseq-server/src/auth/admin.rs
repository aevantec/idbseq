//! The single admin account: password, sessions and login lockout (US-0021, US-0022).

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use serde::{Deserialize, Serialize};

use super::secrets::{constant_time_eq, random_token, sha256_hex};
use crate::db::{self, ADMIN, Db, DbError, SESSIONS};

/// The admin's username.
pub const ADMIN_USERNAME: &str = "admin";
/// Shortest allowed admin password, in characters.
pub const MIN_PASSWORD_LEN: usize = 12;
/// Failed logins allowed within [`LOCKOUT_WINDOW`] before the account locks.
pub const MAX_FAILED_LOGINS: usize = 5;
pub const LOCKOUT_WINDOW: Duration = Duration::from_mins(15);
pub const LOCKOUT_DURATION: Duration = Duration::from_mins(15);

const ADMIN_KEY: &str = "admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdminRecord {
    password_hash: String,
    updated_at: SystemTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SessionRecord {
    expires_at: SystemTime,
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
    #[error(transparent)]
    Storage(#[from] DbError),
    #[error("password hashing failed: {0}")]
    Hash(String),
}

#[derive(Debug, Default)]
struct Lockout {
    failures: VecDeque<SystemTime>,
    locked_until: Option<SystemTime>,
}

/// Admin account and session state. Sessions are cached in memory and persisted
/// as SHA-256 hashes of the token.
pub struct AdminAuth {
    db: Db,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    lockout: Mutex<Lockout>,
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
            lockout: Mutex::new(Lockout::default()),
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

    /// Sign in. Wrong username or password and a locked account give the same error.
    ///
    /// # Errors
    ///
    /// `InvalidCredentials`, `NotInitialized` or storage failure.
    pub fn login(
        &self,
        username: &str,
        password: &str,
        now: SystemTime,
    ) -> Result<Session, AdminError> {
        if self.is_locked(now) {
            return Err(AdminError::InvalidCredentials);
        }
        let password_ok = self.verify_password(password)?;
        let username_ok = constant_time_eq(username, ADMIN_USERNAME);
        if !(password_ok && username_ok) {
            self.record_failure(now);
            return Err(AdminError::InvalidCredentials);
        }
        self.clear_failures();
        self.start_session(now)
    }

    fn start_session(&self, now: SystemTime) -> Result<Session, AdminError> {
        let token = random_token();
        let record = SessionRecord {
            expires_at: now + self.session_ttl,
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
        db::write::<SessionRecord>(&self.db, SESSIONS, &[], &[session_hash])?;
        self.sessions_mut().remove(session_hash);
        Ok(())
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

    fn lockout(&self) -> std::sync::MutexGuard<'_, Lockout> {
        self.lockout.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_locked(&self, now: SystemTime) -> bool {
        self.lockout().locked_until.is_some_and(|until| now < until)
    }

    fn record_failure(&self, now: SystemTime) {
        let mut lockout = self.lockout();
        let window_start = now
            .checked_sub(LOCKOUT_WINDOW)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        lockout.failures.retain(|at| *at > window_start);
        lockout.failures.push_back(now);
        if lockout.failures.len() >= MAX_FAILED_LOGINS {
            lockout.locked_until = Some(now + LOCKOUT_DURATION);
            lockout.failures.clear();
        }
    }

    fn clear_failures(&self) {
        let mut lockout = self.lockout();
        lockout.failures.clear();
        lockout.locked_until = None;
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
