//! Command-line definition (clap).

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::error::exit_code_help;

/// Default per-request timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Command-line client for the idbseq ID sequence server.
///
/// The server URL and credential come from `--url` / `--key`, else
/// `IDBSEQ_URL` / `IDBSEQ_KEY`, else `~/.config/idbseq/config.toml`
/// (`url = "…"`, `key = "…"`). After `idbseq login`, the admin session stored
/// in the same directory is used unless a key is given explicitly.
#[derive(Debug, Parser)]
#[command(name = "idbseq", version, after_long_help = exit_code_help())]
pub struct Cli {
    /// Server base URL (default `http://127.0.0.1:7878`)
    #[arg(long, global = true, env = "IDBSEQ_URL", value_name = "URL")]
    pub url: Option<String>,

    /// API key (`idb_<id>_<secret>`) or admin session token
    #[arg(
        long,
        global = true,
        env = "IDBSEQ_KEY",
        hide_env_values = true,
        value_name = "KEY"
    )]
    pub key: Option<String>,

    /// Print the response `data` as JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,

    /// Per-request timeout in seconds
    #[arg(long, global = true, value_name = "SECONDS", default_value_t = DEFAULT_TIMEOUT_SECS)]
    pub timeout: u64,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List, create, inspect, update and delete collections
    #[command(subcommand)]
    Collections(CollectionsCommand),

    /// Consume the next ID (or `--count N` IDs) of a collection
    Next {
        /// Collection name
        name: String,
        /// Issue a batch of N IDs at once
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        count: Option<u32>,
        /// Make a retry return the original IDs instead of issuing new ones
        #[arg(long, value_name = "KEY")]
        idempotency_key: Option<String>,
    },

    /// Show the last issued ID and the next one, without consuming anything
    Last {
        /// Collection name
        name: String,
    },

    /// Reset a collection's counter (asks for confirmation)
    Reset {
        /// Collection name
        name: String,
        /// Next ID to issue [default: the collection's start]
        #[arg(long, allow_negative_numbers = true)]
        value: Option<i64>,
        /// Allow moving the counter backwards
        #[arg(long)]
        allow_rewind: bool,
        #[command(flatten)]
        confirm: Confirm,
    },

    /// Manage API keys (admin)
    #[command(subcommand)]
    Keys(KeysCommand),

    /// Log in as the admin and store the session token
    ///
    /// The password is read from the terminal without echo, or from the first
    /// line of standard input when it is not a terminal.
    Login {
        /// Admin user name
        #[arg(long, default_value = "admin")]
        username: String,
    },

    /// Revoke the stored admin session and remove it
    Logout,

    /// Show the audit log (admin), newest first
    Audit {
        #[command(flatten)]
        page: Page,
        /// Only entries for this collection
        #[arg(long)]
        collection: Option<String>,
        /// Only this action, e.g. `collection.reset`
        #[arg(long)]
        action: Option<String>,
    },
}

/// Pagination flags.
#[derive(Debug, Clone, Args)]
pub struct Page {
    /// Page size (1-500)
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=500))]
    pub limit: Option<u32>,
    /// Cursor from the previous page
    #[arg(long)]
    pub cursor: Option<String>,
}

/// Confirmation bypass for destructive commands.
#[derive(Debug, Clone, Copy, Args)]
pub struct Confirm {
    /// Do not ask for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DurabilityMode {
    /// Each issued ID is on disk before it is returned
    Strict,
    /// Reserve `--block-size` IDs on disk at a time
    Block,
}

/// Collection settings shared by `create` and `update`.
#[derive(Debug, Clone, Args)]
pub struct CollectionSettings {
    /// First ID
    #[arg(long, allow_negative_numbers = true)]
    pub start: Option<i64>,
    /// Increment between IDs (> 0)
    #[arg(long)]
    pub step: Option<i64>,
    /// Largest ID the collection may issue
    #[arg(long, allow_negative_numbers = true)]
    pub max: Option<i64>,
    /// Durability mode
    #[arg(long, value_enum)]
    pub durability: Option<DurabilityMode>,
    /// IDs reserved per disk write with `--durability block`
    #[arg(long, requires = "durability")]
    pub block_size: Option<u64>,
}

#[derive(Debug, Subcommand)]
pub enum CollectionsCommand {
    /// List collections
    List {
        #[command(flatten)]
        page: Page,
        /// Only names starting with this prefix
        #[arg(long)]
        prefix: Option<String>,
        /// Follow cursors and list every page
        #[arg(long, conflicts_with = "cursor")]
        all: bool,
    },
    /// Create a collection
    Create {
        /// Collection name: 1-64 chars of a-z, 0-9, `_`, `.`, `-`
        name: String,
        #[command(flatten)]
        settings: CollectionSettings,
        /// Only the admin may reset or delete it
        #[arg(long)]
        protected: bool,
    },
    /// Show a collection
    Info {
        /// Collection name
        name: String,
    },
    /// Change a collection's settings
    Update {
        /// Collection name
        name: String,
        #[command(flatten)]
        settings: CollectionSettings,
        /// Set or clear `protected` (admin only)
        #[arg(long, value_name = "true|false")]
        protected: Option<bool>,
    },
    /// Delete a collection and its counter (asks for confirmation)
    Delete {
        /// Collection name
        name: String,
        #[command(flatten)]
        confirm: Confirm,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    Read,
    Generate,
    Manage,
}

impl Scope {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Generate => "generate",
            Self::Manage => "manage",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum KeysCommand {
    /// Create an API key; the secret is shown once
    Create {
        /// A label for the key, e.g. the service using it
        name: String,
        /// Scopes, repeatable or comma-separated
        #[arg(long = "scope", value_enum, value_delimiter = ',', required = true)]
        scopes: Vec<Scope>,
        /// Allowed collections (exact names or `prefix*`); repeatable or comma-separated
        #[arg(long = "collection", value_delimiter = ',')]
        collections: Vec<String>,
    },
    /// List API keys (no secrets)
    List,
    /// Revoke an API key (asks for confirmation)
    Revoke {
        /// Key id, e.g. `k7f3a9c2`
        id: String,
        #[command(flatten)]
        confirm: Confirm,
    },
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_command_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn help_lists_the_exit_codes() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("Exit codes:"));
        assert!(help.contains("SEQUENCE_EXHAUSTED"));
    }

    #[test]
    fn scopes_render_as_wire_names() {
        let names: Vec<&str> = [Scope::Read, Scope::Generate, Scope::Manage]
            .into_iter()
            .map(Scope::as_str)
            .collect();
        assert_eq!(names, ["read", "generate", "manage"]);
    }
}
