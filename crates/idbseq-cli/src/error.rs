//! Errors and their process exit codes.

use std::fmt;

use serde_json::{Value, json};

/// Exit status of a successful run.
pub const EXIT_OK: u8 = 0;
/// Exit status for errors without a more specific code.
pub const EXIT_OTHER: u8 = 1;
/// Exit status for invalid arguments or configuration (also used by clap).
pub const EXIT_USAGE: u8 = 2;
/// Exit status when the server could not be reached.
pub const EXIT_NETWORK: u8 = 3;
/// Exit status when the user declined a confirmation prompt.
pub const EXIT_ABORTED: u8 = 12;

/// Exit status per API error code. Codes not listed exit with [`EXIT_OTHER`].
const API_EXIT_CODES: &[(&str, u8)] = &[
    ("UNAUTHORIZED", 4),
    ("FORBIDDEN", 5),
    ("PROTECTED_COLLECTION", 6),
    ("COLLECTION_NOT_FOUND", 7),
    ("NOT_FOUND", 7),
    ("COLLECTION_EXISTS", 8),
    ("SEQUENCE_EXHAUSTED", 9),
    ("VALIDATION_FAILED", 10),
    ("PAYLOAD_TOO_LARGE", 10),
    ("RATE_LIMITED", 11),
];

/// The documented exit-code table, rendered in `--help` and the README.
pub const EXIT_CODE_TABLE: &[(u8, &str)] = &[
    (EXIT_OK, "success"),
    (
        EXIT_OTHER,
        "other errors: INTERNAL, unknown error codes, unexpected responses, local I/O",
    ),
    (EXIT_USAGE, "invalid arguments or configuration"),
    (
        EXIT_NETWORK,
        "network error: server unreachable or timed out",
    ),
    (4, "UNAUTHORIZED"),
    (5, "FORBIDDEN"),
    (6, "PROTECTED_COLLECTION"),
    (7, "COLLECTION_NOT_FOUND, NOT_FOUND"),
    (8, "COLLECTION_EXISTS"),
    (9, "SEQUENCE_EXHAUSTED"),
    (10, "VALIDATION_FAILED, PAYLOAD_TOO_LARGE"),
    (11, "RATE_LIMITED"),
    (EXIT_ABORTED, "aborted: confirmation declined"),
];

/// Renders [`EXIT_CODE_TABLE`] for `--help`.
#[must_use]
pub fn exit_code_help() -> String {
    let rows: Vec<String> = EXIT_CODE_TABLE
        .iter()
        .map(|(code, meaning)| format!("  {code:>2}  {meaning}"))
        .collect();
    format!("Exit codes:\n{}", rows.join("\n"))
}

/// Everything that can make a command fail.
#[derive(Debug, Clone, PartialEq)]
pub enum CliError {
    /// The server answered with an error envelope.
    Api {
        code: String,
        message: String,
        status: u16,
        details: Value,
        request_id: Option<String>,
    },
    /// No response: connection failure, DNS, TLS or timeout.
    Network(String),
    /// A response that is not a valid idbseq envelope.
    Unexpected { status: u16, message: String },
    /// Invalid arguments or configuration.
    Usage(String),
    /// The user declined a confirmation prompt.
    Aborted,
    /// Local I/O failure (config files, terminal).
    Io(String),
}

impl CliError {
    /// Process exit status for this error.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Api { code, .. } => API_EXIT_CODES
                .iter()
                .find(|(known, _)| known == code)
                .map_or(EXIT_OTHER, |(_, exit)| *exit),
            Self::Network(_) => EXIT_NETWORK,
            Self::Usage(_) => EXIT_USAGE,
            Self::Aborted => EXIT_ABORTED,
            Self::Unexpected { .. } | Self::Io(_) => EXIT_OTHER,
        }
    }

    /// The error as JSON, printed to stderr with `--json`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Api {
                code,
                message,
                status,
                details,
                request_id,
            } => json!({
                "code": code, "message": message, "status": status,
                "details": details, "request_id": request_id,
            }),
            Self::Network(message) => json!({ "code": "NETWORK_ERROR", "message": message }),
            Self::Unexpected { status, message } => {
                json!({ "code": "UNEXPECTED_RESPONSE", "message": message, "status": status })
            }
            Self::Usage(message) => json!({ "code": "USAGE", "message": message }),
            Self::Aborted => json!({ "code": "ABORTED", "message": "aborted" }),
            Self::Io(message) => json!({ "code": "IO_ERROR", "message": message }),
        }
    }
}

fn field_problems(details: &Value) -> Vec<String> {
    details
        .get("fields")
        .and_then(Value::as_object)
        .map(|fields| {
            fields
                .iter()
                .map(|(field, problem)| {
                    let problem = problem
                        .as_str()
                        .map_or_else(|| problem.to_string(), str::to_owned);
                    format!("  {field}: {problem}")
                })
                .collect()
        })
        .unwrap_or_default()
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api {
                code,
                message,
                details,
                request_id,
                ..
            } => {
                write!(f, "{code}: {message}")?;
                if let Some(id) = request_id {
                    write!(f, " (request {id})")?;
                }
                for line in field_problems(details) {
                    write!(f, "\n{line}")?;
                }
                if code == "UNAUTHORIZED" {
                    write!(
                        f,
                        "\nhint: pass --key, set IDBSEQ_KEY, or run `idbseq login`"
                    )?;
                }
                Ok(())
            }
            Self::Network(message) => write!(f, "cannot reach the server: {message}"),
            Self::Unexpected { status, message } => {
                write!(f, "unexpected response (HTTP {status}): {message}")
            }
            Self::Usage(message) | Self::Io(message) => f.write_str(message),
            Self::Aborted => f.write_str("aborted"),
        }
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(code: &str) -> CliError {
        CliError::Api {
            code: code.into(),
            message: "m".into(),
            status: 400,
            details: Value::Null,
            request_id: None,
        }
    }

    #[test]
    fn maps_each_api_code_to_its_documented_exit_status() {
        assert_eq!(api("UNAUTHORIZED").exit_code(), 4);
        assert_eq!(api("FORBIDDEN").exit_code(), 5);
        assert_eq!(api("PROTECTED_COLLECTION").exit_code(), 6);
        assert_eq!(api("COLLECTION_NOT_FOUND").exit_code(), 7);
        assert_eq!(api("NOT_FOUND").exit_code(), 7);
        assert_eq!(api("COLLECTION_EXISTS").exit_code(), 8);
        assert_eq!(api("SEQUENCE_EXHAUSTED").exit_code(), 9);
        assert_eq!(api("VALIDATION_FAILED").exit_code(), 10);
        assert_eq!(api("PAYLOAD_TOO_LARGE").exit_code(), 10);
        assert_eq!(api("RATE_LIMITED").exit_code(), 11);
        assert_eq!(api("INTERNAL").exit_code(), EXIT_OTHER);
        assert_eq!(api("SOMETHING_NEW").exit_code(), EXIT_OTHER);
    }

    #[test]
    fn maps_local_failures_to_their_exit_status() {
        assert_eq!(CliError::Network("x".into()).exit_code(), EXIT_NETWORK);
        assert_eq!(CliError::Usage("x".into()).exit_code(), EXIT_USAGE);
        assert_eq!(CliError::Aborted.exit_code(), EXIT_ABORTED);
        assert_eq!(CliError::Io("x".into()).exit_code(), EXIT_OTHER);
        let unexpected = CliError::Unexpected {
            status: 502,
            message: "x".into(),
        };
        assert_eq!(unexpected.exit_code(), EXIT_OTHER);
    }

    #[test]
    fn every_api_exit_code_is_documented() {
        for (_, exit) in API_EXIT_CODES {
            assert!(EXIT_CODE_TABLE.iter().any(|(code, _)| code == exit));
        }
        assert!(exit_code_help().contains(" 6  PROTECTED_COLLECTION"));
    }

    #[test]
    fn displays_validation_fields_and_request_id() {
        // Arrange
        let error = CliError::Api {
            code: "VALIDATION_FAILED".into(),
            message: "invalid input".into(),
            status: 400,
            details: json!({ "fields": { "name": "too long", "step": 0 } }),
            request_id: Some("req-1".into()),
        };

        // Act
        let text = error.to_string();

        // Assert
        assert_eq!(
            text,
            "VALIDATION_FAILED: invalid input (request req-1)\n  name: too long\n  step: 0"
        );
    }

    #[test]
    fn hints_at_credentials_when_unauthorized() {
        assert!(api("UNAUTHORIZED").to_string().contains("idbseq login"));
    }

    #[test]
    fn renders_every_variant_as_json_with_a_code() {
        let errors = [
            api("FORBIDDEN"),
            CliError::Network("down".into()),
            CliError::Unexpected {
                status: 502,
                message: "html".into(),
            },
            CliError::Usage("bad".into()),
            CliError::Aborted,
            CliError::Io("disk".into()),
        ];
        let codes: Vec<Value> = errors.iter().map(|e| e.to_json()["code"].clone()).collect();
        assert_eq!(
            codes,
            [
                "FORBIDDEN",
                "NETWORK_ERROR",
                "UNEXPECTED_RESPONSE",
                "USAGE",
                "ABORTED",
                "IO_ERROR"
            ]
        );
        assert_eq!(CliError::Aborted.to_string(), "aborted");
        assert!(
            CliError::Network("down".into())
                .to_string()
                .contains("down")
        );
    }
}
