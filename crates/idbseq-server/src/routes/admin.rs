//! Admin sign-in, password change and the audit log (US-0021, US-0022, US-0025).

use std::time::SystemTime;

use axum::extract::State;
use idbseq_core::engine::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

use super::blocking;
use crate::audit::{Action, Event, Filter};
use crate::auth::Principal;
use crate::auth::admin::AdminError;
use crate::clock::rfc3339;
use crate::dto::{AuditDto, AuditQuery, LoginBody, PasswordBody, SessionDto};
use crate::envelope::{EmptyEnvelope, Envelope, Meta, Reply};
use crate::error::{ApiError, Code};
use crate::extract::{ApiJson, ApiQuery, RequestContext};
use crate::state::AppState;

fn admin_error(error: &AdminError) -> ApiError {
    match error {
        AdminError::NotInitialized => ApiError::forbidden(error.to_string()),
        AdminError::InvalidCredentials => ApiError::new(Code::Unauthorized, error.to_string()),
        AdminError::WeakPassword => ApiError::field("new_password", &error.to_string()),
        AdminError::AlreadyInitialized | AdminError::Storage(_) | AdminError::Hash(_) => {
            ApiError::internal(&error)
        }
    }
}

fn admin_event(actor: &str, action: Action, ctx: &RequestContext) -> Event {
    Event {
        actor: actor.to_owned(),
        action,
        collection: None,
        previous: None,
        new: None,
        request_id: ctx.request_id.clone(),
        client_ip: ctx.client_ip.clone(),
    }
}

/// Sign in as the admin. Returns an opaque bearer token.
#[utoipa::path(post, path = "/v1/auth/login", tag = "auth",
    request_body = LoginBody,
    responses(
        (status = 200, description = "Session token", body = Envelope<SessionDto>),
        (status = 401, description = "UNAUTHORIZED: wrong username/password or locked account"),
        (status = 403, description = "FORBIDDEN: no admin exists yet; run `idbseqd admin init`")))]
pub async fn login(
    State(state): State<AppState>,
    ctx: RequestContext,
    ApiJson(body): ApiJson<LoginBody>,
) -> Result<Reply<SessionDto>, ApiError> {
    let worker = state.clone();
    let result = blocking(move || {
        Ok(worker
            .admin()
            .login(&body.username, &body.password, SystemTime::now()))
    })
    .await?;
    match result {
        Ok(session) => {
            state.record(admin_event("admin", Action::Login, &ctx));
            Ok(Reply::ok(SessionDto {
                token: session.token,
                expires_at: rfc3339(session.expires_at),
            }))
        }
        Err(error) => {
            if matches!(error, AdminError::InvalidCredentials) {
                state.metrics().auth_failure();
                state.record(admin_event("anonymous", Action::LoginFailed, &ctx));
            }
            Err(admin_error(&error))
        }
    }
}

/// Revoke the current admin session.
#[utoipa::path(post, path = "/v1/auth/logout", tag = "auth",
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Session revoked", body = EmptyEnvelope)))]
pub async fn logout(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Reply<()>, ApiError> {
    let Principal::Admin { session } = principal else {
        return Err(ApiError::forbidden("only an admin session can log out"));
    };
    let worker = state.clone();
    blocking(move || Ok(worker.admin().logout(&session)?)).await?;
    Ok(Reply::ok(()))
}

/// Change the admin password. Revokes every session.
#[utoipa::path(post, path = "/v1/admin/password", tag = "auth",
    request_body = PasswordBody,
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Password changed; sign in again", body = EmptyEnvelope)))]
pub async fn change_password(
    State(state): State<AppState>,
    principal: Principal,
    ctx: RequestContext,
    ApiJson(body): ApiJson<PasswordBody>,
) -> Result<Reply<()>, ApiError> {
    principal.require_admin()?;
    let worker = state.clone();
    blocking(move || {
        let admin = worker.admin();
        if !admin
            .verify_password(&body.current_password)
            .map_err(|e| admin_error(&e))?
        {
            return Err(ApiError::new(
                Code::Unauthorized,
                "current password is incorrect",
            ));
        }
        admin
            .set_password(&body.new_password, SystemTime::now())
            .map_err(|e| admin_error(&e))
    })
    .await?;
    state.record(admin_event("admin", Action::PasswordChange, &ctx));
    Ok(Reply::ok(()))
}

/// The audit log, newest first.
#[utoipa::path(get, path = "/v1/audit", tag = "audit",
    params(AuditQuery),
    security(("bearerAuth" = [])),
    responses((status = 200, description = "Audit entries", body = Envelope<Vec<AuditDto>>)))]
pub async fn audit(
    State(state): State<AppState>,
    principal: Principal,
    ApiQuery(query): ApiQuery<AuditQuery>,
) -> Result<Reply<Vec<AuditDto>>, ApiError> {
    principal.require_admin()?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(ApiError::field("limit", "must be between 1 and 500"));
    }
    let action = match query.action.as_deref() {
        None => None,
        Some(name) => {
            Some(Action::parse(name).ok_or_else(|| ApiError::field("action", "unknown action"))?)
        }
    };
    let filter = Filter {
        collection: query.collection,
        action,
    };
    let worker = state.clone();
    let page = blocking(move || Ok(worker.audit().list(&filter, limit, query.cursor)?)).await?;
    let meta = Meta {
        total: page.total,
        limit,
        next_cursor: page.next_cursor.map(|c| c.to_string()),
    };
    Ok(Reply::page(
        page.items.into_iter().map(AuditDto::from).collect(),
        meta,
    ))
}
