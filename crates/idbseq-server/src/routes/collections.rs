//! Collection management (US-0016, US-0018 delete, US-0024 protection).

use std::time::SystemTime;

use axum::body::Bytes;
use axum::extract::State;
use idbseq_core::collection::Collection;
use idbseq_core::engine::MAX_PAGE_LIMIT;
use serde_json::json;

use super::{blocking, check_confirm};
use crate::audit::{Action, Event};
use crate::auth::{Principal, Scope};
use crate::dto::{
    CollectionDto, ConfirmBody, CreateCollectionBody, ListQuery, PatchCollectionBody,
};
use crate::envelope::{Envelope, Meta, Reply};
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiPath, ApiQuery, RequestContext};
use crate::state::AppState;

fn event(principal: &Principal, ctx: &RequestContext, action: Action, name: &str) -> Event {
    Event {
        actor: principal.actor(),
        action,
        collection: Some(name.to_owned()),
        previous: None,
        new: None,
        request_id: ctx.request_id.clone(),
        client_ip: ctx.client_ip.clone(),
    }
}

fn snapshot(collection: &Collection) -> serde_json::Value {
    serde_json::to_value(CollectionDto::from(collection)).unwrap_or_default()
}

/// List collections.
#[utoipa::path(get, path = "/v1/collections", tag = "collections",
    params(ListQuery),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "A page of collections, sorted by name", body = Envelope<Vec<CollectionDto>>)))]
pub async fn list(
    State(state): State<AppState>,
    principal: Principal,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Reply<Vec<CollectionDto>>, ApiError> {
    principal.require_scope(Scope::Read)?;
    let engine = state.engine().clone();
    blocking(move || {
        let restricted = matches!(&principal, Principal::Key(k) if !k.collections.is_empty());
        let limit = query
            .limit
            .unwrap_or(idbseq_core::engine::DEFAULT_PAGE_LIMIT);
        if !restricted {
            let page = engine.list(Some(limit), query.cursor, query.prefix)?;
            let items = page.items.iter().map(CollectionDto::from).collect();
            let meta = Meta {
                total: page.total,
                limit,
                next_cursor: page.next_cursor,
            };
            return Ok(Reply::page(items, meta));
        }
        // An allow-listed key sees only its collections: page through all matches.
        let visible = visible_collections(&engine, &principal, query.prefix.as_deref())?;
        let total = visible.len();
        let mut after = visible
            .into_iter()
            .filter(|c| query.cursor.as_deref().is_none_or(|cur| c.name() > cur));
        let page: Vec<Collection> = after
            .by_ref()
            .take(limit.clamp(1, MAX_PAGE_LIMIT))
            .collect();
        let next_cursor = after.next().and(page.last()).map(|c| c.name().to_owned());
        let meta = Meta {
            total,
            limit,
            next_cursor,
        };
        Ok(Reply::page(
            page.iter().map(CollectionDto::from).collect(),
            meta,
        ))
    })
    .await
}

fn visible_collections(
    engine: &idbseq_core::engine::Engine,
    principal: &Principal,
    prefix: Option<&str>,
) -> Result<Vec<Collection>, ApiError> {
    let mut cursor = None;
    let mut visible = Vec::new();
    loop {
        let page = engine.list(Some(MAX_PAGE_LIMIT), cursor, prefix.map(str::to_owned))?;
        visible.extend(
            page.items
                .into_iter()
                .filter(|c| principal.can_see(c.name())),
        );
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(visible),
        }
    }
}

/// Create a collection.
#[utoipa::path(post, path = "/v1/collections", tag = "collections",
    request_body = CreateCollectionBody,
    security(("bearerAuth" = [])),
    responses((status = 201, description = "Created", body = Envelope<CollectionDto>)))]
pub async fn create(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiJson(body): ApiJson<CreateCollectionBody>,
) -> Result<Reply<CollectionDto>, ApiError> {
    principal.require(Scope::Manage, &body.name)?;
    let spec = body.into_spec()?;
    if spec.protected && !principal.is_admin() {
        return Err(ApiError::protected(&spec.name));
    }
    let engine = state.engine().clone();
    let created = blocking(move || Ok(engine.create(spec, SystemTime::now())?)).await?;
    state.record(Event {
        new: Some(snapshot(&created)),
        ..event(&principal, &ctx, Action::CollectionCreate, created.name())
    });
    Ok(Reply::created(CollectionDto::from(&created)))
}

/// Get one collection.
#[utoipa::path(get, path = "/v1/collections/{name}", tag = "collections",
    params(("name" = String, Path)),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "The collection", body = Envelope<CollectionDto>)))]
pub async fn get(
    State(state): State<AppState>,
    principal: Principal,
    ApiPath(name): ApiPath<String>,
) -> Result<Reply<CollectionDto>, ApiError> {
    principal.require(Scope::Read, &name)?;
    let engine = state.engine().clone();
    let collection = blocking(move || Ok(engine.get(&name)?)).await?;
    Ok(Reply::ok(CollectionDto::from(&collection)))
}

/// Change `max`, `protected`, `durability`, or (before the first ID) `start`/`step`.
#[utoipa::path(patch, path = "/v1/collections/{name}", tag = "collections",
    params(("name" = String, Path)),
    request_body = PatchCollectionBody,
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Updated", body = Envelope<CollectionDto>)))]
pub async fn update(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<PatchCollectionBody>,
) -> Result<Reply<CollectionDto>, ApiError> {
    principal.require(Scope::Manage, &name)?;
    let changes_protection = body.protected.is_some();
    let patch = body.into_patch()?;
    let engine = state.engine().clone();
    let guard = principal.clone();
    let target = name.clone();
    let updated = blocking(move || {
        let current = engine.get(&target)?;
        if changes_protection && !guard.is_admin() {
            return Err(ApiError::protected(&target));
        }
        guard.guard_protected(&current)?;
        Ok(engine.update(&target, &patch)?)
    })
    .await?;
    state.record(Event {
        previous: Some(snapshot(&updated.before)),
        new: Some(snapshot(&updated.after)),
        ..event(&principal, &ctx, Action::CollectionUpdate, &name)
    });
    Ok(Reply::ok(CollectionDto::from(&updated.after)))
}

/// Delete a collection and its counter. Requires `confirm` (query or body) equal to the name.
#[utoipa::path(delete, path = "/v1/collections/{name}", tag = "collections",
    params(("name" = String, Path), ConfirmBody),
    request_body(content = Option<ConfirmBody>, description = "Alternative to `?confirm=`"),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "The collection as it was", body = Envelope<CollectionDto>)))]
pub async fn delete(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiPath(name): ApiPath<String>,
    ApiQuery(query): ApiQuery<ConfirmBody>,
    body: Bytes,
) -> Result<Reply<CollectionDto>, ApiError> {
    principal.require(Scope::Manage, &name)?;
    let from_body: ConfirmBody = if body.is_empty() {
        ConfirmBody::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| ApiError::field("confirm", &format!("invalid body: {e}")))?
    };
    check_confirm(query.confirm.or(from_body.confirm).as_deref(), &name)?;
    let engine = state.engine().clone();
    let guard = principal.clone();
    let target = name.clone();
    let deleted = blocking(move || {
        guard.guard_protected(&engine.get(&target)?)?;
        Ok(engine.delete(&target)?)
    })
    .await?;
    state.record(Event {
        previous: Some(snapshot(&deleted)),
        new: Some(json!(null)),
        ..event(&principal, &ctx, Action::CollectionDelete, &name)
    });
    Ok(Reply::ok(CollectionDto::from(&deleted)))
}
