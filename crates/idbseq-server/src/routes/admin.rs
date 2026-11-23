//! Admin sign-in, password change and the audit log (US-0021, US-0022, US-0025).

use std::time::SystemTime;

use axum::extract::State;
use idbseq_core::engine::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

use super::blocking;
use crate::audit::{Action, Event, Filter};
use crate::auth::Principal;
use crate::auth::admin::{ADMIN_USERNAME, AdminError};
use crate::auth::lockout::client_key;
use crate::clock::rfc3339;
use crate::dto::{
    AuditDto, AuditQuery, LoginBody, PasswordBody, SessionDto, SessionExpiryDto, SetupBody,
};
use crate::envelope::{EmptyEnvelope, Envelope, Meta, Reply};
use crate::error::{ApiError, Code};
use crate::extract::{ApiJson, ApiQuery, RequestContext};
use crate::state::AppState;

fn admin_error(error: &AdminError) -> ApiError {
    match error {
        AdminError::NotInitialized => ApiError::forbidden(error.to_string()),
        AdminError::InvalidCredentials | AdminError::Locked => {
            ApiError::new(Code::Unauthorized, error.to_string())
        }
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
    let client = client_key(ctx.client_ip.as_deref());
    let _turn = state.password_check().await?;
    let worker = state.clone();
    let result = blocking(move || {
        Ok(worker
            .admin()
            .login(&client, &body.username, &body.password, SystemTime::now()))
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
            match error {
                AdminError::InvalidCredentials => {
                    state.metrics().auth_failure();
                    state.record(admin_event("anonymous", Action::LoginFailed, &ctx));
                }
                // Refused while locked out: counted, not audited, so a flood of
                // them cannot grow the audit log.
                AdminError::Locked => state.metrics().auth_failure(),
                _ => {}
            }
            Err(admin_error(&error))
        }
    }
}

/// Create the admin from the one-time link `idbseqd` prints on a first start with
/// the console on, then sign in. Served only when the console is on.
#[utoipa::path(post, path = "/v1/auth/setup", tag = "auth",
    request_body = SetupBody,
    responses(
        (status = 200, description = "Admin created; session token", body = Envelope<SessionDto>),
        (status = 400, description = "VALIDATION_FAILED: the password is too short"),
        (status = 401, description = "UNAUTHORIZED: the setup link is invalid, used or expired"),
        (status = 403, description = "FORBIDDEN: an admin account already exists"),
        (status = 404, description = "NOT_FOUND: the console is off")))]
pub async fn setup(
    State(state): State<AppState>,
    ctx: RequestContext,
    ApiJson(body): ApiJson<SetupBody>,
) -> Result<Reply<SessionDto>, ApiError> {
    let client = client_key(ctx.client_ip.as_deref());
    let _turn = state.password_check().await?;
    let worker = state.clone();
    let session = blocking(move || {
        let now = SystemTime::now();
        let admin = worker.admin();
        let created = worker
            .setup()
            .complete(&body.token, now, || {
                admin.init(&body.password, now)?;
                admin.login(&client, ADMIN_USERNAME, &body.password, now)
            })
            .map_err(|error| ApiError::new(Code::Unauthorized, error.to_string()))?;
        created.map_err(|error| {
            if matches!(error, AdminError::AlreadyInitialized) {
                worker.setup().cancel();
            }
            setup_error(&error)
        })
    })
    .await?;
    state.record(admin_event("admin", Action::AdminInit, &ctx));
    state.record(admin_event("admin", Action::Login, &ctx));
    Ok(Reply::ok(SessionDto {
        token: session.token,
        expires_at: rfc3339(session.expires_at),
    }))
}

fn setup_error(error: &AdminError) -> ApiError {
    match error {
        AdminError::AlreadyInitialized => {
            ApiError::forbidden("an admin account already exists; sign in instead")
        }
        AdminError::WeakPassword => ApiError::field("password", &error.to_string()),
        other => admin_error(other),
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

/// Keep the current admin session alive: it now expires a full session TTL
/// from now, but never more than 12 hours after sign-in. The token is unchanged.
#[utoipa::path(post, path = "/v1/auth/refresh", tag = "auth",
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "New expiry", body = Envelope<SessionExpiryDto>),
        (status = 401, description = "UNAUTHORIZED: the session has ended"),
        (status = 403, description = "FORBIDDEN: API keys have no session to refresh")))]
pub async fn refresh(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Reply<SessionExpiryDto>, ApiError> {
    let Principal::Admin { session } = principal else {
        return Err(ApiError::forbidden(
            "only an admin session can be refreshed",
        ));
    };
    let worker = state.clone();
    let expires_at = blocking(move || Ok(worker.admin().refresh(&session, SystemTime::now())?))
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Reply::ok(SessionExpiryDto {
        expires_at: rfc3339(expires_at),
    }))
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
    let _turn = state.password_check().await?;
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
