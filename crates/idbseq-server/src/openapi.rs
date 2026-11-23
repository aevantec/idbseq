//! The `OpenAPI` 3.1 contract, generated from the handlers (US-0019).

use utoipa::openapi::path::Operation;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::openapi::{ContentBuilder, Ref, RefOr, ResponseBuilder};
use utoipa::{Modify, OpenApi};

use crate::routes::{admin, collections, health, ids, keys};

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        let mut scheme = Http::new(HttpAuthScheme::Bearer);
        scheme.description = Some(
            "An API key `idb_<id>_<secret>` or an admin session token from `POST /v1/auth/login`."
                .into(),
        );
        components.add_security_scheme("bearerAuth", SecurityScheme::Http(scheme));
    }
}

/// Statuses every `/v1` operation can answer with, besides its own success and
/// conflict responses (see docs/API.md for which code each status carries).
const ERROR_STATUSES: [(&str, &str); 8] = [
    ("400", "VALIDATION_FAILED"),
    ("401", "UNAUTHORIZED"),
    ("403", "FORBIDDEN or PROTECTED_COLLECTION"),
    ("404", "NOT_FOUND or COLLECTION_NOT_FOUND"),
    ("408", "REQUEST_TIMEOUT"),
    ("413", "PAYLOAD_TOO_LARGE"),
    ("429", "RATE_LIMITED, with a Retry-After header in seconds"),
    ("500", "INTERNAL (generic message; details are logged)"),
];

/// Declare the shared error responses on every `/v1` operation.
struct ErrorResponses;

impl Modify for ErrorResponses {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        let body = ContentBuilder::new()
            .schema(Some(Ref::from_schema_name("EmptyEnvelope")))
            .build();
        for (status, codes) in ERROR_STATUSES {
            let response = ResponseBuilder::new()
                .description(format!("Error envelope: {codes}"))
                .content("application/json", body.clone())
                .build();
            components
                .responses
                .insert(format!("Error{status}"), RefOr::T(response));
        }
        for (path, item) in &mut openapi.paths.paths {
            if !path.starts_with("/v1/") {
                continue;
            }
            let operations = [
                &mut item.get,
                &mut item.post,
                &mut item.patch,
                &mut item.delete,
            ];
            for operation in operations.into_iter().flatten() {
                add_error_responses(operation);
            }
        }
    }
}

fn add_error_responses(operation: &mut Operation) {
    for (status, _) in ERROR_STATUSES {
        operation
            .responses
            .responses
            .entry(status.to_owned())
            .or_insert_with(|| RefOr::Ref(Ref::from_response_name(format!("Error{status}"))));
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "idbseq",
        description = "Increasing numeric ID sequences over HTTP/JSON. Every response uses the envelope \
            `{ success, data, error, meta? }`. Error codes: VALIDATION_FAILED 400, UNAUTHORIZED 401, \
            FORBIDDEN 403, PROTECTED_COLLECTION 403, NOT_FOUND 404, COLLECTION_NOT_FOUND 404, \
            COLLECTION_EXISTS 409, SEQUENCE_EXHAUSTED 409, REQUEST_TIMEOUT 408, PAYLOAD_TOO_LARGE 413, \
            RATE_LIMITED 429 (with Retry-After), INTERNAL 500, and from `idbseqd console` only DATABASE_BUSY 503 \
            (with Retry-After) and DATABASE_UNAVAILABLE 503. See docs/API.md."
    ),
    paths(
        health::healthz, health::readyz,
        admin::login, admin::setup, admin::logout, admin::refresh, admin::change_password, admin::audit,
        keys::list, keys::create, keys::revoke,
        collections::list, collections::create, collections::get, collections::update, collections::delete,
        ids::next, ids::last, ids::reset,
    ),
    components(schemas(crate::error::Code, crate::envelope::ErrorBody, crate::envelope::EmptyEnvelope, crate::envelope::Meta)),
    modifiers(&BearerAuth, &ErrorResponses),
    tags(
        (name = "collections", description = "Create, list, change and delete collections"),
        (name = "ids", description = "Generate, read and reset IDs"),
        (name = "auth", description = "Admin sign-in"),
        (name = "keys", description = "API keys (admin)"),
        (name = "audit", description = "Audit log (admin)"),
        (name = "health", description = "Liveness and readiness"),
    )
)]
pub struct ApiDoc;

/// The contract as served at `/v1/openapi.json` and committed at `api/openapi.json`.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    env!("CARGO_PKG_VERSION").clone_into(&mut doc.info.version);
    doc
}

/// Pretty JSON with a trailing newline, as committed.
#[must_use]
pub fn pretty_json() -> String {
    let mut text = document().to_pretty_json().unwrap_or_default();
    text.push('\n');
    text
}
