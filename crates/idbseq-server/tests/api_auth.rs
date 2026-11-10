//! US-0021 admin, US-0022 sessions, US-0023 API keys, US-0025 audit.
#![allow(clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{ADMIN_PASSWORD, TestApp};
use serde_json::json;

#[tokio::test]
async fn every_v1_call_except_login_needs_credentials() {
    let app = TestApp::new();

    for (method, path) in [
        (Method::GET, "/v1/collections"),
        (Method::POST, "/v1/collections/x/next"),
        (Method::GET, "/v1/keys"),
        (Method::GET, "/v1/audit"),
        (Method::POST, "/v1/auth/logout"),
    ] {
        let response = app.send(method.clone(), path, None, None).await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            "{method} {path}"
        );
    }
    let garbage = app
        .send(Method::GET, "/v1/collections", Some("idb_nope_nope"), None)
        .await;
    assert_eq!(garbage.code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn login_failures_look_the_same_and_lock_after_five() {
    let app = TestApp::new();
    let login = |user: &'static str, password: &'static str| {
        app.send(
            Method::POST,
            "/v1/auth/login",
            None,
            Some(json!({"username": user, "password": password})),
        )
    };

    let wrong_user = login("root", ADMIN_PASSWORD).await;
    let wrong_password = login("admin", "not the password").await;
    for _ in 0..3 {
        login("admin", "not the password").await;
    }
    let locked = login("admin", ADMIN_PASSWORD).await;

    assert_eq!(wrong_user.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_user.body["error"]["message"],
        wrong_password.body["error"]["message"]
    );
    assert_eq!(locked.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_and_password_change_revoke_sessions() {
    let app = TestApp::new();
    let first = app.admin_token().await;
    let second = app.admin_token().await;

    let logout = app
        .send(Method::POST, "/v1/auth/logout", Some(&first), None)
        .await;
    let after_logout = app.send(Method::GET, "/v1/keys", Some(&first), None).await;
    let wrong_current = app
        .send(
            Method::POST,
            "/v1/admin/password",
            Some(&second),
            Some(json!({"current_password": "nope nope nope", "new_password": "another good password"})),
        )
        .await;
    let weak = app
        .send(
            Method::POST,
            "/v1/admin/password",
            Some(&second),
            Some(json!({"current_password": ADMIN_PASSWORD, "new_password": "short"})),
        )
        .await;
    let changed = app
        .send(
            Method::POST,
            "/v1/admin/password",
            Some(&second),
            Some(json!({"current_password": ADMIN_PASSWORD, "new_password": "another good password"})),
        )
        .await;
    let after_change = app.send(Method::GET, "/v1/keys", Some(&second), None).await;

    assert_eq!(
        logout.body,
        json!({"success": true, "data": null, "error": null})
    );
    assert_eq!(after_logout.code(), "UNAUTHORIZED");
    assert_eq!(wrong_current.code(), "UNAUTHORIZED");
    assert_eq!(weak.code(), "VALIDATION_FAILED");
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(after_change.code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn keys_are_shown_once_listed_without_secrets_and_revoked_immediately() {
    let app = TestApp::new();
    let admin = app.admin_token().await;

    let created = app
        .send(
            Method::POST,
            "/v1/keys",
            Some(&admin),
            Some(json!({"name": "billing", "scopes": ["read"], "collections": ["eu.*"]})),
        )
        .await;
    let key = created.data()["key"].as_str().expect("key").to_owned();
    let id = created.data()["id"].as_str().expect("id").to_owned();
    app.send(Method::GET, "/v1/collections", Some(&key), None)
        .await;
    let listed = app.send(Method::GET, "/v1/keys", Some(&admin), None).await;
    let key_cannot_manage_keys = app.send(Method::GET, "/v1/keys", Some(&key), None).await;
    let revoked = app
        .send(
            Method::DELETE,
            &format!("/v1/keys/{id}"),
            Some(&admin),
            None,
        )
        .await;
    let after = app
        .send(Method::GET, "/v1/collections", Some(&key), None)
        .await;
    let unknown = app
        .send(Method::DELETE, "/v1/keys/nope", Some(&admin), None)
        .await;
    let invalid = app
        .send(
            Method::POST,
            "/v1/keys",
            Some(&admin),
            Some(json!({"name": "", "scopes": []})),
        )
        .await;

    assert_eq!(created.status, StatusCode::CREATED);
    assert!(key.starts_with(&format!("idb_{id}_")));
    assert_eq!(listed.data()[0]["scopes"], json!(["read"]));
    assert_eq!(listed.data()[0]["collections"], json!(["eu.*"]));
    assert!(listed.data()[0]["last_used_at"].is_string());
    assert!(!listed.body.to_string().contains(&key));
    assert_eq!(key_cannot_manage_keys.code(), "FORBIDDEN");
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(after.code(), "UNAUTHORIZED");
    assert_eq!(
        (unknown.status, unknown.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
    assert_eq!(invalid.code(), "VALIDATION_FAILED");
}

#[tokio::test]
async fn allow_listed_keys_only_see_and_touch_their_collections() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    for name in ["eu.orders", "eu.invoices", "us.orders"] {
        app.collection(&admin, json!({"name": name})).await;
    }
    let key = app.key(&admin, &["read", "generate"], &["eu.*"]).await;

    let listed = app
        .send(Method::GET, "/v1/collections?limit=1", Some(&key), None)
        .await;
    let allowed = app
        .send(
            Method::POST,
            "/v1/collections/eu.orders/next",
            Some(&key),
            None,
        )
        .await;
    let denied = app
        .send(
            Method::POST,
            "/v1/collections/us.orders/next",
            Some(&key),
            None,
        )
        .await;

    assert_eq!(listed.data()[0]["name"], "eu.invoices", "{}", listed.body);
    assert_eq!(
        listed.body["meta"],
        json!({"total": 2, "limit": 1, "next_cursor": "eu.invoices"})
    );
    assert_eq!(allowed.status, StatusCode::OK);
    assert_eq!(denied.code(), "FORBIDDEN");
}

#[tokio::test]
async fn audit_records_destructive_and_admin_operations() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;
    app.send(
        Method::POST,
        "/v1/collections/orders/next?count=3",
        Some(&admin),
        None,
    )
    .await;
    app.send(
        Method::POST,
        "/v1/collections/orders/reset",
        Some(&admin),
        Some(json!({"confirm": "orders", "allow_rewind": true})),
    )
    .await;
    app.send(
        Method::DELETE,
        "/v1/collections/orders?confirm=orders",
        Some(&admin),
        None,
    )
    .await;
    app.send(
        Method::POST,
        "/v1/auth/login",
        None,
        Some(json!({"username": "admin", "password": "wrong password"})),
    )
    .await;

    let all = app.send(Method::GET, "/v1/audit", Some(&admin), None).await;
    let resets = app
        .send(
            Method::GET,
            "/v1/audit?action=collection.reset",
            Some(&admin),
            None,
        )
        .await;
    let bad_action = app
        .send(Method::GET, "/v1/audit?action=nope", Some(&admin), None)
        .await;

    let actions: Vec<&str> = all
        .data()
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["action"].as_str().expect("action"))
        .collect();
    assert_eq!(
        actions,
        [
            "auth.login_failed",
            "collection.delete",
            "collection.reset",
            "collection.create",
            "auth.login"
        ]
    );
    let reset = &resets.data()[0];
    assert_eq!(reset["actor"], "admin");
    assert_eq!(reset["collection"], "orders");
    assert_eq!(reset["previous"], json!({"last_id": 3}));
    assert_eq!(reset["new"], json!({"last_id": null, "next_id": 1}));
    assert!(reset["request_id"].is_string());
    assert_eq!(bad_action.code(), "VALIDATION_FAILED");
}

#[tokio::test]
async fn without_an_admin_login_is_forbidden_with_a_hint() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = idbseq_server::config::Config {
        data_dir: dir.path().to_path_buf(),
        ..idbseq_server::config::Config::default()
    };
    let (state, _store) = idbseq_server::serve::open_state(&config, None).expect("open");
    let router = idbseq_server::app::router(state);
    let request = axum::http::Request::post("/v1/auth/login")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            r#"{"username":"admin","password":"whatever it is"}"#,
        ))
        .expect("request");

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("body")
        .to_bytes();
    assert!(String::from_utf8_lossy(&bytes).contains("idbseqd admin init"));
}
