//! Local administration channel: a Unix socket in the data directory (mode 0600).
//!
//! The running server owns the redb file, so local commands that need it while the
//! server runs (`backup`, `admin init`, `admin reset-password`) are sent here.
//! Access to the data directory is the authority, as documented for recovery.
//!
//! Protocol: one request line, one response line `ok <message>` or `error <message>`.
//! Requests: `backup <path>`, `admin-init <password>`, `admin-reset-password <password>`.
//!
//! Not on Windows yet (US-0042): there `request` finds no server, so these
//! commands open the data file themselves and need the server stopped.

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::auth::admin::AdminError;
use crate::state::AppState;

pub const SOCKET_FILE: &str = "idbseqd.sock";

#[must_use]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOCKET_FILE)
}

/// Remove the socket file (on shutdown).
pub fn remove_socket(data_dir: &Path) {
    let _ = std::fs::remove_file(socket_path(data_dir));
}

/// Handle one request line against the running server's state.
#[must_use]
pub fn handle(state: &AppState, line: &str) -> String {
    let (command, argument) = line
        .trim_end()
        .split_once(' ')
        .unwrap_or((line.trim_end(), ""));
    let result = match command {
        "backup" => crate::backup::snapshot(&state.database(), Path::new(argument))
            .map(|()| format!("backup written to {argument}"))
            .map_err(|e| e.to_string()),
        "admin-init" => state
            .admin()
            .init(argument, SystemTime::now())
            .map(|()| "admin account created".to_owned())
            .map_err(|e: AdminError| e.to_string()),
        "admin-reset-password" => state
            .admin()
            .set_password(argument, SystemTime::now())
            .map(|()| "admin password reset; all sessions revoked".to_owned())
            .map_err(|e| e.to_string()),
        _ => Err(format!("unknown command `{command}`")),
    };
    match result {
        Ok(message) => format!("ok {message}"),
        Err(message) => format!("error {message}"),
    }
}

/// Send one request to a running server. `None` if no server is listening.
#[cfg(unix)]
#[must_use]
pub fn request(data_dir: &Path, line: &str) -> Option<Result<String, String>> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket_path(data_dir)).ok()?;
    let sent = writeln!(stream, "{line}").and_then(|()| stream.flush());
    if let Err(e) = sent {
        return Some(Err(e.to_string()));
    }
    let mut response = String::new();
    if let Err(e) = BufReader::new(stream).read_line(&mut response) {
        return Some(Err(e.to_string()));
    }
    let response = response.trim_end();
    Some(match response.split_once(' ') {
        Some(("ok", message)) => Ok(message.to_owned()),
        Some((_, message)) => Err(message.to_owned()),
        None => Err(format!("unexpected response `{response}`")),
    })
}

#[cfg(not(unix))]
#[must_use]
pub fn request(_data_dir: &Path, _line: &str) -> Option<Result<String, String>> {
    None
}

/// Start listening. Returns the accept-loop task.
///
/// # Errors
///
/// The socket cannot be created.
#[cfg(unix)]
pub fn listen(data_dir: &Path, state: AppState) -> std::io::Result<tokio::task::JoinHandle<()>> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let path = socket_path(data_dir);
    let _ = std::fs::remove_file(&path); // stale socket from a crash; the redb lock guarantees we own the dir
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let state = state.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if tokio::io::BufReader::new(read)
                    .read_line(&mut line)
                    .await
                    .is_err()
                {
                    return;
                }
                let reply = tokio::task::spawn_blocking(move || handle(&state, &line))
                    .await
                    .unwrap_or_else(|e| format!("error {e}"));
                let _ = write.write_all(format!("{reply}\n").as_bytes()).await;
            });
        }
    }))
}

/// No local channel on Windows yet (US-0042): nothing to listen on.
///
/// # Errors
///
/// None; the signature matches the Unix version.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub fn listen(_data_dir: &Path, _state: AppState) -> std::io::Result<tokio::task::JoinHandle<()>> {
    Ok(tokio::spawn(async {}))
}
