//! `idbseqd console <file>`: the admin console on an embedded file (US-0054).
//! One token is the admin, server-only routes are gone, and IDs taken through
//! the console and by another user of the file never repeat.
#![allow(clippy::expect_used)] // test code: a failed expectation is the failure

use std::io::{BufRead, BufReader};
use std::num::NonZeroU32;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use idbseq_core::store::{
    AdvanceRequest, DEFAULT_BUSY_TIMEOUT, SequenceStore, SharedFileStore, StoreError,
};
use idbseq_server::error::{ApiError, Code};
use idbseq_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

const BIN: &str = env!("CARGO_BIN_EXE_idbseqd");

struct Console {
    router: Router,
    token: String,
    file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Console {
    fn open() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("app.idbseq");
        let state: AppState = idbseq_server::console::open(&file, 0, true).expect("console");
        let token = state.local_token().expect("token").to_owned();
        Self {
            router: idbseq_server::app::router(state),
            token,
            file,
            _dir: dir,
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
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
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn admin(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let token = self.token.clone();
        self.send(method, path, Some(&token), body).await
    }
}

fn next_elsewhere(file: &Path, name: &str) -> i64 {
    let store = SharedFileStore::new(file, DEFAULT_BUSY_TIMEOUT);
    let one = NonZeroU32::new(1).expect("non-zero");
    store
        .advance(&AdvanceRequest::new(name, one))
        .expect("advance")
        .range
        .first
}

#[tokio::test]
async fn only_the_printed_token_is_the_admin() {
    let console = Console::open();

    let (missing, _) = console
        .send(Method::GET, "/v1/collections", None, None)
        .await;
    let (wrong, body) = console
        .send(Method::GET, "/v1/collections", Some("not-the-token"), None)
        .await;
    let (right, _) = console.admin(Method::GET, "/v1/collections", None).await;

    assert_eq!(missing, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    assert_eq!(right, StatusCode::OK);
}

#[tokio::test]
async fn accounts_keys_and_the_audit_log_do_not_exist() {
    let console = Console::open();

    for (method, path) in [
        (Method::POST, "/v1/auth/login"),
        (Method::POST, "/v1/auth/setup"),
        (Method::POST, "/v1/auth/refresh"),
        (Method::POST, "/v1/admin/password"),
        (Method::GET, "/v1/keys"),
        (Method::GET, "/v1/audit"),
        (Method::GET, "/metrics"),
    ] {
        let (status, _) = console.admin(method, path, Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (ui, _) = console.send(Method::GET, "/ui/", None, None).await;
    assert_eq!(ui, StatusCode::OK);
    // The UI asks, so a reload offers the console link rather than a password form.
    let (_, health) = console.send(Method::GET, "/healthz", None, None).await;
    assert_eq!(health["data"]["mode"], "console");
}

#[tokio::test]
async fn the_console_and_another_user_of_the_file_never_repeat_an_id() {
    let console = Console::open();
    let (created, _) = console
        .admin(
            Method::POST,
            "/v1/collections",
            Some(json!({"name": "cards", "start": 100})),
        )
        .await;
    assert_eq!(created, StatusCode::CREATED);

    let mut ids = Vec::new();
    for _ in 0..5 {
        let (status, body) = console
            .admin(Method::POST, "/v1/collections/cards/next", None)
            .await;
        assert_eq!(status, StatusCode::OK);
        ids.push(body["data"]["id"].as_i64().expect("id"));
        ids.push(next_elsewhere(&console.file, "cards"));
    }

    assert_eq!(ids, (100..110).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_file_deleted_while_the_console_runs_is_reported_not_recreated() {
    let console = Console::open();
    let (created, _) = console
        .admin(
            Method::POST,
            "/v1/collections",
            Some(json!({"name": "cards"})),
        )
        .await;
    assert_eq!(created, StatusCode::CREATED);

    std::fs::remove_file(&console.file).expect("delete");
    let (status, body) = console.admin(Method::GET, "/v1/collections", None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "DATABASE_UNAVAILABLE");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("deleted or moved"))
    );
    assert!(!console.file.exists(), "no empty file took its place");
}

#[test]
fn a_busy_or_unopenable_file_is_a_503() {
    let busy = ApiError::from(StoreError::Busy {
        path: "app.idbseq".into(),
        waited: Duration::from_secs(5),
    });
    let unavailable = ApiError::from(StoreError::Unavailable(
        idbseq_core::store::OpenError::FileInUse("app.idbseq".into()),
    ));

    assert_eq!(
        (busy.code, busy.retry_after_secs),
        (Code::DatabaseBusy, Some(1))
    );
    assert_eq!(busy.code.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(unavailable.code, Code::DatabaseUnavailable);
    assert!(
        !unavailable.message.contains("app.idbseq"),
        "no paths leak to clients"
    );
}

#[test]
fn the_command_prints_a_working_link_and_refuses_a_missing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("app.idbseq");

    let missing = Command::new(BIN)
        .arg("console")
        .arg(&file)
        .output()
        .expect("run");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("does not exist"));

    let mut child = Command::new(BIN)
        .args(["console", "--create"])
        .arg(&file)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn");
    let stderr = child.stderr.take().expect("stderr");
    let link = BufReader::new(stderr)
        .lines()
        .map_while(Result::ok)
        .find(|line| line.starts_with("http://"))
        .expect("printed link");
    let (base, fragment) = link.split_once("/ui/#/local?").expect("console link");
    let token = fragment
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .expect("token");

    let http = reqwest::blocking::Client::new();
    let without = http
        .get(format!("{base}/v1/collections"))
        .send()
        .expect("get");
    let with = http
        .get(format!("{base}/v1/collections"))
        .bearer_auth(token)
        .send()
        .expect("get");
    child.kill().expect("stop");
    child.wait().expect("wait");

    assert!(base.starts_with("http://127.0.0.1:"), "{base}");
    assert!(fragment.ends_with("&file=app.idbseq"), "{fragment}");
    assert_eq!(without.status().as_u16(), 401);
    assert_eq!(with.status().as_u16(), 200);
}
