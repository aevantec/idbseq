//! `idbseqd` command line.

use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use clap::{Parser, Subcommand};

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(name = "idbseqd", version, about = "idbseq ID sequence server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the server (the default).
    Serve,
    /// Manage the admin account (needs local access to the data directory).
    Admin {
        #[command(subcommand)]
        action: AdminCommand,
    },
    /// Write a consistent snapshot of the data store to a new file (works while running).
    Backup { path: PathBuf },
    /// Replace the data store with a backup. The server must be stopped.
    Restore {
        path: PathBuf,
        /// Restore even if counters would move backwards (then skip ahead with `reset --value`).
        #[arg(long)]
        force: bool,
    },
    /// Print the `OpenAPI` contract, or check a committed copy for drift.
    Openapi {
        #[arg(long)]
        check: Option<PathBuf>,
    },
    /// Exit 0 if `GET /healthz` on the configured address answers 200 (for container health checks).
    Healthcheck,
}

#[derive(Clone, Copy, Debug, Subcommand)]
pub enum AdminCommand {
    /// Create the admin account (prompts for the password).
    Init {
        /// Read the password from the first line of stdin instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Set a new admin password and revoke all sessions (local recovery).
    ResetPassword {
        #[arg(long)]
        password_stdin: bool,
    },
}

/// A command's failure, with the process exit code to use.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Run a non-serve command. Returns the message to print on success.
///
/// # Errors
///
/// The command failed; see [`Failure`].
pub fn run(command: Command, config: &Config) -> Result<String, Failure> {
    match command {
        Command::Serve => Err(Failure::new(2, "serve is handled by main")),
        Command::Admin { action } => admin(action, config),
        Command::Backup { path } => backup(config, &path),
        Command::Restore { path, force } => crate::backup::restore(&config.data_dir, &path, force)
            .map(|kept| match kept {
                Some(old) => format!(
                    "restored {}; previous data kept at {}",
                    path.display(),
                    old.display()
                ),
                None => format!("restored {}", path.display()),
            })
            .map_err(|e| Failure::new(1, e.to_string())),
        Command::Openapi { check } => openapi(check.as_deref()),
        Command::Healthcheck => healthcheck(config.bind),
    }
}

fn read_password(from_stdin: bool) -> Result<String, Failure> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| Failure::new(1, e.to_string()))?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
    }
    let first = rpassword::prompt_password("New admin password: ")
        .map_err(|e| Failure::new(1, e.to_string()))?;
    let second = rpassword::prompt_password("Repeat password: ")
        .map_err(|e| Failure::new(1, e.to_string()))?;
    if first != second {
        return Err(Failure::new(1, "passwords do not match"));
    }
    Ok(first)
}

fn admin(action: AdminCommand, config: &Config) -> Result<String, Failure> {
    let (command, from_stdin, reset) = match action {
        AdminCommand::Init { password_stdin } => ("admin-init", password_stdin, false),
        AdminCommand::ResetPassword { password_stdin } => {
            ("admin-reset-password", password_stdin, true)
        }
    };
    let password = read_password(from_stdin)?;
    if password.contains('\n') {
        return Err(Failure::new(1, "password must be a single line"));
    }
    if let Some(reply) = crate::local::request(&config.data_dir, &format!("{command} {password}")) {
        return reply.map_err(|m| Failure::new(1, m));
    }
    let (state, _store) =
        crate::serve::open_state(config, None).map_err(|e| Failure::new(1, e.to_string()))?;
    let result = if reset {
        state.admin().set_password(&password, SystemTime::now())
    } else {
        state.admin().init(&password, SystemTime::now())
    };
    result
        .map(|()| {
            if reset {
                "admin password reset; all sessions revoked"
            } else {
                "admin account created"
            }
            .to_owned()
        })
        .map_err(|e| Failure::new(1, e.to_string()))
}

fn backup(config: &Config, path: &Path) -> Result<String, Failure> {
    let absolute = std::path::absolute(path).map_err(|e| Failure::new(1, e.to_string()))?;
    let line = format!("backup {}", absolute.display());
    if let Some(reply) = crate::local::request(&config.data_dir, &line) {
        return reply.map_err(|m| Failure::new(1, m));
    }
    let (state, _store) =
        crate::serve::open_state(config, None).map_err(|e| Failure::new(1, e.to_string()))?;
    crate::backup::snapshot(&state.database(), &absolute)
        .map(|()| format!("backup written to {}", absolute.display()))
        .map_err(|e| Failure::new(1, e.to_string()))
}

fn openapi(check: Option<&Path>) -> Result<String, Failure> {
    let generated = crate::openapi::pretty_json();
    let Some(path) = check else {
        return Ok(generated.trim_end().to_owned());
    };
    let committed = std::fs::read_to_string(path)
        .map_err(|e| Failure::new(1, format!("{}: {e}", path.display())))?;
    if committed == generated {
        Ok(format!("{} matches the generated contract", path.display()))
    } else {
        Err(Failure::new(
            1,
            format!(
                "{} is out of date; regenerate with `idbseqd openapi > {}`",
                path.display(),
                path.display()
            ),
        ))
    }
}

fn healthcheck(bind: SocketAddr) -> Result<String, Failure> {
    let target = if bind.ip().is_unspecified() {
        SocketAddr::from(([127, 0, 0, 1], bind.port()))
    } else {
        bind
    };
    let unhealthy = |e: &dyn std::fmt::Display| Failure::new(1, format!("unhealthy: {e}"));
    let mut stream =
        TcpStream::connect_timeout(&target, Duration::from_secs(3)).map_err(|e| unhealthy(&e))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| unhealthy(&e))?;
    stream
        .write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .map_err(|e| unhealthy(&e))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| unhealthy(&e))?;
    let status = response.split_whitespace().nth(1).unwrap_or_default();
    if status == "200" {
        Ok("healthy".into())
    } else {
        Err(unhealthy(&format!("status {status}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthcheck_fails_when_nothing_listens() {
        let free = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = free.local_addr().expect("addr");
        drop(free);

        let result = healthcheck(addr);

        assert!(result.is_err());
    }

    #[test]
    fn openapi_check_detects_drift() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("openapi.json");
        std::fs::write(&file, crate::openapi::pretty_json()).expect("write");
        let stale = dir.path().join("stale.json");
        std::fs::write(&stale, "{}").expect("write");

        assert!(openapi(Some(&file)).is_ok());
        assert_eq!(openapi(Some(&stale)).map_err(|f| f.code), Err(1));
        assert!(openapi(None).expect("print").starts_with('{'));
    }
}
