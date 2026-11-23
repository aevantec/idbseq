//! Health, readiness, metrics and the API contract.

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use idbseq_core::engine::MAX_PAGE_LIMIT;

use super::blocking;
use crate::dto::HealthDto;
use crate::envelope::{Envelope, Reply};
use crate::error::{ApiError, Code};
use crate::state::AppState;

/// Liveness: the process is up.
#[utoipa::path(get, path = "/healthz", tag = "health",
    responses((status = 200, description = "Up", body = Envelope<HealthDto>)))]
pub async fn healthz(State(state): State<AppState>) -> Reply<HealthDto> {
    Reply::ok(HealthDto::new("ok", &state))
}

/// Readiness: the store is open and the server is not shutting down.
#[utoipa::path(get, path = "/readyz", tag = "health",
    responses((status = 200, description = "Ready", body = Envelope<HealthDto>), (status = 503, description = "Not ready")))]
pub async fn readyz(State(state): State<AppState>) -> Response {
    if state.is_ready() {
        return Reply::ok(HealthDto::new("ready", &state)).into_response();
    }
    let body = Envelope::<()>::failure(Code::Internal, "not ready".into(), None);
    (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
}

/// Prometheus metrics (only routed when `metrics.enabled`).
pub async fn metrics(State(state): State<AppState>) -> Result<Response, ApiError> {
    let engine = state.engine().clone();
    let collections = blocking(move || {
        let mut all = Vec::new();
        let mut cursor = None;
        loop {
            let page = engine.list(Some(MAX_PAGE_LIMIT), cursor, None)?;
            all.extend(page.items);
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(all),
            }
        }
    })
    .await?;
    let text = state.metrics().render(&collections);
    Ok((
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        text,
    )
        .into_response())
}

/// The `OpenAPI` 3.1 contract.
pub async fn openapi() -> Response {
    Json(crate::openapi::document()).into_response()
}

/// Swagger UI (only routed when `swagger_ui` is on). Assets load from a CDN.
pub async fn docs() -> Html<&'static str> {
    Html(
        r##"<!doctype html><html><head><meta charset="utf-8"><title>idbseq API</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui.css"></head>
<body><div id="ui"></div><script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script>
<script>SwaggerUIBundle({url:"/v1/openapi.json",dom_id:"#ui"});</script></body></html>"##,
    )
}
