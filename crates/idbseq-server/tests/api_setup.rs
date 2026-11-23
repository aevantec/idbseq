//! First-run setup from the console (US-0047): a one-time link creates the admin.
#![allow(clippy::expect_used)]

mod common;

use std::time::SystemTime;

use axum::http::{Method, StatusCode};
use common::TestApp;
use idbseq_server::config::Config;
use serde_json::json;

const PASSWORD: &str = "chosen in the browser";

fn console_on(config: Config) -> Config {
    config.with_ui()
}

fn setup_body(token: &str, password: &str) -> serde_json::Value {
    json!({"token": token, "password": password})
}

#[tokio::test]
async fn setup_is_not_served_when_the_console_is_off() {
    let app = TestApp::without_admin(|c| c);
    let token = app.state.setup().issue(SystemTime::now());

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, PASSWORD)),
        )
        .await;

    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(!app.state.admin().exists().expect("exists"));
}

#[tokio::test]
async fn the_setup_link_creates_the_admin_and_signs_in() {
    let app = TestApp::without_admin(console_on);
    let token = app.state.setup().issue(SystemTime::now());

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, PASSWORD)),
        )
        .await;

    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let session = reply.data()["token"].as_str().expect("session token");
    assert!(reply.data()["expires_at"].is_string());
    let collections = app
        .send(Method::GET, "/v1/collections", Some(session), None)
        .await;
    assert_eq!(collections.status, StatusCode::OK);
    let login = app
        .send(
            Method::POST,
            "/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": PASSWORD})),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
    let audit = app
        .send(
            Method::GET,
            "/v1/audit?action=admin.init",
            Some(session),
            None,
        )
        .await;
    assert_eq!(
        audit.data().as_array().map(Vec::len),
        Some(1),
        "{:?}",
        audit.body
    );
}

#[tokio::test]
async fn the_link_works_only_once() {
    let app = TestApp::without_admin(console_on);
    let token = app.state.setup().issue(SystemTime::now());
    let first = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, PASSWORD)),
        )
        .await;
    assert_eq!(first.status, StatusCode::OK);

    let again = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, "another password!")),
        )
        .await;

    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    assert_eq!(again.code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_refused() {
    let app = TestApp::without_admin(console_on);
    let _token = app.state.setup().issue(SystemTime::now());

    let wrong = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&"0".repeat(64), PASSWORD)),
        )
        .await;
    let missing = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(json!({"password": PASSWORD})),
        )
        .await;

    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    assert!(!app.state.admin().exists().expect("exists"));
}

#[tokio::test]
async fn without_a_pending_link_setup_is_refused() {
    let app = TestApp::without_admin(console_on);

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&"0".repeat(64), PASSWORD)),
        )
        .await;

    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_weak_password_is_rejected_and_the_link_stays_valid() {
    let app = TestApp::without_admin(console_on);
    let token = app.state.setup().issue(SystemTime::now());

    let weak = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, "short")),
        )
        .await;
    let good = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, PASSWORD)),
        )
        .await;

    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    assert_eq!(weak.code(), "VALIDATION_FAILED");
    assert!(
        weak.body["error"]["details"]["fields"]["password"].is_string(),
        "{:?}",
        weak.body
    );
    assert_eq!(good.status, StatusCode::OK);
}

#[tokio::test]
async fn setup_is_refused_once_an_admin_exists() {
    let app = TestApp::with_config(console_on);
    let token = app.state.setup().issue(SystemTime::now());

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/setup",
            None,
            Some(setup_body(&token, PASSWORD)),
        )
        .await;

    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    let old = app.admin_token().await;
    assert!(!old.is_empty(), "the original password still works");
}
