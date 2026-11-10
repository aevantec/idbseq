//! Extractors that turn malformed input into envelope errors.

use std::net::SocketAddr;

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::error::{ApiError, Code};

/// JSON body; bad JSON → `VALIDATION_FAILED`, too large → `PAYLOAD_TOO_LARGE`.
/// An empty body deserializes as `{}`.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        let empty = request
            .headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .is_some_and(|len| len == "0");
        if empty {
            return serde_json::from_str("{}")
                .map(Self)
                .map_err(|e| ApiError::new(Code::ValidationFailed, format!("invalid body: {e}")));
        }
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(&rejection)),
        }
    }
}

fn json_rejection(rejection: &JsonRejection) -> ApiError {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return ApiError::new(Code::PayloadTooLarge, "request body is too large");
    }
    ApiError::new(
        Code::ValidationFailed,
        format!("invalid body: {}", rejection.body_text()),
    )
}

/// Query string; bad values → `VALIDATION_FAILED`.
pub struct ApiQuery<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|q| Self(q.0))
            .map_err(|e: QueryRejection| {
                ApiError::new(
                    Code::ValidationFailed,
                    format!("invalid query: {}", e.body_text()),
                )
            })
    }
}

/// Path parameters; bad values → `VALIDATION_FAILED`.
pub struct ApiPath<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for ApiPath<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|p| Self(p.0))
            .map_err(|e: PathRejection| {
                ApiError::new(
                    Code::ValidationFailed,
                    format!("invalid path: {}", e.body_text()),
                )
            })
    }
}

/// Request ID and client IP, for the audit log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestContext {
    pub request_id: Option<String>,
    pub client_ip: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self::from_parts(parts)))
    }
}

impl RequestContext {
    fn from_parts(parts: &Parts) -> Self {
        let request_id = parts
            .headers
            .get(crate::app::REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let client_ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip().to_string());
        Self {
            request_id,
            client_ip,
        }
    }
}
