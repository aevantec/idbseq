//! Shared application state.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use idbseq_core::engine::Engine;
use idbseq_core::store::{RedbStore, SequenceStore};

use crate::audit::{AuditLog, Event};
use crate::auth::admin::AdminAuth;
use crate::auth::keys::KeyStore;
use crate::config::Config;
use crate::db::{self, Db, DbError};
use crate::error::ApiError;
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    config: Config,
    database: Db,
    engine: Engine,
    admin: AdminAuth,
    keys: KeyStore,
    audit: AuditLog,
    metrics: Metrics,
    limiter: Option<DefaultKeyedRateLimiter<String>>,
    ready: AtomicBool,
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
        let database: Db = store.database();
        db::init(&database)?;
        let engine_store: Arc<dyn SequenceStore> = Arc::clone(store) as Arc<dyn SequenceStore>;
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
            keys,
            audit,
            metrics: Metrics::new(),
            limiter,
            ready: AtomicBool::new(true),
        })))
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
