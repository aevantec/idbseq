//! Running the server: logging, store, admin bootstrap, background maintenance,
//! TLS, and graceful shutdown on SIGTERM / Ctrl-C (US-0014, US-0026).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use idbseq_core::store::RedbStore;
use idbseq_core::store::redb::OpenError;

use crate::app::{SHUTDOWN_GRACE, router};
use crate::auth::setup;
use crate::config::Config;
use crate::state::AppState;

const KEY_FLUSH_INTERVAL: Duration = Duration::from_mins(1);
const PURGE_INTERVAL: Duration = Duration::from_hours(1);

/// Why the server could not start or stopped with an error.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error("cannot initialise server tables: {0}")]
    Tables(#[from] crate::db::DbError),
    #[error("cannot create the admin from IDBSEQ_ADMIN_PASSWORD: {0}")]
    Bootstrap(crate::auth::admin::AdminError),
    #[error("TLS setup failed: {0}")]
    Tls(String),
    #[error("server error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Console(String),
}

/// Install JSON logs filtered by `config.log`. Safe to call once per process.
pub fn init_logging(filter: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(filter)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(true)
        .try_init();
}

/// Open the store and build the state, creating the admin from
/// `admin_password` if none exists yet (ignored once an admin exists).
///
/// # Errors
///
/// Store, tables or bootstrap failure.
pub fn open_state(
    config: &Config,
    admin_password: Option<&str>,
) -> Result<(AppState, Arc<RedbStore>), ServeError> {
    let store = Arc::new(RedbStore::open(&config.data_dir, config.group_commit())?);
    let state = AppState::new(config.clone(), &store)?;
    if let Some(password) = admin_password {
        bootstrap_admin(&state, password)?;
    }
    Ok((state, store))
}

fn bootstrap_admin(state: &AppState, password: &str) -> Result<(), ServeError> {
    if state.admin().exists()? {
        tracing::info!("IDBSEQ_ADMIN_PASSWORD ignored: an admin account already exists");
        return Ok(());
    }
    state
        .admin()
        .init(password, SystemTime::now())
        .map_err(ServeError::Bootstrap)?;
    tracing::info!("admin account created from IDBSEQ_ADMIN_PASSWORD");
    Ok(())
}

/// With no admin yet: print a one-time setup link when the console is on (US-0047),
/// otherwise say how to create the admin.
fn offer_setup(config: &Config, state: &AppState) {
    if !config.ui {
        tracing::warn!("no admin account: run `idbseqd admin init` or set IDBSEQ_ADMIN_PASSWORD");
        return;
    }
    let token = state.setup().issue(SystemTime::now());
    let url = setup::link(config.bind, config.tls.is_some(), &token);
    let minutes = setup::SETUP_TTL.as_secs() / 60;
    // Printed rather than logged at a level a filter could hide: the person
    // starting the server needs this line to finish setting it up.
    eprintln!(
        "\nidbseqd: no admin account yet. Open this link within {minutes} minutes to choose the admin password:\n\n{url}\n\n(It works once. Restart idbseqd for a new link, or run `idbseqd admin init` instead.)\n"
    );
    tracing::warn!(
        "no admin account: waiting for first-run setup through the console link printed on stderr"
    );
}

/// Serve until SIGTERM or Ctrl-C, then finish in-flight requests and close the store.
///
/// # Errors
///
/// Startup or I/O failure.
pub async fn run(config: Config, admin_password: Option<String>) -> Result<(), ServeError> {
    let (state, store) = open_state(&config, admin_password.as_deref())?;
    if !state.admin().exists()? {
        offer_setup(&config, &state);
    }
    warn_if_exposed(&config);
    let local = crate::local::listen(&config.data_dir, state.clone())
        .inspect_err(|error| {
            tracing::warn!(
                %error,
                socket = %crate::local::socket_path(&config.data_dir).display(),
                "local admin socket unavailable; stop the server to run `admin` or `backup` commands"
            );
        })
        .ok();
    let maintenance = tokio::spawn(maintenance(state.clone()));

    let handle: Handle<SocketAddr> = Handle::new();
    tokio::spawn(shutdown_on_signal(handle.clone()));
    let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    tracing::info!(bind = %config.bind, tls = config.tls.is_some(), "idbseqd listening");
    let served = match &config.tls {
        Some(tls) => {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let tls = RustlsConfig::from_pem_file(&tls.cert, &tls.key)
                .await
                .map_err(|e| ServeError::Tls(e.to_string()))?;
            axum_server::bind_rustls(config.bind, tls)
                .handle(handle)
                .serve(app)
                .await
        }
        None => {
            axum_server::bind(config.bind)
                .handle(handle)
                .serve(app)
                .await
        }
    };

    state.set_not_ready();
    maintenance.abort();
    if let Some(local) = local {
        local.abort();
    }
    crate::local::remove_socket(&config.data_dir);
    flush_keys(&state);
    drop(state);
    drop(store);
    tracing::info!("idbseqd stopped; store closed");
    served.map_err(ServeError::Io)
}

/// True for loopback addresses (127.0.0.0/8, `::1`).
#[must_use]
pub fn is_loopback(addr: SocketAddr) -> bool {
    addr.ip().is_loopback()
}

fn warn_if_exposed(config: &Config) {
    if !is_loopback(config.bind) && config.tls.is_none() {
        tracing::warn!(
            bind = %config.bind,
            "listening on a non-loopback address without TLS: credentials travel in clear text"
        );
    }
}

/// Finish in-flight requests and stop on Ctrl-C or SIGTERM.
pub(crate) async fn shutdown_on_signal(handle: Handle<SocketAddr>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down: finishing in-flight requests");
    handle.graceful_shutdown(Some(SHUTDOWN_GRACE));
}

/// Periodic upkeep: flush key usage; purge idempotency keys, sessions and old audit entries.
async fn maintenance(state: AppState) {
    let mut flush = tokio::time::interval(KEY_FLUSH_INTERVAL);
    let mut purge = tokio::time::interval(PURGE_INTERVAL);
    loop {
        tokio::select! {
            _ = flush.tick() => {
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || flush_keys(&s)).await;
            }
            _ = purge.tick() => {
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || purge_expired(&s)).await;
            }
        }
    }
}

fn flush_keys(state: &AppState) {
    if let Err(error) = state.keys().flush_last_used() {
        tracing::error!(%error, "failed to persist API key usage");
    }
}

fn purge_expired(state: &AppState) {
    let now = SystemTime::now();
    if let Err(error) = state.engine().purge_idempotency(now) {
        tracing::error!(%error, "failed to purge idempotency keys");
    }
    if let Err(error) = state.admin().purge_expired(now) {
        tracing::error!(%error, "failed to purge sessions");
    }
    match state.audit().purge(now) {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "purged audit entries past retention"),
        Err(error) => tracing::error!(%error, "failed to purge audit log"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("127.0.0.1:7878".parse().expect("addr")));
        assert!(is_loopback("[::1]:7878".parse().expect("addr")));
        assert!(!is_loopback("0.0.0.0:7878".parse().expect("addr")));
        assert!(!is_loopback("10.0.0.5:7878".parse().expect("addr")));
    }

    #[test]
    fn bootstrap_creates_the_admin_only_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        let (state, store) = open_state(&config, Some("first password!!")).expect("open");
        let first_ok = state
            .admin()
            .verify_password("first password!!")
            .expect("verify");
        drop((state, store));

        let (state, _store) = open_state(&config, Some("second password!!")).expect("reopen");

        assert!(first_ok);
        assert!(
            state
                .admin()
                .verify_password("first password!!")
                .expect("verify")
        );
    }

    #[test]
    fn a_weak_bootstrap_password_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        };

        let result = open_state(&config, Some("short"));

        assert!(matches!(result, Err(ServeError::Bootstrap(_))));
    }
}
