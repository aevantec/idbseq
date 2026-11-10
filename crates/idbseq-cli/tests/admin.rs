//! Keys, login/logout, audit and configuration against a mock server.

mod support;

use std::fs;

use serde_json::{Value, json};
use support::{MockServer, fail, ok, ok_page, run_cli};

fn key(id: &str) -> Value {
    json!({
        "id": id, "name": "billing", "scopes": ["read", "generate"], "collections": [],
        "created_at": "2026-11-10T08:15:30Z", "last_used_at": null,
    })
}

#[test]
fn keys_create_shows_the_secret_once() {
    // Arrange
    let mut created = key("k1");
    created["key"] = json!("idb_k1_secret");
    let server = MockServer::start(vec![ok(created)]);

    // Act
    let run = run_cli(
        &[
            "--url",
            &server.url,
            "keys",
            "create",
            "billing",
            "--scope",
            "read,generate",
            "--collection",
            "eu.*",
        ],
        None,
        &[],
    );

    // Assert
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("Key: idb_k1_secret\n"));
    assert_eq!(
        server.only_request().json(),
        json!({ "name": "billing", "scopes": ["read", "generate"], "collections": ["eu.*"] })
    );
}

#[test]
fn keys_create_without_an_allow_list_omits_it() {
    let server = MockServer::start(vec![ok(key("k1"))]);

    run_cli(
        &[
            "--url",
            &server.url,
            "keys",
            "create",
            "svc",
            "--scope",
            "read",
        ],
        None,
        &[],
    );

    assert_eq!(
        server.only_request().json(),
        json!({ "name": "svc", "scopes": ["read"] })
    );
}

#[test]
fn keys_list_prints_a_table() {
    let server = MockServer::start(vec![ok(json!([key("k1")]))]);

    let run = run_cli(&["--url", &server.url, "keys", "list"], None, &[]);

    assert_eq!(
        run.stdout,
        "+----+---------+---------------+-------------+----------------------+--------------+\n\
         | ID | NAME    | SCOPES        | COLLECTIONS | CREATED_AT           | LAST_USED_AT |\n\
         +----+---------+---------------+-------------+----------------------+--------------+\n\
         | k1 | billing | read,generate | *           | 2026-11-10T08:15:30Z | -            |\n\
         +----+---------+---------------+-------------+----------------------+--------------+\n"
    );
}

#[test]
fn keys_revoke_confirms_with_the_id() {
    let server = MockServer::start(vec![ok(Value::Null), fail(404, "NOT_FOUND", "no key")]);

    let revoked = run_cli(
        &["--url", &server.url, "keys", "revoke", "k1"],
        None,
        &["k1"],
    );
    let unknown = run_cli(
        &["--url", &server.url, "keys", "revoke", "nope", "--yes"],
        None,
        &[],
    );

    assert_eq!(
        (revoked.code, revoked.stdout.as_str()),
        (0, "Revoked key k1\n")
    );
    assert_eq!(unknown.code, 7);
    let targets: Vec<(String, String)> = server
        .requests()
        .into_iter()
        .map(|r| (r.method, r.target))
        .collect();
    assert_eq!(
        targets,
        [
            ("DELETE".into(), "/v1/keys/k1".into()),
            ("DELETE".into(), "/v1/keys/nope".into())
        ]
    );
}

#[test]
fn login_stores_the_session_and_later_calls_use_it() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(vec![
        ok(json!({ "token": "sess-1", "expires_at": "2026-11-10T08:30:00Z" })),
        ok(json!({ "id": 1 })),
        ok(Value::Null),
        ok(json!({ "id": 2 })),
    ]);
    let url = server.url.clone();

    // Act
    let login = run_cli(
        &["--url", &url, "login"],
        Some(dir.path()),
        &["correct horse"],
    );
    let with_session = run_cli(&["--url", &url, "next", "orders"], Some(dir.path()), &[]);
    let logout = run_cli(&["logout"], Some(dir.path()), &[]);
    let after = run_cli(&["--url", &url, "next", "orders"], Some(dir.path()), &[]);

    // Assert
    assert_eq!(login.code, 0, "{}", login.stderr);
    assert!(login.asked[0].starts_with("Password for admin"));
    assert!(
        login
            .stdout
            .contains("session expires at 2026-11-10T08:30:00Z")
    );
    assert_eq!(with_session.code, 0);
    assert_eq!(logout.stdout, format!("Logged out of {url}\n"));
    assert_eq!(after.code, 0);
    let requests = server.requests();
    assert_eq!(requests[0].header("authorization"), None);
    assert_eq!(
        requests[0].json(),
        json!({ "username": "admin", "password": "correct horse" })
    );
    assert_eq!(requests[1].header("authorization"), Some("Bearer sess-1"));
    assert_eq!(requests[2].target, "/v1/auth/logout");
    assert_eq!(requests[2].header("authorization"), Some("Bearer sess-1"));
    assert_eq!(requests[3].header("authorization"), None);
    assert!(!dir.path().join("session.toml").exists());
}

#[test]
fn an_explicit_key_wins_over_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(vec![
        ok(json!({ "token": "sess", "expires_at": "x" })),
        ok(json!({ "id": 1 })),
    ]);
    run_cli(&["--url", &server.url, "login"], Some(dir.path()), &["pw"]);

    run_cli(
        &["--url", &server.url, "--key", "idb_a_b", "next", "o"],
        Some(dir.path()),
        &[],
    );

    assert_eq!(
        server.requests()[1].header("authorization"),
        Some("Bearer idb_a_b")
    );
}

#[test]
fn a_session_for_another_server_is_not_sent() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("session.toml"),
        "url = \"http://elsewhere:1\"\ntoken = \"t\"\nexpires_at = \"x\"\n",
    )
    .unwrap();
    let server = MockServer::start(vec![ok(json!({ "id": 1 }))]);

    run_cli(&["--url", &server.url, "next", "o"], Some(dir.path()), &[]);

    assert_eq!(server.only_request().header("authorization"), None);
}

#[test]
fn failed_login_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(vec![fail(401, "UNAUTHORIZED", "invalid credentials")]);

    let run = run_cli(
        &["--url", &server.url, "login"],
        Some(dir.path()),
        &["wrong"],
    );

    assert_eq!(run.code, 4);
    assert!(!dir.path().join("session.toml").exists());
}

#[test]
fn login_needs_a_config_directory() {
    let run = run_cli(&["--url", "http://127.0.0.1:9", "login"], None, &["pw"]);

    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("IDBSEQ_CONFIG_DIR"));
}

#[test]
fn logout_without_a_session_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();

    let run = run_cli(&["logout"], Some(dir.path()), &[]);

    assert_eq!((run.code, run.stdout.as_str()), (0, "Not logged in.\n"));
}

#[test]
fn logout_forgets_an_expired_session() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(vec![fail(401, "UNAUTHORIZED", "expired")]);
    let session = format!(
        "url = \"{}\"\ntoken = \"old\"\nexpires_at = \"x\"\n",
        server.url
    );
    fs::write(dir.path().join("session.toml"), session).unwrap();

    let run = run_cli(&["logout"], Some(dir.path()), &[]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(!dir.path().join("session.toml").exists());
}

#[test]
fn logout_keeps_the_session_when_the_server_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("session.toml"),
        "url = \"http://127.0.0.1:9\"\ntoken = \"t\"\nexpires_at = \"x\"\n",
    )
    .unwrap();

    let run = run_cli(&["--timeout", "2", "logout"], Some(dir.path()), &[]);

    assert_eq!(run.code, 3);
    assert!(dir.path().join("session.toml").exists());
}

#[test]
fn config_file_supplies_url_and_key() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(vec![ok(json!({ "last_id": 1, "next_id": 2 }))]);
    fs::write(
        dir.path().join("config.toml"),
        format!("url = \"{}/\"\nkey = \"idb_c_k\"\n", server.url),
    )
    .unwrap();

    let run = run_cli(&["last", "orders"], Some(dir.path()), &[]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        server.only_request().header("authorization"),
        Some("Bearer idb_c_k")
    );
}

#[test]
fn an_invalid_config_file_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "url = [").unwrap();

    let run = run_cli(&["last", "orders"], Some(dir.path()), &[]);

    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("config.toml"));
}

#[test]
fn audit_passes_filters_and_prints_entries() {
    let entry = json!({ "id": 1, "at": "2026-11-10T08:15:30Z", "actor": "admin", "action": "collection.reset",
                        "collection": "orders", "previous": null, "new": null, "request_id": "r1", "client_ip": "::1" });
    let server = MockServer::start(vec![ok_page(
        json!([entry]),
        json!({ "total": 1, "limit": 10, "next_cursor": null }),
    )]);

    let run = run_cli(
        &[
            "--url",
            &server.url,
            "audit",
            "--limit",
            "10",
            "--collection",
            "orders",
            "--action",
            "collection.reset",
        ],
        None,
        &[],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        "+----------------------+-------+------------------+------------+------------+\n\
         | AT                   | ACTOR | ACTION           | COLLECTION | REQUEST_ID |\n\
         +----------------------+-------+------------------+------------+------------+\n\
         | 2026-11-10T08:15:30Z | admin | collection.reset | orders     | r1         |\n\
         +----------------------+-------+------------------+------------+------------+\n\
         1 of 1 entries\n"
    );
    assert_eq!(
        server.only_request().target,
        "/v1/audit?limit=10&collection=orders&action=collection.reset"
    );
}
