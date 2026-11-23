//! US-0016 collection endpoints, US-0013 listing, US-0015 envelope.
#![allow(clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn create_returns_201_with_defaults_in_the_envelope() {
    let app = TestApp::new();
    let admin = app.admin_token().await;

    let created = app.collection(&admin, json!({"name": "orders"})).await;

    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["success"], true);
    assert_eq!(created.body["error"], json!(null));
    let data = created.data();
    assert_eq!(data["name"], "orders");
    assert_eq!(
        (data["start"].clone(), data["step"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(data["max"], json!(i64::MAX));
    assert_eq!(data["protected"], false);
    assert_eq!(data["durability"], json!({"mode": "strict"}));
    assert_eq!(
        (data["last_id"].clone(), data["next_id"].clone()),
        (json!(null), json!(1))
    );
    assert!(data["created_at"].as_str().expect("time").ends_with('Z'));
}

#[tokio::test]
async fn create_reports_every_invalid_field_and_duplicates() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;

    let invalid = app
        .collection(&admin, json!({"name": "Bad!", "start": 10, "max": 5}))
        .await;
    let zero_step = app
        .collection(&admin, json!({"name": "ok", "step": 0}))
        .await;
    // The contract's step is an int64: larger ones break typed clients.
    let huge_step = app
        .collection(
            &admin,
            json!({"name": "ok", "step": 9_223_372_036_854_775_808_u64}),
        )
        .await;
    let unknown_field = app
        .collection(&admin, json!({"name": "ok", "colour": "red"}))
        .await;
    let duplicate = app.collection(&admin, json!({"name": "orders"})).await;

    assert_eq!(
        (invalid.status, invalid.code()),
        (StatusCode::BAD_REQUEST, "VALIDATION_FAILED")
    );
    let fields = invalid.body["error"]["details"]["fields"]
        .as_object()
        .expect("fields");
    assert!(fields.contains_key("name") && fields.contains_key("max"));
    assert_eq!(
        zero_step.body["error"]["details"]["fields"]["step"],
        "must be greater than 0"
    );
    assert_eq!(
        huge_step.body["error"]["details"]["fields"]["step"],
        "must be at most 9223372036854775807"
    );
    assert_eq!(unknown_field.code(), "VALIDATION_FAILED");
    assert_eq!(
        (duplicate.status, duplicate.code()),
        (StatusCode::CONFLICT, "COLLECTION_EXISTS")
    );
}

#[tokio::test]
async fn get_and_list_with_pagination_and_prefix() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    for name in ["eu.orders", "eu.invoices", "us.orders"] {
        app.collection(&admin, json!({"name": name})).await;
    }

    let one = app
        .send(Method::GET, "/v1/collections/eu.orders", Some(&admin), None)
        .await;
    let missing = app
        .send(Method::GET, "/v1/collections/nope", Some(&admin), None)
        .await;
    let page = app
        .send(
            Method::GET,
            "/v1/collections?limit=1&prefix=eu.",
            Some(&admin),
            None,
        )
        .await;
    let next = app
        .send(
            Method::GET,
            "/v1/collections?limit=1&prefix=eu.&cursor=eu.invoices",
            Some(&admin),
            None,
        )
        .await;
    let bad_limit = app
        .send(Method::GET, "/v1/collections?limit=501", Some(&admin), None)
        .await;

    assert_eq!(one.data()["name"], "eu.orders");
    assert_eq!(
        (missing.status, missing.code()),
        (StatusCode::NOT_FOUND, "COLLECTION_NOT_FOUND")
    );
    assert_eq!(page.data()[0]["name"], "eu.invoices");
    assert_eq!(
        page.body["meta"],
        json!({"total": 2, "limit": 1, "next_cursor": "eu.invoices"})
    );
    assert_eq!(next.data()[0]["name"], "eu.orders");
    assert_eq!(next.body["meta"]["next_cursor"], json!(null));
    assert_eq!(bad_limit.code(), "VALIDATION_FAILED");
}

#[tokio::test]
async fn patch_changes_max_and_freezes_start_after_the_first_id() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;
    let before_use = app
        .send(
            Method::PATCH,
            "/v1/collections/orders",
            Some(&admin),
            Some(json!({"start": 100})),
        )
        .await;
    app.send(
        Method::POST,
        "/v1/collections/orders/next?count=5",
        Some(&admin),
        None,
    )
    .await;

    let max = app
        .send(
            Method::PATCH,
            "/v1/collections/orders",
            Some(&admin),
            Some(json!({"max": 1000})),
        )
        .await;
    let frozen = app
        .send(
            Method::PATCH,
            "/v1/collections/orders",
            Some(&admin),
            Some(json!({"start": 5})),
        )
        .await;
    let below = app
        .send(
            Method::PATCH,
            "/v1/collections/orders",
            Some(&admin),
            Some(json!({"max": 102})),
        )
        .await;

    assert_eq!(before_use.data()["next_id"], 100);
    assert_eq!(max.data()["max"], 1000);
    assert_eq!(
        frozen.body["error"]["details"]["fields"]["start"],
        "cannot change after the first ID is issued"
    );
    assert_eq!(
        below.body["error"]["details"]["fields"]["max"],
        "must not be below last_id"
    );
}

#[tokio::test]
async fn delete_needs_confirm_in_query_or_body() {
    let app = TestApp::new();
    let admin = app.admin_token().await;
    app.collection(&admin, json!({"name": "orders"})).await;
    app.collection(&admin, json!({"name": "invoices"})).await;

    let unconfirmed = app
        .send(Method::DELETE, "/v1/collections/orders", Some(&admin), None)
        .await;
    let wrong = app
        .send(
            Method::DELETE,
            "/v1/collections/orders?confirm=invoices",
            Some(&admin),
            None,
        )
        .await;
    let by_query = app
        .send(
            Method::DELETE,
            "/v1/collections/orders?confirm=orders",
            Some(&admin),
            None,
        )
        .await;
    let by_body = app
        .send(
            Method::DELETE,
            "/v1/collections/invoices",
            Some(&admin),
            Some(json!({"confirm": "invoices"})),
        )
        .await;
    let again = app
        .send(
            Method::DELETE,
            "/v1/collections/orders?confirm=orders",
            Some(&admin),
            None,
        )
        .await;

    assert_eq!(
        unconfirmed.body["error"]["details"]["fields"]["confirm"],
        "must equal the collection name"
    );
    assert_eq!(wrong.code(), "VALIDATION_FAILED");
    assert_eq!(
        (by_query.status, by_query.data()["name"].clone()),
        (StatusCode::OK, json!("orders"))
    );
    assert_eq!(by_body.status, StatusCode::OK);
    assert_eq!(again.code(), "COLLECTION_NOT_FOUND");
}

#[tokio::test]
async fn block_durability_is_accepted_and_reported() {
    let app = TestApp::new();
    let admin = app.admin_token().await;

    let created = app
        .collection(
            &admin,
            json!({"name": "fast", "durability": {"mode": "block", "size": 100}}),
        )
        .await;
    let zero = app
        .collection(
            &admin,
            json!({"name": "zero", "durability": {"mode": "block", "size": 0}}),
        )
        .await;

    assert_eq!(
        created.data()["durability"],
        json!({"mode": "block", "size": 100})
    );
    assert_eq!(
        zero.body["error"]["details"]["fields"]["durability"],
        "block size must be greater than 0"
    );
}
