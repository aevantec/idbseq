//! API key management, admin only (US-0023).

use std::time::SystemTime;

use axum::extract::State;
use serde_json::json;

use super::blocking;
use crate::audit::{Action, Event};
use crate::auth::Principal;
use crate::auth::keys::{KeyError, NewKey};
use crate::dto::{CreateKeyBody, CreatedKeyDto, KeyDto};
use crate::envelope::{EmptyEnvelope, Envelope, Reply};
use crate::error::{ApiError, Code};
use crate::extract::{ApiJson, ApiPath, RequestContext};
use crate::state::AppState;

fn key_error(error: KeyError) -> ApiError {
    match error {
        KeyError::Invalid(errors) => idbseq_core::store::StoreError::Invalid(errors).into(),
        KeyError::NotFound(id) => {
            ApiError::new(Code::NotFound, format!("API key `{id}` not found"))
        }
        KeyError::Storage(e) => ApiError::internal(&e),
    }
}

fn key_event(
    action: Action,
    ctx: &RequestContext,
    previous: Option<&KeyDto>,
    new: Option<&KeyDto>,
) -> Event {
    Event {
        actor: "admin".into(),
        action,
        collection: None,
        previous: previous.map(|k| json!(k)),
        new: new.map(|k| json!(k)),
        request_id: ctx.request_id.clone(),
        client_ip: ctx.client_ip.clone(),
    }
}

/// List API keys (never with secrets).
#[utoipa::path(get, path = "/v1/keys", tag = "keys",
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Every key", body = Envelope<Vec<KeyDto>>)))]
pub async fn list(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Reply<Vec<KeyDto>>, ApiError> {
    principal.require_admin()?;
    Ok(Reply::ok(
        state.keys().list().iter().map(KeyDto::from).collect(),
    ))
}

/// Create an API key. The full key is in the response only.
#[utoipa::path(post, path = "/v1/keys", tag = "keys",
    request_body = CreateKeyBody,
    security(("bearerAuth" = [])),
    responses((status = 201, description = "Created; `key` is shown once", body = Envelope<CreatedKeyDto>)))]
pub async fn create(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiJson(body): ApiJson<CreateKeyBody>,
) -> Result<Reply<CreatedKeyDto>, ApiError> {
    principal.require_admin()?;
    let new = NewKey {
        name: body.name,
        scopes: body.scopes,
        collections: body.collections,
    };
    let worker = state.clone();
    let (record, secret) = blocking(move || {
        worker
            .keys()
            .create(new, SystemTime::now())
            .map_err(key_error)
    })
    .await?;
    let dto = KeyDto::from(&record);
    state.record(key_event(Action::KeyCreate, &ctx, None, Some(&dto)));
    Ok(Reply::created(CreatedKeyDto { key: dto, secret }))
}

/// Revoke an API key immediately.
#[utoipa::path(delete, path = "/v1/keys/{id}", tag = "keys",
    params(("id" = String, Path)),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Revoked", body = EmptyEnvelope), (status = 404, description = "NOT_FOUND")))]
pub async fn revoke(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiPath(id): ApiPath<String>,
) -> Result<Reply<()>, ApiError> {
    principal.require_admin()?;
    let worker = state.clone();
    let record = blocking(move || worker.keys().revoke(&id).map_err(key_error)).await?;
    state.record(key_event(
        Action::KeyRevoke,
        &ctx,
        Some(&KeyDto::from(&record)),
        None,
    ));
    Ok(Reply::ok(()))
}
