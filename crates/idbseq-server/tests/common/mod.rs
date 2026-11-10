//! In-process test harness: a router over a temp data dir with an admin account.
#![allow(dead_code, clippy::expect_used)] // shared by several test crates

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use idbseq_core::store::RedbStore;
use idbseq_server::config::Config;
use idbseq_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

pub const ADMIN_PASSWORD: &str = "a very good password";

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    _store: Arc<RedbStore>,
    _dir: tempfile::TempDir,
}

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Response {
    pub fn data(&self) -> &Value {
        &self.body["data"]
    }

    pub fn code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

impl TestApp {
    pub fn new() -> Self {
        Self::with_config(|c| c)
    }

    pub fn with_config(edit: impl FnOnce(Config) -> Config) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = edit(Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        });
        let (state, store) =
            idbseq_server::serve::open_state(&config, Some(ADMIN_PASSWORD)).expect("open state");
        Self {
            router: idbseq_server::app::router(state.clone()),
            state,
            _store: store,
            _dir: dir,
        }
    }

    pub async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        self.send_with(method, path, token, body, &[]).await
    }

    pub async fn send_with(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Response {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("response");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
        Response {
            status,
            headers,
            body,
        }
    }

    pub async fn admin_token(&self) -> String {
        let response = self
            .send(
                Method::POST,
                "/v1/auth/login",
                None,
                Some(json!({"username": "admin", "password": ADMIN_PASSWORD})),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{:?}", response.body);
        response.data()["token"].as_str().expect("token").to_owned()
    }

    pub async fn key(&self, admin: &str, scopes: &[&str], collections: &[&str]) -> String {
        let response = self
            .send(
                Method::POST,
                "/v1/keys",
                Some(admin),
                Some(json!({"name": "test", "scopes": scopes, "collections": collections})),
            )
            .await;
        assert_eq!(response.status, StatusCode::CREATED, "{:?}", response.body);
        response.data()["key"].as_str().expect("key").to_owned()
    }

    pub async fn collection(&self, token: &str, body: Value) -> Response {
        self.send(Method::POST, "/v1/collections", Some(token), Some(body))
            .await
    }
}
