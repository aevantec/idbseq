//! The built-in admin console (US-0046): a Vue app built into `ui/dist`
//! (committed), compiled into the binary by `build.rs`, and served at `/ui/`
//! when `ui = true`. It calls the same-origin API with the admin session, so it
//! needs no CORS and loads nothing from a CDN.

use axum::extract::Path;
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};

use crate::error::{ApiError, Code};

/// Scripts and styles only from this origin, no inline code, no framing.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
    style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'none'";

/// One file of the console, served at `/ui/<path>`.
pub struct UiFile {
    pub path: &'static str,
    pub content_type: &'static str,
    pub body: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/ui_files.rs"));

/// `/ui` → `/ui/`, so the page's relative imports resolve under `/ui/`.
pub async fn redirect() -> Redirect {
    Redirect::permanent("/ui/")
}

/// `/ui/`: the console's page.
pub async fn index() -> Response {
    serve("index.html")
}

/// `/ui/<path>`: one of [`FILES`], or `NOT_FOUND`. Routes live in the URL
/// fragment (`/ui/#/keys`), so there is no fallback to `index.html`.
pub async fn file(Path(path): Path<String>) -> Response {
    serve(&path)
}

fn serve(path: &str) -> Response {
    match FILES.iter().find(|file| file.path == path) {
        Some(file) => (
            [
                (header::CONTENT_TYPE, file.content_type),
                (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
            ],
            file.body,
        )
            .into_response(),
        None => ApiError::new(Code::NotFound, "no such file").into_response(),
    }
}
