//! US-0015 envelope for non-handler errors, US-0019 health/metrics, US-0026 hardening:
//! security headers, request IDs, body limits, rate limits, the served contract.
#![allow(clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use idbseq_server::config::{Config, LimitsConfig, MetricsConfig, RateLimitConfig};
use serde_json::json;

#[tokio::test]
async fn unknown_routes_and_methods_use_the_envelope() {
    let app = TestApp::new();

    let missing = app.send(Method::GET, "/v1/nope", None, None).await;
    let wrong_method = app.send(Method::PUT, "/v1/collections", None, None).await;

    assert_eq!(
        (missing.status, missing.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
    assert_eq!(wrong_method.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong_method.body["success"], false);
}

#[tokio::test]
async fn every_response_carries_security_headers_and_a_request_id() {
    let app = TestApp::new();

    let fresh = app.send(Method::GET, "/healthz", None, None).await;
    let echoed = app
        .send_with(
            Method::GET,
            "/v1/nope",
            None,
            None,
            &[("x-request-id", "trace-123")],
        )
        .await;

    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
    ] {
        assert_eq!(fresh.headers[name], value, "{name}");
        assert_eq!(echoed.headers[name], value, "{name} on an error");
    }
    assert!(!fresh.headers["x-request-id"].is_empty());
    assert_eq!(echoed.headers["x-request-id"], "trace-123");
}

#[tokio::test]
async fn oversized_bodies_are_rejected_with_413() {
    let app = TestApp::with_config(|c| Config {
        limits: LimitsConfig {
            body_bytes: 64,
            ..c.limits.clone()
        },
        ..c
    });
    let admin = app.admin_token().await;

    let large = app
        .collection(
            &admin,
            json!({"name": "orders", "padding": "x".repeat(200)}),
        )
        .await;

    assert_eq!(
        (large.status, large.code()),
        (StatusCode::PAYLOAD_TOO_LARGE, "PAYLOAD_TOO_LARGE")
    );
}

#[tokio::test]
async fn bursts_beyond_the_rate_limit_get_429_with_retry_after() {
    let app = TestApp::with_config(|c| Config {
        rate_limit: RateLimitConfig {
            per_second: 1,
            burst: 2,
        },
        ..c
    });
    let admin = app.admin_token().await;

    let mut last = None;
    for _ in 0..3 {
        last = Some(
            app.send(Method::GET, "/v1/collections", Some(&admin), None)
                .await,
        );
    }
    let limited = last.expect("response");

    assert_eq!(
        (limited.status, limited.code()),
        (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED")
    );
    let wait: u64 = limited.headers["retry-after"]
        .to_str()
        .expect("ascii")
        .parse()
        .expect("seconds");
    assert!(wait >= 1);
}

#[tokio::test]
async fn health_endpoints_need_no_credentials() {
    let app = TestApp::new();

    let live = app.send(Method::GET, "/healthz", None, None).await;
    let ready = app.send(Method::GET, "/readyz", None, None).await;
    app.state.set_not_ready();
    let draining = app.send(Method::GET, "/readyz", None, None).await;

    assert_eq!(live.data(), &json!({"status": "ok"}));
    assert_eq!(ready.data(), &json!({"status": "ready"}));
    assert_eq!(draining.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn metrics_are_off_by_default_and_exported_when_enabled() {
    let off = TestApp::new();
    let on = TestApp::with_config(|c| Config {
        metrics: MetricsConfig { enabled: true },
        ..c
    });
    let admin = on.admin_token().await;
    on.collection(&admin, json!({"name": "orders"})).await;
    on.send(
        Method::POST,
        "/v1/collections/orders/next?count=4",
        Some(&admin),
        None,
    )
    .await;

    let disabled = off.send(Method::GET, "/metrics", None, None).await;
    let exported = on.send(Method::GET, "/metrics", None, None).await;

    assert_eq!(disabled.status, StatusCode::NOT_FOUND);
    let text = exported.body.as_str().expect("text");
    assert!(text.contains("idbseq_http_requests_total"), "{text}");
    assert!(text.contains("orders"), "{text}");
}

#[tokio::test]
async fn the_openapi_contract_is_served_and_lists_every_route() {
    let app = TestApp::new();

    let doc = app.send(Method::GET, "/v1/openapi.json", None, None).await;

    assert!(
        doc.body["openapi"]
            .as_str()
            .expect("version")
            .starts_with("3.1")
    );
    let paths = doc.body["paths"].as_object().expect("paths");
    for path in [
        "/v1/collections",
        "/v1/collections/{name}",
        "/v1/collections/{name}/next",
        "/v1/collections/{name}/last",
        "/v1/collections/{name}/reset",
        "/v1/auth/login",
        "/v1/auth/logout",
        "/v1/admin/password",
        "/v1/keys",
        "/v1/keys/{id}",
        "/v1/audit",
        "/healthz",
        "/readyz",
    ] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    assert!(doc.body["components"]["securitySchemes"]["bearerAuth"].is_object());
    let next = &doc.body["paths"]["/v1/collections/{name}/next"]["post"]["responses"];
    for status in ["200", "400", "401", "403", "404", "409", "429", "500"] {
        assert!(next[status].is_object(), "next lacks {status}");
    }
    assert_eq!(next["429"]["$ref"], "#/components/responses/Error429");
}
