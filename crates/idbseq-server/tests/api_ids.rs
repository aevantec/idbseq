//! US-0017 next/last, US-0018 guarded reset, US-0020 idempotency keys.
#![allow(clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn next_returns_id_and_count_returns_a_batch() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(
        &admin,
        json!({"name": "invoices", "start": 1000, "step": 10}),
    )
    .await;

    let single = app
        .send(
            Method::POST,
            "/v1/collections/invoices/next",
            Some(&admin),
            None,
        )
        .await;
    let batch = app
        .send(
            Method::POST,
            "/v1/collections/invoices/next?count=3",
            Some(&admin),
            None,
        )
        .await;
    let last = app
        .send(
            Method::GET,
            "/v1/collections/invoices/last",
            Some(&admin),
            None,
        )
        .await;

    assert_eq!(single.data(), &json!({"id": 1000}));
    assert_eq!(
        batch.data(),
        &json!({"first": 1010, "last": 1030, "count": 3, "step": 10})
    );
    assert_eq!(last.data(), &json!({"last_id": 1030, "next_id": 1040}));
}

#[tokio::test]
async fn exhaustion_unknown_collections_and_batch_limits() {
    let app = TestApp::with_config(|c| idbseq_server::config::Config {
        limits: idbseq_server::config::LimitsConfig {
            max_batch: 5,
            ..c.limits.clone()
        },
        ..c
    });
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "tiny", "max": 2}))
        .await;

    let too_many = app
        .send(
            Method::POST,
            "/v1/collections/tiny/next?count=3",
            Some(&admin),
            None,
        )
        .await;
    let over_limit = app
        .send(
            Method::POST,
            "/v1/collections/tiny/next?count=6",
            Some(&admin),
            None,
        )
        .await;
    let zero = app
        .send(
            Method::POST,
            "/v1/collections/tiny/next?count=0",
            Some(&admin),
            None,
        )
        .await;
    let not_a_number = app
        .send(
            Method::POST,
            "/v1/collections/tiny/next?count=x",
            Some(&admin),
            None,
        )
        .await;
    app.send(
        Method::POST,
        "/v1/collections/tiny/next?count=2",
        Some(&admin),
        None,
    )
    .await;
    let exhausted = app
        .send(
            Method::POST,
            "/v1/collections/tiny/next",
            Some(&admin),
            None,
        )
        .await;
    let last = app
        .send(Method::GET, "/v1/collections/tiny/last", Some(&admin), None)
        .await;
    let unknown = app
        .send(
            Method::POST,
            "/v1/collections/nope/next",
            Some(&admin),
            None,
        )
        .await;

    assert_eq!(
        (too_many.status, too_many.code()),
        (StatusCode::CONFLICT, "SEQUENCE_EXHAUSTED")
    );
    assert_eq!(
        over_limit.body["error"]["details"]["fields"]["count"],
        "must be between 1 and 5"
    );
    assert_eq!(zero.code(), "VALIDATION_FAILED");
    assert_eq!(not_a_number.code(), "VALIDATION_FAILED");
    assert_eq!(exhausted.code(), "SEQUENCE_EXHAUSTED");
    assert_eq!(last.data(), &json!({"last_id": 2, "next_id": null}));
    assert_eq!(
        (unknown.status, unknown.code()),
        (StatusCode::NOT_FOUND, "COLLECTION_NOT_FOUND")
    );
}

#[tokio::test]
async fn reset_needs_confirm_and_allow_rewind_to_go_backwards() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;
    app.send(
        Method::POST,
        "/v1/collections/orders/next?count=10",
        Some(&admin),
        None,
    )
    .await;

    let no_body = app
        .send(
            Method::POST,
            "/v1/collections/orders/reset",
            Some(&admin),
            None,
        )
        .await;
    let unconfirmed = app
        .send(
            Method::POST,
            "/v1/collections/orders/reset",
            Some(&admin),
            Some(json!({})),
        )
        .await;
    let rewind_refused = app
        .send(
            Method::POST,
            "/v1/collections/orders/reset",
            Some(&admin),
            Some(json!({"confirm": "orders"})),
        )
        .await;
    let forward = app
        .send(
            Method::POST,
            "/v1/collections/orders/reset",
            Some(&admin),
            Some(json!({"confirm": "orders", "value": 100})),
        )
        .await;
    let rewound = app
        .send(
            Method::POST,
            "/v1/collections/orders/reset",
            Some(&admin),
            Some(json!({"confirm": "orders", "allow_rewind": true})),
        )
        .await;

    assert_eq!(no_body.code(), "VALIDATION_FAILED");
    assert_eq!(
        unconfirmed.body["error"]["details"]["fields"]["confirm"],
        "must equal the collection name"
    );
    assert!(rewind_refused.body["error"]["details"]["fields"]["allow_rewind"].is_string());
    assert_eq!(
        forward.data(),
        &json!({"previous_last_id": 10, "last_id": 99, "next_id": 100})
    );
    assert_eq!(
        rewound.data(),
        &json!({"previous_last_id": 99, "last_id": null, "next_id": 1})
    );
}

#[tokio::test]
async fn an_idempotent_retry_returns_the_original_ids_per_credential() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;
    let key = app.key(&admin, &["generate"], &[]).await;
    let header = [("idempotency-key", "order-42")];

    let first = app
        .send_with(
            Method::POST,
            "/v1/collections/orders/next?count=2",
            Some(&key),
            None,
            &header,
        )
        .await;
    let retry = app
        .send_with(
            Method::POST,
            "/v1/collections/orders/next?count=2",
            Some(&key),
            None,
            &header,
        )
        .await;
    let different = app
        .send_with(
            Method::POST,
            "/v1/collections/orders/next?count=3",
            Some(&key),
            None,
            &header,
        )
        .await;
    let other_credential = app
        .send_with(
            Method::POST,
            "/v1/collections/orders/next?count=2",
            Some(&admin),
            None,
            &header,
        )
        .await;
    let bad_header = app
        .send_with(
            Method::POST,
            "/v1/collections/orders/next",
            Some(&key),
            None,
            &[("idempotency-key", "has space")],
        )
        .await;

    assert_eq!(
        first.data(),
        &json!({"first": 1, "last": 2, "count": 2, "step": 1})
    );
    assert_eq!(retry.data(), first.data());
    assert_eq!(
        different.body["error"]["details"]["fields"]["idempotency_key"],
        "was already used for a different request"
    );
    assert_eq!(other_credential.data()["first"], 3);
    assert_eq!(bad_header.code(), "VALIDATION_FAILED");
}
