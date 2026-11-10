//! API errors with stable codes, rendered in the response envelope (US-0015).

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use idbseq_core::store::StoreError;
use serde_json::{Value, json};

use crate::envelope::Envelope;

/// Stable error codes (see `docs/API.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Code {
    ValidationFailed,
    Unauthorized,
    Forbidden,
    ProtectedCollection,
    NotFound,
    CollectionNotFound,
    CollectionExists,
    SequenceExhausted,
    RequestTimeout,
    PayloadTooLarge,
    RateLimited,
    Internal,
}

impl Code {
    #[must_use]
    pub fn status(self) -> StatusCode {
        match self {
            Self::ValidationFailed => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::ProtectedCollection => StatusCode::FORBIDDEN,
            Self::NotFound | Self::CollectionNotFound => StatusCode::NOT_FOUND,
            Self::CollectionExists | Self::SequenceExhausted => StatusCode::CONFLICT,
            Self::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// An error returned to the client. Internal details never leave the server.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub code: Code,
    pub message: String,
    pub details: Option<Value>,
    pub retry_after_secs: Option<u64>,
}

impl ApiError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            retry_after_secs: None,
        }
    }

    #[must_use]
    pub fn with_details(self, details: Value) -> Self {
        Self {
            details: Some(details),
            ..self
        }
    }

    /// One failing field.
    #[must_use]
    pub fn field(field: &str, problem: &str) -> Self {
        Self::new(Code::ValidationFailed, format!("{field}: {problem}"))
            .with_details(json!({ "fields": { field: problem } }))
    }

    #[must_use]
    pub fn unauthorized() -> Self {
        Self::new(Code::Unauthorized, "missing or invalid credentials")
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(Code::Forbidden, message)
    }

    #[must_use]
    pub fn protected(name: &str) -> Self {
        Self::new(
            Code::ProtectedCollection,
            format!("collection `{name}` is protected: this operation needs the admin"),
        )
    }

    /// Log `detail` server-side and return a generic message to the client.
    pub fn internal(detail: &dyn std::fmt::Display) -> Self {
        tracing::error!(error = %detail, "internal error");
        Self::new(Code::Internal, "internal server error")
    }

    #[must_use]
    pub fn rate_limited(retry_after_secs: u64) -> Self {
        Self {
            retry_after_secs: Some(retry_after_secs.max(1)),
            ..Self::new(Code::RateLimited, "too many requests")
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(name) => Self::new(
                Code::CollectionNotFound,
                format!("collection `{name}` not found"),
            ),
            StoreError::AlreadyExists(name) => Self::new(
                Code::CollectionExists,
                format!("collection `{name}` already exists"),
            ),
            StoreError::Exhausted(name) => Self::new(
                Code::SequenceExhausted,
                format!("collection `{name}` is exhausted"),
            ),
            StoreError::IdempotencyMismatch => Self::field(
                "idempotency_key",
                "was already used for a different request",
            ),
            StoreError::Invalid(errors) => Self::new(Code::ValidationFailed, errors.to_string())
                .with_details(json!({ "fields": errors.fields() })),
            StoreError::Backend(detail) => Self::internal(&detail),
        }
    }
}

impl From<idbseq_core::validation::ValidationErrors> for ApiError {
    fn from(errors: idbseq_core::validation::ValidationErrors) -> Self {
        StoreError::Invalid(errors).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.code.status();
        let retry_after = self.retry_after_secs;
        let body = Envelope::<()>::failure(self.code, self.message, self.details);
        let mut response = (status, axum::Json(body)).into_response();
        if let Some(secs) = retry_after
            && let Ok(value) = HeaderValue::from_str(&secs.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_maps_to_its_documented_status() {
        let cases = [
            (Code::ValidationFailed, 400),
            (Code::Unauthorized, 401),
            (Code::Forbidden, 403),
            (Code::ProtectedCollection, 403),
            (Code::NotFound, 404),
            (Code::CollectionNotFound, 404),
            (Code::CollectionExists, 409),
            (Code::SequenceExhausted, 409),
            (Code::RequestTimeout, 408),
            (Code::PayloadTooLarge, 413),
            (Code::RateLimited, 429),
            (Code::Internal, 500),
        ];
        for (code, status) in cases {
            assert_eq!(code.status().as_u16(), status, "{code:?}");
        }
    }

    #[test]
    fn backend_errors_hide_their_detail() {
        let error = ApiError::from(StoreError::Backend("disk on fire at /secret/path".into()));

        assert_eq!(error.code, Code::Internal);
        assert_eq!(error.message, "internal server error");
    }

    #[test]
    fn rate_limited_errors_carry_retry_after() {
        let response = ApiError::rate_limited(0).into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
    }
}
