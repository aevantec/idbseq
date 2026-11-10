//! HTTP handlers.
#![allow(
    clippy::missing_errors_doc,
    reason = "handler docs become OpenAPI descriptions; the error codes are documented in docs/API.md"
)]

pub mod admin;
pub mod collections;
pub mod health;
pub mod ids;
pub mod keys;

use crate::error::ApiError;

/// Run blocking work (storage, password hashing) off the async runtime.
///
/// # Errors
///
/// Whatever `work` returns, or `INTERNAL` if the task panicked.
pub async fn blocking<T, F>(work: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| ApiError::internal(&e))?
}

/// Fail unless `confirm` equals `name`.
///
/// # Errors
///
/// `VALIDATION_FAILED` on the `confirm` field.
pub fn check_confirm(confirm: Option<&str>, name: &str) -> Result<(), ApiError> {
    if confirm == Some(name) {
        Ok(())
    } else {
        Err(ApiError::field("confirm", "must equal the collection name"))
    }
}
