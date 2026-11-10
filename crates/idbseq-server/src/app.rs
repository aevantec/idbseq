//! The router and its middleware: request IDs, tracing, metrics, security
//! headers, timeouts, body limits, panic safety, and envelopes for every error.

use std::any::Any;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::error::{ApiError, Code};
use crate::routes::{admin, collections, health, ids, keys};
use crate::state::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

const SECURITY_HEADERS: [(&str, &str); 4] = [
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    ("cache-control", "no-store"),
];

/// Build the full application.
pub fn router(state: AppState) -> Router {
    let config = state.config().clone();
    let api = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/v1/openapi.json", get(health::openapi))
        .route("/v1/auth/login", post(admin::login))
        .route("/v1/auth/logout", post(admin::logout))
        .route("/v1/admin/password", post(admin::change_password))
        .route("/v1/audit", get(admin::audit))
        .route("/v1/keys", get(keys::list).post(keys::create))
        .route("/v1/keys/{id}", axum::routing::delete(keys::revoke))
        .route(
            "/v1/collections",
            get(collections::list).post(collections::create),
        )
        .route(
            "/v1/collections/{name}",
            get(collections::get)
                .patch(collections::update)
                .delete(collections::delete),
        )
        .route("/v1/collections/{name}/next", post(ids::next))
        .route("/v1/collections/{name}/last", get(ids::last))
        .route("/v1/collections/{name}/reset", post(ids::reset));
    let api = if config.metrics.enabled {
        api.route("/metrics", get(health::metrics))
    } else {
        api
    };
    let api = if config.swagger_ui {
        api.route("/docs", get(health::docs))
    } else {
        api
    };

    let request_id = HeaderName::from_static(REQUEST_ID_HEADER);
    api.fallback(not_found)
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(request_id.clone(), MakeRequestUuid))
                .layer(TraceLayer::new_for_http().make_span_with(make_span))
                .layer(PropagateRequestIdLayer::new(request_id))
                .layer(header_layer(0))
                .layer(header_layer(1))
                .layer(header_layer(2))
                .layer(header_layer(3))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    record_metrics,
                ))
                .layer(middleware::from_fn(envelope_errors))
                .layer(CatchPanicLayer::custom(panic_response))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    config.limits.request_timeout(),
                ))
                .layer(DefaultBodyLimit::max(config.limits.body_bytes)),
        )
        .with_state(state)
}

fn header_layer(index: usize) -> SetResponseHeaderLayer<HeaderValue> {
    let (name, value) = SECURITY_HEADERS[index];
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    )
}

fn make_span(request: &Request<Body>) -> tracing::Span {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    tracing::info_span!("request", method = %request.method(), path = %request.uri().path(), request_id)
}

async fn not_found() -> ApiError {
    ApiError::new(Code::NotFound, "no such endpoint")
}

async fn record_metrics(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let response = next.run(request).await;
    state
        .metrics()
        .request(response.status().as_u16(), started.elapsed().as_secs_f64());
    response
}

/// Wrap error responses produced outside our handlers (405, 408, 413, …) in the envelope.
async fn envelope_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"));
    if !(status.is_client_error() || status.is_server_error()) || is_json {
        return response;
    }
    let error = match status {
        StatusCode::METHOD_NOT_ALLOWED => {
            ApiError::new(Code::NotFound, "method not allowed for this endpoint")
        }
        StatusCode::REQUEST_TIMEOUT => ApiError::new(Code::RequestTimeout, "request timed out"),
        StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError::new(Code::PayloadTooLarge, "request body is too large")
        }
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiError::new(
            Code::ValidationFailed,
            "expected Content-Type: application/json",
        ),
        s if s.is_server_error() => ApiError::internal(&format!("unexpected {s} response")),
        _ => ApiError::new(Code::ValidationFailed, "bad request"),
    };
    let mut wrapped = error.into_response();
    *wrapped.status_mut() = status;
    wrapped
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "CatchPanicLayer::custom hands over the boxed panic"
)]
fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response {
    let detail = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    ApiError::internal(&format!("handler panicked: {detail}")).into_response()
}

/// Default for the graceful-shutdown grace period.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
