//! Command implementations. Each returns the response `data` (printed with
//! `--json`) and a human-readable rendering of it.

mod admin;
mod collections;
mod ids;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::cli::Command;
use crate::error::CliError;
use crate::http::{Client, encode};
use crate::prompt::Prompt;

/// What a successful command prints.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Printed as JSON with `--json`.
    pub data: Value,
    /// Printed otherwise.
    pub text: String,
}

impl Outcome {
    #[must_use]
    pub fn new(data: Value, text: impl Into<String>) -> Self {
        Self {
            data,
            text: text.into(),
        }
    }
}

/// Everything a command needs besides its arguments.
pub struct Context<'a> {
    pub client: Client,
    pub config_dir: Option<PathBuf>,
    pub timeout: Duration,
    pub prompt: &'a mut dyn Prompt,
}

/// Runs one command.
///
/// # Errors
///
/// Any [`CliError`] from the request, the prompt or local files.
pub fn dispatch(command: Command, ctx: &mut Context<'_>) -> Result<Outcome, CliError> {
    match command {
        Command::Collections(command) => collections::run(command, ctx),
        Command::Next {
            name,
            count,
            idempotency_key,
        } => ids::next(&ctx.client, &name, count, idempotency_key),
        Command::Last { name } => ids::last(&ctx.client, &name),
        Command::Reset {
            name,
            value,
            allow_rewind,
            confirm,
        } => ids::reset(ctx, &name, value, allow_rewind, confirm.yes),
        Command::Keys(command) => admin::keys(command, ctx),
        Command::Login { username } => admin::login(ctx, &username),
        Command::Logout => admin::logout(ctx),
        Command::Audit {
            page,
            collection,
            action,
        } => admin::audit(&ctx.client, &page, collection, action),
    }
}

/// `/v1/collections/{name}{suffix}` with the name percent-encoded.
fn collection_path(name: &str, suffix: &str) -> Result<String, CliError> {
    if name.is_empty() {
        return Err(CliError::Usage(
            "the collection name must not be empty".to_owned(),
        ));
    }
    Ok(format!("/v1/collections/{}{suffix}", encode(name)))
}

/// The data as a list, or an empty slice.
fn items(data: &Value) -> &[Value] {
    data.as_array().map_or(&[], Vec::as_slice)
}
