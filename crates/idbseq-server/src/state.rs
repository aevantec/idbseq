//! Shared application state.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use idbseq_core::engine::Engine;
use idbseq_core::store::{DEFAULT_BUSY_TIMEOUT, RedbStore, SequenceStore, SharedFileStore};

use crate::audit::{AuditLog, Event};
use crate::auth::admin::AdminAuth;
use crate::auth::keys::KeyStore;
use crate::auth::setup::Setup;
use crate::config::Config;
use crate::db::{self, Db, DbError, db_err};
use crate::error::ApiError;
use crate::metrics::Metrics;

/// Password checks (Argon2, about 19 MiB and tens of milliseconds each) that may
/// run at once. More wait their turn, so a burst of logins cannot exhaust memory
/// or the blocking threads that also issue IDs.
pub const MAX_CONCURRENT_PASSWORD_CHECKS: usize = 2;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    config: Config,
    database: Db,
    engine: Engine,
    admin: AdminAuth,
    setup: Setup,
    keys: KeyStore,
    audit: AuditLog,
    metrics: Metrics,
    limiter: Option<DefaultKeyedRateLimiter<String>>,
    password_checks: tokio::sync::Semaphore,
    ready: AtomicBool,
    /// `idbseqd console`: the one token that is the admin. `None` on a server.
    local_token: Option<String>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    /// Build the state around an open store.
    ///
    /// # Errors
    ///
    /// Server tables cannot be created or loaded.
    pub fn new(config: Config, store: &Arc<RedbStore>) -> Result<Self, DbError> {
        let engine_store: Arc<dyn SequenceStore> = Arc::clone(store) as Arc<dyn SequenceStore>;
        Self::build(config, engine_store, store.database(), None)
    }

    /// `idbseqd console`: IDs from `file`, opened for each call so other
    /// processes can use it too; keys, sessions and the audit log only in
    /// memory; and `token` as the admin.
    ///
    /// # Errors
    ///
    /// The in-memory tables cannot be created.
    pub fn local_console(config: Config, file: &Path, token: String) -> Result<Self, DbError> {
        let memory = ::redb::Database::builder()
            .create_with_backend(::redb::backends::InMemoryBackend::new())
            .map_err(db_err)?;
        let store = Arc::new(SharedFileStore::new(file, DEFAULT_BUSY_TIMEOUT));
        Self::build(config, store, Arc::new(memory), Some(token))
    }

    fn build(
        config: Config,
        engine_store: Arc<dyn SequenceStore>,
        database: Db,
        local_token: Option<String>,
    ) -> Result<Self, DbError> {
        db::init(&database)?;
        let engine = Engine::new(engine_store, config.engine_limits());
        let admin = AdminAuth::load(Arc::clone(&database), config.auth.session_ttl())?;
        let keys = KeyStore::load(Arc::clone(&database))?;
        let retention = Duration::from_hours(24 * config.audit.retention_days.max(1));
        let audit = AuditLog::new(Arc::clone(&database), retention);
        let limiter = rate_limiter(config.rate_limit.per_second, config.rate_limit.burst);
        Ok(Self(Arc::new(Inner {
            config,
            database,
            engine,
            admin,
            setup: Setup::default(),
            keys,
            audit,
            metrics: Metrics::new(),
            limiter,
            password_checks: tokio::sync::Semaphore::new(MAX_CONCURRENT_PASSWORD_CHECKS),
            ready: AtomicBool::new(true),
            local_token,
        })))
    }

    /// In `idbseqd console`, the token that is the admin.
    #[must_use]
    pub fn local_token(&self) -> Option<&str> {
        self.0.local_token.as_deref()
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    /// The redb file shared by the store and the server tables (for backups).
    #[must_use]
    pub fn database(&self) -> Db {
        Arc::clone(&self.0.database)
    }

    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.0.engine
    }

    #[must_use]
    pub fn admin(&self) -> &AdminAuth {
        &self.0.admin
    }

    /// Wait for a turn to check or hash a password (see
    /// [`MAX_CONCURRENT_PASSWORD_CHECKS`]); the turn ends when the permit drops.
    ///
    /// # Errors
    ///
    /// Only if the semaphore were closed, which never happens.
    pub async fn password_check(&self) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
        self.0
            .password_checks
            .acquire()
            .await
            .map_err(|e| ApiError::internal(&e))
    }

    /// The first-run setup link, if one is pending (US-0047).
    #[must_use]
    pub fn setup(&self) -> &Setup {
        &self.0.setup
    }

    #[must_use]
    pub fn keys(&self) -> &KeyStore {
        &self.0.keys
    }

    #[must_use]
    pub fn audit(&self) -> &AuditLog {
        &self.0.audit
    }

    #[must_use]
    pub fn metrics(&self) -> &Metrics {
        &self.0.metrics
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.0.ready.load(Ordering::SeqCst)
    }

    /// Mark the store as closing (readiness fails during shutdown).
    pub fn set_not_ready(&self) {
        self.0.ready.store(false, Ordering::SeqCst);
    }

    /// Apply the per-credential rate limit.
    ///
    /// # Errors
    ///
    /// `RATE_LIMITED` with the seconds to wait.
    pub fn check_rate(&self, credential: &str) -> Result<(), ApiError> {
        let Some(limiter) = &self.0.limiter else {
            return Ok(());
        };
        limiter
            .check_key(&credential.to_owned())
            .map_err(|not_until| {
                let wait = not_until.wait_time_from(DefaultClock::default().now());
                ApiError::rate_limited(wait.as_secs() + u64::from(wait.subsec_nanos() > 0))
            })
    }

    /// Record an audit event. A failure is logged, never shown to the caller:
    /// the operation itself already succeeded.
    pub fn record(&self, event: Event) {
        if let Err(error) = self.0.audit.append(event, SystemTime::now()) {
            tracing::error!(%error, "failed to write audit entry");
        }
    }
}

fn rate_limiter(per_second: u32, burst: u32) -> Option<DefaultKeyedRateLimiter<String>> {
    let rate = NonZeroU32::new(per_second)?;
    let burst = NonZeroU32::new(burst).unwrap_or(rate);
    Some(RateLimiter::keyed(
        Quota::per_second(rate).allow_burst(burst),
    ))
}
