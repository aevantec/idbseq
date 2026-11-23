//! Authentication and authorization (US-0021 – US-0024).
//!
//! Every `/v1` call except login carries `Authorization: Bearer <token>`: an API
//! key `idb_<id>_<secret>` or an admin session token. The [`Principal`] extractor
//! resolves it, applies the per-credential rate limit, and handlers check scopes,
//! allow-lists and collection protection.

pub mod admin;
pub mod keys;
pub mod lockout;
pub mod secrets;
pub mod setup;

use std::time::SystemTime;

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use idbseq_core::collection::Collection;

use crate::error::ApiError;
use crate::state::AppState;
pub use keys::{KeyRecord, Scope};

/// Who is calling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// The admin, via a session (identified by the token's hash).
    Admin { session: String },
    /// An application, via an API key.
    Key(KeyRecord),
}

impl Principal {
    /// `"admin"` or `"key:<id>"`, for the audit log and idempotency scoping.
    #[must_use]
    pub fn actor(&self) -> String {
        match self {
            Self::Admin { .. } => "admin".to_owned(),
            Self::Key(key) => format!("key:{}", key.id),
        }
    }

    #[must_use]
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin { .. })
    }

    /// Fail unless the caller is the admin.
    ///
    /// # Errors
    ///
    /// `FORBIDDEN`.
    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden("this operation needs the admin"))
        }
    }

    /// Fail unless the caller has `scope` and may touch `collection`.
    ///
    /// # Errors
    ///
    /// `FORBIDDEN`.
    pub fn require(&self, scope: Scope, collection: &str) -> Result<(), ApiError> {
        self.require_scope(scope)?;
        match self {
            Self::Key(key) if !key.allows(collection) => Err(ApiError::forbidden(format!(
                "this API key may not access collection `{collection}`"
            ))),
            _ => Ok(()),
        }
    }

    /// Check the scope only, for calls not tied to one collection (listing).
    ///
    /// # Errors
    ///
    /// `FORBIDDEN` when an API key lacks `scope`.
    pub fn require_scope(&self, scope: Scope) -> Result<(), ApiError> {
        match self {
            Self::Key(key) if !key.has_scope(scope) => Err(ApiError::forbidden(format!(
                "this API key lacks the `{}` scope",
                scope_name(scope)
            ))),
            _ => Ok(()),
        }
    }

    /// Fail with `PROTECTED_COLLECTION` if `collection` is protected and the caller is not the admin.
    ///
    /// # Errors
    ///
    /// `PROTECTED_COLLECTION`.
    pub fn guard_protected(&self, collection: &Collection) -> Result<(), ApiError> {
        if collection.spec().protected && !self.is_admin() {
            return Err(ApiError::protected(collection.name()));
        }
        Ok(())
    }

    /// Whether a list result may include `collection`.
    #[must_use]
    pub fn can_see(&self, collection: &str) -> bool {
        match self {
            Self::Admin { .. } => true,
            Self::Key(key) => key.allows(collection),
        }
    }
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Read => "read",
        Scope::Generate => "generate",
        Scope::Manage => "manage",
    }
}

fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, ApiError>> + Send {
        std::future::ready(Self::authenticate(parts, state))
    }
}

impl Principal {
    fn authenticate(parts: &Parts, state: &AppState) -> Result<Self, ApiError> {
        if let Some(expected) = state.local_token() {
            return Self::authenticate_local(parts, state, expected);
        }
        let now = SystemTime::now();
        let principal = bearer(parts).and_then(|token| {
            if token.starts_with(keys::KEY_PREFIX) {
                state.keys().authenticate(token, now).map(Self::Key)
            } else {
                state
                    .admin()
                    .session(token, now)
                    .map(|session| Self::Admin { session })
            }
        });
        let Some(principal) = principal else {
            state.metrics().auth_failure();
            return Err(ApiError::unauthorized());
        };
        state.check_rate(&principal.actor())?;
        Ok(principal)
    }

    /// `idbseqd console`: the token printed at start is the admin; nothing else is.
    fn authenticate_local(
        parts: &Parts,
        state: &AppState,
        expected: &str,
    ) -> Result<Self, ApiError> {
        match bearer(parts) {
            Some(token) if secrets::constant_time_eq(token, expected) => Ok(Self::Admin {
                session: LOCAL_SESSION.to_owned(),
            }),
            _ => {
                state.metrics().auth_failure();
                Err(ApiError::unauthorized())
            }
        }
    }
}

/// The session name of the admin in `idbseqd console`.
pub const LOCAL_SESSION: &str = "local";

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use idbseq_core::collection::CollectionSpec;

    use super::*;
    use crate::error::Code;

    fn key(scopes: &[Scope], collections: &[&str]) -> Principal {
        let json = serde_json::json!({
            "id": "abc123", "name": "k", "scopes": scopes,
            "collections": collections, "secret_hash": "x",
            "created_at": SystemTime::UNIX_EPOCH, "last_used_at": null
        });
        Principal::Key(serde_json::from_value(json).expect("record"))
    }

    fn admin() -> Principal {
        Principal::Admin {
            session: "h".into(),
        }
    }

    fn protected(name: &str) -> Collection {
        let spec = CollectionSpec {
            protected: true,
            ..CollectionSpec::named(name)
        };
        Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid")
    }

    #[test]
    fn the_admin_may_do_everything() {
        let admin = admin();

        assert!(admin.require(Scope::Manage, "x").is_ok());
        assert!(admin.require_admin().is_ok());
        assert!(admin.guard_protected(&protected("x")).is_ok());
        assert_eq!(admin.actor(), "admin");
    }

    #[test]
    fn keys_need_the_scope_and_an_allowed_collection() {
        let reader = key(&[Scope::Read], &["eu.*"]);

        let ok = reader.require(Scope::Read, "eu.orders");
        let wrong_scope = reader
            .require(Scope::Generate, "eu.orders")
            .expect_err("scope");
        let wrong_collection = reader
            .require(Scope::Read, "us.orders")
            .expect_err("allow-list");

        assert!(ok.is_ok());
        assert_eq!(wrong_scope.code, Code::Forbidden);
        assert!(wrong_scope.message.contains("`generate`"));
        assert_eq!(wrong_collection.code, Code::Forbidden);
        assert!(!reader.can_see("us.orders"));
        assert_eq!(reader.actor(), "key:abc123");
    }

    #[test]
    fn keys_get_protected_collection_errors_and_never_admin_rights() {
        let manager = key(&[Scope::Manage], &[]);

        let protected_error = manager
            .guard_protected(&protected("tickets"))
            .expect_err("protected");
        let admin_error = manager.require_admin().expect_err("admin only");

        assert_eq!(protected_error.code, Code::ProtectedCollection);
        assert_eq!(admin_error.code, Code::Forbidden);
    }
}
