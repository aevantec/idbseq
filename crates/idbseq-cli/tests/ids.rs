//! `idbseq next`, `last`, `reset` and error handling against a mock server.

mod support;

use serde_json::{Value, json};
use support::{MockServer, fail, ok, raw, run_cli};

#[test]
fn next_prints_the_id_and_sends_the_key() {
    // Arrange
    let server = MockServer::start(vec![ok(json!({ "id": 1044 }))]);

    // Act
    let run = run_cli(
        &["--url", &server.url, "--key", "idb_k_s", "next", "orders"],
        None,
        &[],
    );

    // Assert
    assert_eq!((run.code, run.stdout.as_str()), (0, "1044\n"));
    let request = server.only_request();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/collections/orders/next")
    );
    assert_eq!(request.header("authorization"), Some("Bearer idb_k_s"));
    assert_eq!(request.header("idempotency-key"), None);
}

#[test]
fn next_with_count_requests_a_batch_with_an_idempotency_key() {
    let server = MockServer::start(vec![ok(
        json!({ "first": 5, "last": 7, "count": 3, "step": 1 }),
    )]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "next",
            "eu/orders",
            "--count",
            "3",
            "--idempotency-key",
            "k-1",
        ],
        None,
        &[],
    );

    assert_eq!(run.stdout, "5..7 (count 3, step 1)\n");
    let request = server.only_request();
    assert_eq!(request.target, "/v1/collections/eu%2Forders/next?count=3");
    assert_eq!(request.header("idempotency-key"), Some("k-1"));
    assert_eq!(request.header("authorization"), None);
}

#[test]
fn json_output_keeps_64_bit_ids_exact() {
    let server = MockServer::start(vec![raw(
        200,
        r#"{"success":true,"data":{"id":9223372036854775807},"error":null}"#,
    )]);

    let run = run_cli(
        &["--url", &server.url, "--json", "next", "orders"],
        None,
        &[],
    );

    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, "{\n  \"id\": 9223372036854775807\n}\n");
}

#[test]
fn last_prints_last_and_next() {
    let server = MockServer::start(vec![ok(json!({ "last_id": null, "next_id": 1 }))]);

    let run = run_cli(&["--url", &server.url, "last", "orders"], None, &[]);

    assert_eq!(run.stdout, "last_id: -\nnext_id: 1\n");
    assert_eq!(server.only_request().target, "/v1/collections/orders/last");
}

#[test]
fn reset_asks_for_the_collection_name_then_sends_confirm() {
    // Arrange
    let server = MockServer::start(vec![ok(
        json!({ "previous_last_id": 50, "last_id": 9, "next_id": 10 }),
    )]);

    // Act
    let run = run_cli(
        &[
            "--url",
            &server.url,
            "reset",
            "orders",
            "--value",
            "10",
            "--allow-rewind",
        ],
        None,
        &["orders"],
    );

    // Assert
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.asked[0].contains("Type `orders` to confirm"));
    assert_eq!(
        run.stdout,
        "Reset `orders`: previous last_id 50, next ID 10\n"
    );
    let request = server.only_request();
    assert_eq!(request.target, "/v1/collections/orders/reset");
    assert_eq!(
        request.json(),
        json!({ "confirm": "orders", "value": 10, "allow_rewind": true })
    );
}

#[test]
fn reset_declined_exits_12_without_calling_the_server() {
    let server = MockServer::start(vec![]);

    let run = run_cli(&["--url", &server.url, "reset", "orders"], None, &["no"]);

    assert_eq!(run.code, 12);
    assert_eq!(run.stderr, "error: aborted\n");
    assert_eq!(server.requests().len(), 0);
}

#[test]
fn reset_with_yes_does_not_ask() {
    let server = MockServer::start(vec![ok(
        json!({ "previous_last_id": 6, "last_id": null, "next_id": 1 }),
    )]);

    let run = run_cli(&["--url", &server.url, "reset", "orders", "-y"], None, &[]);

    assert_eq!(run.code, 0);
    assert_eq!(run.asked, Vec::<String>::new());
    assert_eq!(server.only_request().json(), json!({ "confirm": "orders" }));
}

#[test]
fn api_errors_map_to_exit_codes_and_messages() {
    let cases = [
        ("UNAUTHORIZED", 401, 4),
        ("FORBIDDEN", 403, 5),
        ("PROTECTED_COLLECTION", 403, 6),
        ("COLLECTION_NOT_FOUND", 404, 7),
        ("SEQUENCE_EXHAUSTED", 409, 9),
        ("VALIDATION_FAILED", 400, 10),
        ("RATE_LIMITED", 429, 11),
        ("INTERNAL", 500, 1),
        ("BRAND_NEW", 418, 1),
    ];
    for (code, status, exit) in cases {
        let server = MockServer::start(vec![fail(status, code, "nope")]);

        let run = run_cli(&["--url", &server.url, "next", "orders"], None, &[]);

        assert_eq!(run.code, exit, "{code}");
        assert!(
            run.stderr
                .starts_with(&format!("error: {code}: nope (request req-test)")),
            "{}",
            run.stderr
        );
        assert_eq!(run.stdout, "");
    }
}

#[test]
fn json_errors_go_to_stderr_as_json() {
    let server = MockServer::start(vec![fail(409, "SEQUENCE_EXHAUSTED", "exhausted")]);

    let run = run_cli(
        &["--url", &server.url, "--json", "next", "orders"],
        None,
        &[],
    );

    let error: Value = serde_json::from_str(&run.stderr).unwrap();
    assert_eq!(run.code, 9);
    assert_eq!(error["code"], "SEQUENCE_EXHAUSTED");
    assert_eq!(error["status"], 409);
    assert_eq!(error["request_id"], "req-test");
}

#[test]
fn non_envelope_responses_exit_1() {
    let server = MockServer::start(vec![raw(502, "<html>Bad Gateway</html>")]);

    let run = run_cli(&["--url", &server.url, "last", "orders"], None, &[]);

    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("unexpected response (HTTP 502)"));
}

#[test]
fn unreachable_servers_exit_3() {
    let run = run_cli(
        &[
            "--url",
            "http://127.0.0.1:9",
            "--timeout",
            "2",
            "last",
            "orders",
        ],
        None,
        &[],
    );

    assert_eq!(run.code, 3);
    assert!(run.stderr.starts_with("error: cannot reach the server"));
}

#[test]
fn invalid_urls_exit_2() {
    let run = run_cli(&["--url", "ids:7878", "last", "orders"], None, &[]);

    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("invalid server URL"));
}

#[test]
fn empty_collection_names_exit_2() {
    let run = run_cli(&["--url", "http://127.0.0.1:9", "last", ""], None, &[]);

    assert_eq!(run.code, 2);
}
