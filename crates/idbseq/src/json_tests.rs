#![allow(clippy::expect_used)] // test code: a failed setup step should panic

use serde_json::{Value, json};

use super::*;

fn open() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Database::open(dir.path().join("app.idbseq")).expect("open");
    (dir, db)
}

fn run(db: &Database, op: &str, args: &Value) -> Value {
    serde_json::from_str(&call(db, op, &args.to_string())).expect("envelope is JSON")
}

fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().expect("an error envelope")
}

#[test]
fn a_collection_has_the_api_shape() {
    let (_dir, db) = open();

    let reply = run(
        &db,
        "create",
        &json!({ "name": "cards", "start": 50, "durability": { "mode": "block", "size": 10 } }),
    );

    let data = &reply["data"];
    assert_eq!(data["name"], "cards");
    assert_eq!(
        (data["start"].as_i64(), data["step"].as_u64()),
        (Some(50), Some(1))
    );
    assert_eq!(data["max"].as_i64(), Some(i64::MAX));
    assert_eq!(data["protected"], false);
    assert_eq!(data["durability"], json!({ "mode": "block", "size": 10 }));
    assert_eq!(
        (data["last_id"].clone(), data["next_id"].as_i64()),
        (Value::Null, Some(50))
    );
    let created_at = data["created_at"].as_str().expect("timestamp");
    assert!(
        created_at.ends_with('Z') && created_at.len() == 20,
        "{created_at}"
    );
}

#[test]
fn next_returns_an_id_or_a_batch_like_the_api() {
    let (_dir, db) = open();
    run(&db, "create", &json!({ "name": "cards" }));

    let single = run(&db, "next", &json!({ "name": "cards" }));
    let batch = run(&db, "next", &json!({ "name": "cards", "count": 3 }));
    let last = run(&db, "last", &json!({ "name": "cards" }));

    assert_eq!(single, json!({ "data": { "id": 1 } }));
    assert_eq!(
        batch,
        json!({ "data": { "first": 2, "last": 4, "count": 3, "step": 1 } })
    );
    assert_eq!(last, json!({ "data": { "last_id": 4, "next_id": 5 } }));
}

#[test]
fn a_repeated_key_replays_and_a_reused_key_is_refused() {
    let (_dir, db) = open();
    run(&db, "create", &json!({ "name": "cards" }));

    let first = run(
        &db,
        "next",
        &json!({ "name": "cards", "idempotency_key": "k1" }),
    );
    let again = run(
        &db,
        "next",
        &json!({ "name": "cards", "idempotency_key": "k1" }),
    );
    let other = run(
        &db,
        "next",
        &json!({ "name": "cards", "count": 2, "idempotency_key": "k1" }),
    );

    assert_eq!(first, again);
    assert_eq!(code(&other), "VALIDATION_FAILED");
    assert!(other["error"]["details"]["fields"]["idempotency_key"].is_string());
}

#[test]
fn list_update_reset_and_delete_have_the_api_shapes() {
    let (_dir, db) = open();
    for name in ["a.one", "a.two", "b.three"] {
        run(&db, "create", &json!({ "name": name }));
    }
    run(&db, "next", &json!({ "name": "a.one", "count": 5 }));

    let page = run(&db, "list", &json!({ "prefix": "a.", "limit": 1 }));
    let updated = run(&db, "update", &json!({ "name": "a.one", "max": 100 }));
    let reset = run(
        &db,
        "reset",
        &json!({ "name": "a.one", "value": 2, "allow_rewind": true }),
    );
    let deleted = run(&db, "delete", &json!({ "name": "b.three" }));

    assert_eq!(page["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        page["meta"],
        json!({ "total": 2, "limit": 1, "next_cursor": "a.one" })
    );
    assert_eq!(updated["data"]["max"].as_i64(), Some(100));
    assert_eq!(
        reset,
        json!({ "data": { "previous_last_id": 5, "last_id": 1, "next_id": 2 } })
    );
    assert_eq!(deleted["data"]["name"], "b.three");
}

#[test]
fn errors_use_the_api_codes() {
    let (_dir, db) = open();
    run(&db, "create", &json!({ "name": "tiny", "max": 1 }));
    run(&db, "next", &json!({ "name": "tiny" }));

    assert_eq!(
        code(&run(&db, "get", &json!({ "name": "nope" }))),
        "COLLECTION_NOT_FOUND"
    );
    assert_eq!(
        code(&run(&db, "create", &json!({ "name": "tiny" }))),
        "COLLECTION_EXISTS"
    );
    assert_eq!(
        code(&run(&db, "next", &json!({ "name": "tiny" }))),
        "SEQUENCE_EXHAUSTED"
    );
    assert_eq!(
        code(&run(&db, "create", &json!({ "name": "Bad Name" }))),
        "VALIDATION_FAILED"
    );
    assert_eq!(
        code(&run(&db, "create", &json!({ "name": "x", "step": 0 }))),
        "VALIDATION_FAILED"
    );
    assert_eq!(
        code(&run(
            &db,
            "create",
            &json!({ "name": "x", "step": 9_223_372_036_854_775_808_u64 })
        )),
        "VALIDATION_FAILED"
    );
    assert_eq!(
        code(&run(&db, "get", &json!({ "name": "tiny", "extra": 1 }))),
        "VALIDATION_FAILED"
    );
    assert_eq!(
        code(&run(&db, "update", &json!({ "name": "tiny", "nope": 1 }))),
        "VALIDATION_FAILED"
    );
    assert_eq!(code(&run(&db, "fly", &json!({}))), "VALIDATION_FAILED");
}

#[test]
fn open_failures_have_their_own_codes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("app.idbseq");
    let _held = Database::open(&path).expect("open");
    let busy = Database::open_with(
        &path,
        crate::Options::default().busy_timeout(std::time::Duration::ZERO),
    )
    .expect_err("held");
    let missing =
        Database::open(dir.path().join("missing").join("app.idbseq")).expect_err("no folder");

    let busy: Value = serde_json::from_str(&error(&busy)).expect("json");
    let missing: Value = serde_json::from_str(&error(&missing)).expect("json");

    assert_eq!(code(&busy), "DATABASE_BUSY");
    assert_eq!(code(&missing), "DATABASE_UNAVAILABLE");
    assert_eq!(
        serde_json::from_str::<Value>(&error_with_code("DATABASE_CLOSED", "closed")).expect("json")
            ["error"]["code"],
        "DATABASE_CLOSED"
    );
    assert_eq!(empty(), r#"{"data":null}"#);
}
