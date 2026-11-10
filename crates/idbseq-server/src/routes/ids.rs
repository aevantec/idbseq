//! ID endpoints: next, last, reset (US-0017, US-0018, US-0020).

use std::time::SystemTime;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use idbseq_core::engine::NextRequest;
use serde_json::json;

use super::{blocking, check_confirm};
use crate::audit::{Action, Event};
use crate::auth::{Principal, Scope};
use crate::dto::{BatchDto, IdDto, LastDto, NextQuery, ResetBody, ResetDto};
use crate::envelope::{Envelope, Reply};
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiPath, ApiQuery, RequestContext};
use crate::state::AppState;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";
const MAX_IDEMPOTENCY_KEY_LEN: usize = 255;

fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get(IDEMPOTENCY_HEADER) else {
        return Ok(None);
    };
    let key = value.to_str().unwrap_or_default();
    let visible = key.bytes().all(|b| b.is_ascii_graphic());
    if key.is_empty() || key.len() > MAX_IDEMPOTENCY_KEY_LEN || !visible {
        return Err(ApiError::field(
            "idempotency_key",
            "must be 1-255 visible ASCII characters",
        ));
    }
    Ok(Some(key.to_owned()))
}

/// Issue the next ID, or a batch with `?count=n`.
#[utoipa::path(post, path = "/v1/collections/{name}/next", tag = "ids",
    params(("name" = String, Path), NextQuery,
        ("Idempotency-Key" = Option<String>, Header, description = "Retry-safe key; a repeat returns the original IDs")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "`{ id }` without `count`; `{ first, last, count, step }` with it", body = Envelope<BatchDto>),
        (status = 409, description = "SEQUENCE_EXHAUSTED: nothing was issued")))]
pub async fn next(
    State(state): State<AppState>,
    principal: Principal,
    headers: HeaderMap,
    ApiPath(name): ApiPath<String>,
    ApiQuery(query): ApiQuery<NextQuery>,
) -> Result<Response, ApiError> {
    principal.require(Scope::Generate, &name)?;
    let scoped_key = idempotency_key(&headers)?.map(|key| format!("{}:{key}", principal.actor()));
    let request = NextRequest {
        name: name.clone(),
        count: query.count,
        idempotency_key: scoped_key,
    };
    let engine = state.engine().clone();
    let issued = blocking(move || Ok(engine.next(&request, SystemTime::now())?)).await?;
    if !issued.replayed {
        state.metrics().ids_issued(&name, issued.range.count.get());
    }
    Ok(match query.count {
        None => Reply::ok(IdDto {
            id: issued.range.first,
        })
        .into_response(),
        Some(_) => Reply::ok(BatchDto::from(issued.range)).into_response(),
    })
}

/// The last issued ID and a preview of the next one.
#[utoipa::path(get, path = "/v1/collections/{name}/last", tag = "ids",
    params(("name" = String, Path)),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Never changes the counter", body = Envelope<LastDto>)))]
pub async fn last(
    State(state): State<AppState>,
    principal: Principal,
    ApiPath(name): ApiPath<String>,
) -> Result<Reply<LastDto>, ApiError> {
    principal.require(Scope::Read, &name)?;
    let engine = state.engine().clone();
    let last = blocking(move || Ok(engine.last(&name)?)).await?;
    Ok(Reply::ok(LastDto {
        last_id: last.last_id,
        next_id: last.next_id,
    }))
}

/// Move the counter. Requires `confirm` equal to the name; backwards needs `allow_rewind`.
#[utoipa::path(post, path = "/v1/collections/{name}/reset", tag = "ids",
    params(("name" = String, Path)),
    request_body = ResetBody,
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Previous and new counter position", body = Envelope<ResetDto>)))]
pub async fn reset(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<ResetBody>,
) -> Result<Reply<ResetDto>, ApiError> {
    principal.require(Scope::Manage, &name)?;
    check_confirm(body.confirm.as_deref(), &name)?;
    let engine = state.engine().clone();
    let guard = principal.clone();
    let target = name.clone();
    let outcome = blocking(move || {
        guard.guard_protected(&engine.get(&target)?)?;
        Ok(engine.reset(&target, body.value, body.allow_rewind)?)
    })
    .await?;
    state.record(Event {
        actor: principal.actor(),
        action: Action::CollectionReset,
        collection: Some(name),
        previous: Some(json!({ "last_id": outcome.previous_last_id })),
        new: Some(json!({ "last_id": outcome.last_id, "next_id": outcome.next_id })),
        request_id: ctx.request_id,
        client_ip: ctx.client_ip,
    });
    Ok(Reply::ok(ResetDto::from(outcome)))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            IDEMPOTENCY_HEADER,
            HeaderValue::from_str(value).expect("header"),
        );
        headers
    }

    #[test]
    fn idempotency_keys_must_be_visible_ascii_up_to_255_bytes() {
        assert_eq!(idempotency_key(&HeaderMap::new()), Ok(None));
        assert_eq!(
            idempotency_key(&headers("abc-123")),
            Ok(Some("abc-123".into()))
        );
        assert!(idempotency_key(&headers("has space")).is_err());
        assert!(idempotency_key(&headers(&"x".repeat(256))).is_err());
    }
}
