//! `idbseq`: command-line client for the idbseq HTTP API (US-0027).
//!
//! The binary is a thin wrapper around [`run`], which tests call in-process.

pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod format;
pub mod http;
pub mod prompt;

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use cli::Cli;
use commands::{Context, Outcome, dispatch};
use config::{Config, load_config, load_session};
use error::{CliError, EXIT_OK, EXIT_OTHER};
use http::{Client, normalise_base_url};
use prompt::Prompt;

/// Server used when neither flags, environment nor config name one.
pub const DEFAULT_URL: &str = "http://127.0.0.1:7878";

/// Server URL and credential after applying flags, environment and config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub url: String,
    pub credential: Option<String>,
    pub config_dir: Option<PathBuf>,
}

/// Precedence: flag or environment (clap), then the stored admin session for
/// the same server (credential only), then `config.toml`, then the default URL.
///
/// # Errors
///
/// [`CliError::Usage`] for an invalid URL or config file, [`CliError::Io`] when
/// the config directory cannot be read.
pub fn resolve_settings(
    cli: &Cli,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Settings, CliError> {
    let config_dir = config::config_dir(env);
    let config = config_dir
        .as_deref()
        .map(load_config)
        .transpose()?
        .unwrap_or_default();
    let Config {
        url: config_url,
        key: config_key,
    } = config;
    let url = normalise_base_url(
        cli.url
            .as_deref()
            .or(config_url.as_deref())
            .unwrap_or(DEFAULT_URL),
    )?;
    let session_token = match (&cli.key, config_dir.as_deref()) {
        (None, Some(dir)) => load_session(dir)?
            .filter(|session| normalise_base_url(&session.url).is_ok_and(|stored| stored == url))
            .map(|session| session.token),
        _ => None,
    };
    let credential = cli.key.clone().or(session_token).or(config_key);
    Ok(Settings {
        url,
        credential,
        config_dir,
    })
}

fn execute(
    cli: Cli,
    env: &dyn Fn(&str) -> Option<String>,
    prompt: &mut dyn Prompt,
) -> Result<Outcome, CliError> {
    let settings = resolve_settings(&cli, env)?;
    let timeout = Duration::from_secs(cli.timeout.max(1));
    let mut ctx = Context {
        client: Client::new(&settings.url, settings.credential, timeout)?,
        config_dir: settings.config_dir,
        timeout,
        prompt,
    };
    dispatch(cli.command, &mut ctx)
}

fn print(out: &mut dyn Write, json: bool, outcome: &Outcome) -> std::io::Result<()> {
    if json {
        let text = serde_json::to_string_pretty(&outcome.data).map_err(std::io::Error::other)?;
        writeln!(out, "{text}")
    } else if outcome.text.is_empty() {
        Ok(())
    } else {
        writeln!(out, "{}", outcome.text)
    }
}

fn print_error(err: &mut dyn Write, json: bool, error: &CliError) -> std::io::Result<()> {
    if json {
        writeln!(err, "{}", error.to_json())
    } else {
        writeln!(err, "error: {error}")
    }
}

/// Runs a parsed command line and returns the process exit status.
pub fn run(
    cli: Cli,
    env: &dyn Fn(&str) -> Option<String>,
    prompt: &mut dyn Prompt,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let json = cli.json;
    match execute(cli, env, prompt) {
        Ok(outcome) => match print(out, json, &outcome) {
            Ok(()) => EXIT_OK,
            Err(_) => EXIT_OTHER,
        },
        Err(error) => {
            // If even stderr is gone there is nothing left to report to.
            let _ = print_error(err, json, &error);
            error.exit_code()
        }
    }
}
