//! The response envelope used by every endpoint (US-0015).

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

use crate::error::Code;

/// `{ success, data, error, meta? }`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Envelope<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// The `error` member of a failed response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub code: Code,
    pub message: String,
    pub details: Option<Value>,
}

/// A success with no data: `{ "success": true, "data": null, "error": null }`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmptyEnvelope {
    pub success: bool,
    /// Always `null`.
    pub data: Option<Value>,
    pub error: Option<ErrorBody>,
}

/// Pagination metadata for lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct Meta {
    pub total: usize,
    pub limit: usize,
    pub next_cursor: Option<String>,
}

impl<T> Envelope<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            meta: None,
        }
    }

    pub fn page(data: T, meta: Meta) -> Self {
        Self {
            meta: Some(meta),
            ..Self::success(data)
        }
    }

    #[must_use]
    pub fn failure(code: Code, message: String, details: Option<Value>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ErrorBody {
                code,
                message,
                details,
            }),
            meta: None,
        }
    }
}

/// A successful response with a status code.
pub struct Reply<T>(pub StatusCode, pub Envelope<T>);

impl<T: Serialize> Reply<T> {
    pub fn ok(data: T) -> Self {
        Self(StatusCode::OK, Envelope::success(data))
    }

    pub fn created(data: T) -> Self {
        Self(StatusCode::CREATED, Envelope::success(data))
    }

    pub fn page(data: T, meta: Meta) -> Self {
        Self(StatusCode::OK, Envelope::page(data, meta))
    }
}

impl<T: Serialize> IntoResponse for Reply<T> {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}
