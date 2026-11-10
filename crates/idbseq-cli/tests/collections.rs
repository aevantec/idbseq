//! `idbseq collections …` against a mock server.

mod support;

use serde_json::{Value, json};
use support::{MockServer, ok, ok_page, run_cli};

fn collection(name: &str, last_id: &Value) -> Value {
    json!({
        "name": name, "start": 1, "step": 1, "max": 9_223_372_036_854_775_807_i64,
        "protected": false, "durability": { "mode": "strict" },
        "created_at": "2026-11-10T08:15:30Z", "last_id": last_id, "next_id": 1044,
    })
}

#[test]
fn list_prints_a_table_and_the_next_cursor() {
    // Arrange
    let server = MockServer::start(vec![ok_page(
        json!([
            collection("orders", &json!(1043)),
            collection("eu.x", &Value::Null)
        ]),
        json!({ "total": 5, "limit": 2, "next_cursor": "eu.x" }),
    )]);

    // Act
    let run = run_cli(
        &[
            "--url",
            &server.url,
            "collections",
            "list",
            "--limit",
            "2",
            "--prefix",
            "o",
        ],
        None,
        &[],
    );

    // Assert
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        "+--------+---------+---------+-----------+------------+\n\
         | NAME   | LAST_ID | NEXT_ID | PROTECTED | DURABILITY |\n\
         +--------+---------+---------+-----------+------------+\n\
         | orders |    1043 |    1044 | no        | strict     |\n\
         | eu.x   |       - |    1044 | no        | strict     |\n\
         +--------+---------+---------+-----------+------------+\n\
         2 of 5 collections; next page: --cursor eu.x\n"
    );
    assert_eq!(
        server.only_request().target,
        "/v1/collections?limit=2&prefix=o"
    );
}

#[test]
fn list_all_follows_cursors() {
    let server = MockServer::start(vec![
        ok_page(
            json!([collection("a", &Value::Null)]),
            json!({ "total": 2, "limit": 1, "next_cursor": "a" }),
        ),
        ok_page(
            json!([collection("b", &Value::Null)]),
            json!({ "total": 2, "limit": 1, "next_cursor": null }),
        ),
    ]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "--json",
            "collections",
            "list",
            "--all",
            "--limit",
            "1",
        ],
        None,
        &[],
    );

    let names: Vec<Value> = serde_json::from_str::<Value>(&run.stdout)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    assert_eq!(names, [json!("a"), json!("b")]);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/v1/collections?limit=1",
            "/v1/collections?limit=1&cursor=a"
        ]
    );
}

#[test]
fn create_sends_the_settings() {
    // Arrange
    let server = MockServer::start(vec![ok(collection("orders", &Value::Null))]);

    // Act
    let run = run_cli(
        &[
            "--url",
            &server.url,
            "collections",
            "create",
            "orders",
            "--start",
            "-5",
            "--step",
            "2",
            "--max",
            "100",
            "--protected",
            "--durability",
            "block",
            "--block-size",
            "50",
        ],
        None,
        &[],
    );

    // Assert
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .starts_with("Created collection\nname:       orders\n")
    );
    let request = server.only_request();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/collections")
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(
        request.json(),
        json!({ "name": "orders", "start": -5, "step": 2, "max": 100, "protected": true,
                "durability": { "mode": "block", "size": 50 } })
    );
}

#[test]
fn create_rejects_block_durability_without_a_size() {
    let server = MockServer::start(vec![]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "collections",
            "create",
            "x",
            "--durability",
            "block",
        ],
        None,
        &[],
    );

    assert_eq!(run.code, 2);
    assert_eq!(server.requests().len(), 0);
}

#[test]
fn info_describes_the_collection() {
    let server = MockServer::start(vec![ok(collection("orders", &json!(1043)))]);

    let run = run_cli(
        &["--url", &server.url, "collections", "info", "orders"],
        None,
        &[],
    );

    assert_eq!(
        run.stdout,
        "name:       orders\nstart:      1\nstep:       1\nmax:        9223372036854775807\nprotected:  no\n\
         durability: strict\ncreated_at: 2026-11-10T08:15:30Z\nlast_id:    1043\nnext_id:    1044\n"
    );
}

#[test]
fn update_patches_only_the_given_fields() {
    let server = MockServer::start(vec![ok(collection("orders", &Value::Null))]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "collections",
            "update",
            "orders",
            "--max",
            "5000",
            "--protected",
            "false",
        ],
        None,
        &[],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    let request = server.only_request();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("PATCH", "/v1/collections/orders")
    );
    assert_eq!(request.json(), json!({ "max": 5000, "protected": false }));
}

#[test]
fn delete_confirms_then_sends_confirm() {
    // Arrange
    let server = MockServer::start(vec![ok(collection("orders", &json!(7)))]);

    // Act
    let run = run_cli(
        &["--url", &server.url, "collections", "delete", "orders"],
        None,
        &["orders"],
    );

    // Assert
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.asked[0].starts_with("Delete collection `orders`"));
    assert_eq!(run.stdout, "Deleted collection `orders` (last_id 7)\n");
    let request = server.only_request();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("DELETE", "/v1/collections/orders?confirm=orders")
    );
}

#[test]
fn delete_with_the_wrong_name_is_aborted() {
    let server = MockServer::start(vec![]);

    let run = run_cli(
        &["--url", &server.url, "collections", "delete", "orders"],
        None,
        &["order"],
    );

    assert_eq!(run.code, 12);
    assert_eq!(server.requests().len(), 0);
}

#[test]
fn delete_with_yes_skips_the_prompt() {
    let server = MockServer::start(vec![ok(collection("orders", &Value::Null))]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "--json",
            "collections",
            "delete",
            "orders",
            "--yes",
        ],
        None,
        &[],
    );

    assert_eq!(run.code, 0);
    assert_eq!(run.asked, Vec::<String>::new());
    assert_eq!(
        serde_json::from_str::<Value>(&run.stdout).unwrap()["name"],
        "orders"
    );
}
