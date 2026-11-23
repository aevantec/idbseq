//! `idbseqd console <file>`: the admin console on an embedded idbseq file.
//!
//! The file is opened for each request, so the app that owns it keeps taking
//! IDs while the console is open (in shared mode, or between its own opens).
//! There are no accounts: the server listens on 127.0.0.1 only and accepts one
//! random token, printed in the console link. Keys and the audit log are not
//! available, and nothing but collections is written to the file.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum_server::Handle;
use idbseq_core::store::{BlockCache, DEFAULT_BUSY_TIMEOUT, open_waiting};

use crate::app::router;
use crate::auth::secrets;
use crate::config::{Config, MetricsConfig, RateLimitConfig};
use crate::serve::{ServeError, shutdown_on_signal};
use crate::state::AppState;

/// The console's settings: the server defaults with the console on, metrics,
/// API docs and rate limiting off, and only `127.0.0.1:<port>`.
#[must_use]
pub fn config(port: u16) -> Config {
    Config {
        bind: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        ui: true,
        swagger_ui: false,
        tls: None,
        metrics: MetricsConfig { enabled: false },
        rate_limit: RateLimitConfig {
            per_second: 0,
            burst: 0,
        },
        log: "warn".to_owned(),
        ..Config::default()
    }
}

/// Build the console's state for `file` with a fresh token, checking that the
/// file opens (or, with `create`, creating it).
///
/// # Errors
///
/// The file does not exist or cannot be opened (or stays busy).
pub fn open(file: &Path, port: u16, create: bool) -> Result<AppState, ServeError> {
    if !create && !file.is_file() {
        return Err(ServeError::Console(format!(
            "{} does not exist: check the path, or pass --create for a new file",
            file.display()
        )));
    }
    if create {
        // The console's own store never creates the file (so a file deleted
        // while it runs is reported, not replaced): create it here, once.
        drop(
            open_waiting(file, BlockCache::Off, DEFAULT_BUSY_TIMEOUT)
                .map_err(|error| ServeError::Console(error.to_string()))?,
        );
    }
    let state = AppState::local_console(config(port), file, secrets::random_token())?;
    // Forget expired retry keys; this also checks the file opens.
    state
        .engine()
        .purge_idempotency(SystemTime::now())
        .map_err(|error| ServeError::Console(error.to_string()))?;
    Ok(state)
}

/// The link that opens the console signed in: the token and the file's name
/// travel in the URL fragment, which browsers never send to the server.
#[must_use]
pub fn link(addr: SocketAddr, token: &str, file: &Path) -> String {
    let name = file
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    format!(
        "http://{addr}/ui/#/local?token={token}&file={}",
        percent_encode(&name)
    )
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Serve the console for `file` until Ctrl-C.
///
/// # Errors
///
/// The file cannot be opened, or the port cannot be bound.
pub async fn run(file: PathBuf, port: u16, create: bool) -> Result<(), ServeError> {
    let state = open(&file, port, create)?;
    let token = state.local_token().unwrap_or_default().to_owned();
    let handle: Handle<SocketAddr> = Handle::new();
    tokio::spawn(shutdown_on_signal(handle.clone()));
    let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(
        axum_server::bind(state.config().bind)
            .handle(handle.clone())
            .serve(app),
    );
    let Some(addr) = handle.listening().await else {
        return match server.await {
            Ok(Err(error)) => Err(ServeError::Io(error)),
            _ => Err(ServeError::Console("the console did not start".to_owned())),
        };
    };
    // Printed rather than logged: the person running the command needs it.
    eprintln!(
        "\nidbseqd console: {}\n\nOpen this link (it signs you in; keep it private):\n\n{}\n\nPress Ctrl-C to stop.\n",
        file.display(),
        link(addr, &token, &file)
    );
    server
        .await
        .map_err(|error| ServeError::Console(error.to_string()))?
        .map_err(ServeError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_carries_the_token_and_an_encoded_file_name() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7900));

        let url = link(addr, "abc", Path::new("/data/my ids+1.idbseq"));

        assert_eq!(
            url,
            "http://127.0.0.1:7900/ui/#/local?token=abc&file=my%20ids%2B1.idbseq"
        );
    }

    #[test]
    fn the_console_listens_on_loopback_only_with_the_ui_on() {
        let config = config(0);

        assert!(config.bind.ip().is_loopback());
        assert!(config.ui && !config.swagger_ui && !config.metrics.enabled);
    }

    #[test]
    fn a_missing_file_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");

        let opened = open(&dir.path().join("nope.idbseq"), 0, false);

        assert!(
            matches!(opened, Err(ServeError::Console(message)) if message.contains("does not exist"))
        );
    }
}
