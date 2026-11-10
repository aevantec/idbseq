//! US-0024: every operation × {admin, manage key, generate key, read key, no auth},
//! on a protected and an unprotected collection.
#![allow(clippy::expect_used)]

mod common;

use axum::http::Method;
use common::TestApp;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
enum Op {
    Read,
    Last,
    Next,
    Patch,
    SetProtected,
    Reset,
    Delete,
}

impl Op {
    fn request(self, name: &str) -> (Method, String, Option<Value>) {
        let base = format!("/v1/collections/{name}");
        match self {
            Self::Read => (Method::GET, base, None),
            Self::Last => (Method::GET, format!("{base}/last"), None),
            Self::Next => (Method::POST, format!("{base}/next"), None),
            Self::Patch => (Method::PATCH, base, Some(json!({"max": 1_000_000}))),
            Self::SetProtected => (Method::PATCH, base, Some(json!({"protected": false}))),
            Self::Reset => (
                Method::POST,
                format!("{base}/reset"),
                Some(json!({"confirm": name, "value": 500})),
            ),
            Self::Delete => (Method::DELETE, format!("{base}?confirm={name}"), None),
        }
    }
}

const OPS: [Op; 7] = [
    Op::Read,
    Op::Last,
    Op::Next,
    Op::Patch,
    Op::SetProtected,
    Op::Reset,
    Op::Delete,
];

/// Expected outcome code ("" = success) per principal, for (protected, unprotected).
#[allow(
    clippy::match_same_arms,
    reason = "one row per principal reads as the spec table"
)]
fn expected(op: Op, principal: &str) -> (&'static str, &'static str) {
    use Op::{Delete, Last, Next, Patch, Read, Reset, SetProtected};
    match (principal, op) {
        ("admin", _) => ("", ""),
        ("none", _) => ("UNAUTHORIZED", "UNAUTHORIZED"),
        ("manage", Read | Last | Next) => ("FORBIDDEN", "FORBIDDEN"),
        ("manage", SetProtected) => ("PROTECTED_COLLECTION", "PROTECTED_COLLECTION"),
        ("manage", Patch | Reset | Delete) => ("PROTECTED_COLLECTION", ""),
        ("generate", Next) | ("read", Read | Last) => ("", ""),
        _ => ("FORBIDDEN", "FORBIDDEN"),
    }
}

#[tokio::test]
async fn the_protection_matrix_holds() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    let tokens = [
        ("admin", Some(admin.clone())),
        ("manage", Some(app.key(&admin, &["manage"], &[]).await)),
        ("generate", Some(app.key(&admin, &["generate"], &[]).await)),
        ("read", Some(app.key(&admin, &["read"], &[]).await)),
        ("none", None),
    ];
    let mut failures = Vec::new();
    for (principal, token) in &tokens {
        for op in OPS {
            let (want_protected, want_open) = expected(op, principal);
            for (protected, want) in [(true, want_protected), (false, want_open)] {
                let name = format!("c-{principal}-{op:?}-{protected}").to_lowercase();
                app.collection(&admin, json!({"name": name, "protected": protected}))
                    .await;
                let (method, path, body) = op.request(&name);
                let response = app.send(method, &path, token.as_deref(), body).await;
                let got = if response.status.is_success() {
                    ""
                } else {
                    response.code()
                };
                if got != want {
                    failures.push(format!(
                        "{principal} {op:?} protected={protected}: want {want:?}, got {got:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn only_the_admin_creates_protected_collections() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    let manager = app.key(&admin, &["manage"], &[]).await;

    let by_key = app
        .collection(&manager, json!({"name": "tickets", "protected": true}))
        .await;
    let unprotected_by_key = app.collection(&manager, json!({"name": "notes"})).await;
    let by_admin = app
        .collection(&admin, json!({"name": "tickets", "protected": true}))
        .await;

    assert_eq!(by_key.code(), "PROTECTED_COLLECTION");
    assert!(unprotected_by_key.status.is_success());
    assert_eq!(by_admin.data()["protected"], true);
}
