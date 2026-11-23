//! US-0046 built-in admin console: off by default, served from the binary at
//! `/ui/` with a strict Content-Security-Policy when `ui = true`.
#![allow(clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode, header};
use common::TestApp;
use idbseq_server::config::Config;

fn with_ui() -> TestApp {
    TestApp::with_config(|config| Config { ui: true, ..config })
}

fn header_of<'a>(response: &'a common::Response, name: &str) -> &'a str {
    response
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

#[tokio::test]
async fn the_console_is_off_by_default() {
    let app = TestApp::new();

    let response = app.send(Method::GET, "/ui/", None, None).await;

    assert_eq!(
        (response.status, response.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
}

#[tokio::test]
async fn ui_without_a_slash_redirects_so_relative_assets_resolve() {
    let app = with_ui();

    let response = app.send(Method::GET, "/ui", None, None).await;

    assert_eq!(response.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(header_of(&response, header::LOCATION.as_str()), "/ui/");
}

#[tokio::test]
async fn the_page_is_served_with_a_strict_content_security_policy() {
    let app = with_ui();

    let response = app.send(Method::GET, "/ui/", None, None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(header_of(&response, "content-type").starts_with("text/html"));
    let csp = header_of(&response, "content-security-policy");
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(csp.contains("script-src 'self'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    assert!(!csp.contains("unsafe-inline"), "{csp}");
    let html = response.body.as_str().expect("html");
    assert!(html.contains(r#"<script type="module" crossorigin src="/ui/assets/"#));
}

#[tokio::test]
async fn scripts_and_styles_have_their_content_types() {
    let app = with_ui();
    let script = asset_with_extension("js");
    let style = asset_with_extension("css");

    let script = app
        .send(Method::GET, &format!("/ui/{script}"), None, None)
        .await;
    let style = app
        .send(Method::GET, &format!("/ui/{style}"), None, None)
        .await;

    assert_eq!(script.status, StatusCode::OK);
    assert!(header_of(&script, "content-type").starts_with("text/javascript"));
    assert!(header_of(&script, "content-security-policy").contains("default-src 'none'"));
    assert_eq!(style.status, StatusCode::OK);
    assert!(header_of(&style, "content-type").starts_with("text/css"));
}

fn asset_with_extension(extension: &str) -> &'static str {
    idbseq_server::ui::FILES
        .iter()
        .map(|file| file.path)
        .find(|path| path.starts_with("assets/") && path.rsplit('.').next() == Some(extension))
        .unwrap_or_else(|| panic!("no .{extension} asset in ui/dist"))
}

#[tokio::test]
async fn every_module_the_page_imports_is_served() {
    let app = with_ui();

    for file in idbseq_server::ui::FILES {
        let response = app
            .send(Method::GET, &format!("/ui/{}", file.path), None, None)
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", file.path);
    }
}

#[tokio::test]
async fn unknown_files_are_not_found() {
    let app = with_ui();

    for path in [
        "/ui/nope.js",
        "/ui/../v1/collections",
        "/ui/views/../../Cargo.toml",
    ] {
        let response = app.send(Method::GET, path, None, None).await;
        assert_ne!(response.status, StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn the_console_needs_no_credentials_but_the_api_still_does() {
    let app = with_ui();

    let page = app.send(Method::GET, "/ui/", None, None).await;
    let api = app.send(Method::GET, "/v1/collections", None, None).await;

    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(api.status, StatusCode::UNAUTHORIZED);
}

#[test]
fn every_asset_the_page_references_is_embedded() {
    let index = idbseq_server::ui::FILES
        .iter()
        .find(|file| file.path == "index.html")
        .expect("index.html");
    let html = std::str::from_utf8(index.body).expect("utf-8");
    let served: Vec<&str> = idbseq_server::ui::FILES.iter().map(|f| f.path).collect();

    let references: Vec<&str> = html
        .split(['"', '\''])
        .filter_map(|part| {
            part.strip_prefix("/ui/")
                .or_else(|| part.strip_prefix("./"))
        })
        .filter(|part| part.contains('.'))
        .collect();

    assert!(
        references.len() >= 2,
        "expected a script and a stylesheet: {references:?}"
    );
    for reference in references {
        assert!(
            served.contains(&reference),
            "index.html references {reference}, which is not embedded"
        );
    }
}

#[test]
fn the_built_page_has_no_inline_scripts_or_styles() {
    let index = idbseq_server::ui::FILES
        .iter()
        .find(|file| file.path == "index.html")
        .expect("index.html");
    let html = std::str::from_utf8(index.body).expect("utf-8");

    assert!(!html.contains("<script>"), "inline script in index.html");
    assert!(!html.contains("<style"), "inline style in index.html");
    assert!(!html.contains(" style="), "style attribute in index.html");
}
