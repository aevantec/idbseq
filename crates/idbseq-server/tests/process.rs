//! Real `idbseqd` processes: US-0010 crash safety (SIGKILL), US-0019 graceful shutdown,
//! single instance per data dir, `healthcheck`, US-0033 backup while running and restore,
//! US-0047 the first-run setup link.
#![cfg(unix)]
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_idbseqd");
const PASSWORD: &str = "a very good password";
const STARTUP: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(50);
const CRASH_ROUNDS: usize = 3;
const WRITERS: usize = 8;

struct Server {
    child: Child,
    base: String,
    bind: String,
    data_dir: PathBuf,
    http: Client,
}

impl Server {
    fn start(data_dir: &Path) -> Self {
        let bind = free_address();
        let child = Command::new(BIN)
            .env("IDBSEQ_DATA_DIR", data_dir)
            .env("IDBSEQ_BIND", &bind)
            .env("IDBSEQ_ADMIN_PASSWORD", PASSWORD)
            .env("IDBSEQ_LOG", "error")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn idbseqd");
        let server = Self {
            child,
            base: format!("http://{bind}"),
            bind,
            data_dir: data_dir.to_path_buf(),
            http: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
        };
        server.wait_until_up();
        server
    }

    fn wait_until_up(&self) {
        let deadline = Instant::now() + STARTUP;
        while Instant::now() < deadline {
            if self
                .http
                .get(format!("{}/healthz", self.base))
                .send()
                .is_ok_and(|r| r.status().is_success())
            {
                return;
            }
            thread::sleep(POLL);
        }
        panic!("idbseqd did not come up on {}", self.bind);
    }

    fn token(&self) -> String {
        let reply = self.call(
            "POST",
            "/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": PASSWORD})),
        );
        reply["data"]["token"].as_str().expect("token").to_owned()
    }

    fn call(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> Value {
        try_call(&self.http, &self.base, method, path, token, body).expect("request")
    }

    fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .args([format!("-{name}"), self.child.id().to_string()])
            .status()
            .expect("kill");
        assert!(status.success());
    }

    fn wait(mut self, limit: Duration) -> ExitStatus {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "idbseqd did not exit within {limit:?}"
            );
            thread::sleep(POLL);
        }
    }

    fn crash(mut self) {
        self.child.kill().expect("SIGKILL");
        self.child.wait().expect("reap");
    }

    fn command(&self, args: &[&str]) -> Output {
        command(&self.data_dir, &self.bind, args)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").to_string()
}

fn command(data_dir: &Path, bind: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env("IDBSEQ_DATA_DIR", data_dir)
        .env("IDBSEQ_BIND", bind)
        .output()
        .expect("run idbseqd")
}

fn try_call(
    http: &Client,
    base: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> reqwest::Result<Value> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).expect("method");
    let mut request = http.request(method, format!("{base}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send()?.json()
}

/// Hammer `next` from several threads until the server dies; return every acknowledged ID.
fn issue_until_killed(
    server: Server,
    token: &str,
    collection: &str,
    run_for: Duration,
) -> Vec<i64> {
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let alive = Arc::new(AtomicBool::new(true));
    let writers: Vec<_> = (0..WRITERS)
        .map(|_| {
            let (acknowledged, alive) = (Arc::clone(&acknowledged), Arc::clone(&alive));
            let (http, base, token) = (server.http.clone(), server.base.clone(), token.to_owned());
            let path = format!("/v1/collections/{collection}/next");
            thread::spawn(move || {
                while alive.load(Ordering::Relaxed) {
                    let Ok(reply) = try_call(&http, &base, "POST", &path, Some(&token), None)
                    else {
                        return;
                    };
                    if let Some(id) = reply["data"]["id"].as_i64() {
                        acknowledged.lock().expect("lock").push(id);
                    }
                }
            })
        })
        .collect();
    thread::sleep(run_for);
    server.crash();
    alive.store(false, Ordering::Relaxed);
    for writer in writers {
        writer.join().expect("writer");
    }
    Arc::try_unwrap(acknowledged)
        .expect("sole owner")
        .into_inner()
        .expect("lock")
}

#[test]
fn acknowledged_ids_survive_sigkill_and_are_never_reissued() {
    let dir = tempfile::tempdir().expect("temp dir");
    let server = Server::start(dir.path());
    let token = server.token();
    for (name, durability) in [
        ("strict", json!({"mode": "strict"})),
        ("block", json!({"mode": "block", "size": 50})),
    ] {
        server.call(
            "POST",
            "/v1/collections",
            Some(&token),
            Some(json!({"name": name, "durability": durability})),
        );
    }
    let mut server = Some(server);

    for name in ["strict", "block"] {
        let mut seen = BTreeSet::new();
        for round in 0..CRASH_ROUNDS {
            let running = server.take().unwrap_or_else(|| Server::start(dir.path()));
            let token = running.token();
            let ids = issue_until_killed(running, &token, name, Duration::from_millis(300));
            assert!(!ids.is_empty(), "{name} round {round}: no IDs issued");
            for id in &ids {
                assert!(seen.insert(*id), "{name} round {round}: {id} issued twice");
            }
            let restarted = Server::start(dir.path());
            let token = restarted.token();
            let next = restarted.call(
                "POST",
                &format!("/v1/collections/{name}/next"),
                Some(&token),
                None,
            );
            let next = next["data"]["id"].as_i64().expect("id");
            let max = *seen.last().expect("ids");
            assert!(
                next > max,
                "{name} round {round}: next {next} <= acknowledged {max}"
            );
            seen.insert(next);
            server = Some(restarted);
        }
    }
}

#[test]
fn sigterm_shuts_down_cleanly_and_healthcheck_follows_the_server() {
    let dir = tempfile::tempdir().expect("temp dir");
    let server = Server::start(dir.path());
    let bind = server.bind.clone();

    let healthy = server.command(&["healthcheck"]);
    server.signal("TERM");
    let status = server.wait(Duration::from_secs(10));
    let after = command(dir.path(), &bind, &["healthcheck"]);

    assert!(healthy.status.success(), "{healthy:?}");
    assert!(status.success(), "exit status {status:?}");
    assert!(!after.status.success());
}

#[test]
fn a_second_instance_on_the_same_data_dir_fails_with_a_clear_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let _first = Server::start(dir.path());

    let second = Command::new(BIN)
        .env("IDBSEQ_DATA_DIR", dir.path())
        .env("IDBSEQ_BIND", free_address())
        .env("IDBSEQ_LOG", "error")
        .output()
        .expect("run second idbseqd");

    assert!(!second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already in use"), "{stderr}");
}

#[test]
fn backup_while_running_then_restore_and_skip_ahead() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snapshot = dir.path().join("snapshot.redb");
    let snapshot_arg = snapshot.to_str().expect("utf-8 path");
    let server = Server::start(dir.path());
    let token = server.token();
    server.call(
        "POST",
        "/v1/collections",
        Some(&token),
        Some(json!({"name": "orders"})),
    );
    server.call(
        "POST",
        "/v1/collections/orders/next?count=5",
        Some(&token),
        None,
    );

    let backup = server.command(&["backup", snapshot_arg]);
    server.call(
        "POST",
        "/v1/collections/orders/next?count=5",
        Some(&token),
        None,
    );
    let while_running = server.command(&["restore", snapshot_arg]);
    server.signal("TERM");
    let bind = server.bind.clone();
    server.wait(Duration::from_secs(10));
    let refused = command(dir.path(), &bind, &["restore", snapshot_arg]);
    let forced = command(dir.path(), &bind, &["restore", "--force", snapshot_arg]);
    let restored = Server::start(dir.path());
    let token = restored.token();
    let last = restored.call("GET", "/v1/collections/orders/last", Some(&token), None);
    let skipped = restored.call(
        "POST",
        "/v1/collections/orders/reset",
        Some(&token),
        Some(json!({"confirm": "orders", "value": 11})),
    );
    let next = restored.call("POST", "/v1/collections/orders/next", Some(&token), None);

    assert!(backup.status.success(), "{backup:?}");
    assert!(!while_running.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("orders"),
        "{refused:?}"
    );
    assert!(forced.status.success(), "{forced:?}");
    assert!(String::from_utf8_lossy(&forced.stdout).contains("pre-restore"));
    assert_eq!(last["data"], json!({"last_id": 5, "next_id": 6}));
    assert_eq!(skipped["data"]["next_id"], 11);
    assert_eq!(next["data"], json!({"id": 11}));
}

fn command_with_stdin(data_dir: &Path, args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(BIN)
        .args(args)
        .env("IDBSEQ_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn idbseqd");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

#[test]
fn admin_init_offline_and_password_reset_through_the_running_server() {
    let dir = tempfile::tempdir().expect("temp dir");
    let new_password = "a brand new password";

    let init = command_with_stdin(
        dir.path(),
        &["admin", "init", "--password-stdin"],
        &format!("{PASSWORD}\n"),
    );
    let again = command_with_stdin(
        dir.path(),
        &["admin", "init", "--password-stdin"],
        &format!("{PASSWORD}\n"),
    );
    let server = Server::start(dir.path());
    let old_session = server.token();
    let weak = command_with_stdin(
        dir.path(),
        &["admin", "reset-password", "--password-stdin"],
        "short\n",
    );
    let reset = command_with_stdin(
        dir.path(),
        &["admin", "reset-password", "--password-stdin"],
        &format!("{new_password}\n"),
    );
    let revoked = server.call("GET", "/v1/keys", Some(&old_session), None);
    let login = server.call(
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({"username": "admin", "password": new_password})),
    );

    assert!(init.status.success(), "{init:?}");
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("already exists"),
        "{again:?}"
    );
    assert!(!weak.status.success());
    assert!(reset.status.success(), "{reset:?}");
    assert_eq!(revoked["error"]["code"], "UNAUTHORIZED");
    assert!(login["data"]["token"].is_string(), "{login}");
}

#[test]
fn openapi_prints_the_contract_and_detects_drift() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stale = dir.path().join("openapi.json");
    std::fs::write(&stale, "{}\n").expect("write");

    let printed = command(dir.path(), "127.0.0.1:1", &["openapi"]);
    let drift = command(
        dir.path(),
        "127.0.0.1:1",
        &["openapi", "--check", stale.to_str().expect("utf-8")],
    );

    assert!(String::from_utf8_lossy(&printed.stdout).contains("\"openapi\": \"3.1"));
    assert!(!drift.status.success());
}

#[test]
fn starts_even_when_the_data_dir_is_too_long_for_the_admin_socket() {
    let dir = tempfile::tempdir().expect("temp dir");
    let long = dir
        .path()
        .join("a-data-directory-whose-path-is-longer-than-a-unix-socket-path-may-be");
    std::fs::create_dir_all(&long).expect("create");

    let server = Server::start(&long);
    let token = server.token();
    server.call(
        "POST",
        "/v1/collections",
        Some(&token),
        Some(json!({"name": "orders"})),
    );
    let next = server.call("POST", "/v1/collections/orders/next", Some(&token), None);

    assert_eq!(next["data"], json!({"id": 1}));
}

/// Read `child`'s stderr until a line contains `needle`, or panic after `STARTUP`.
fn wait_for_stderr_line(child: &mut Child, needle: &str) -> String {
    use std::io::{BufRead, BufReader};
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                return;
            }
        }
    });
    let deadline = Instant::now() + STARTUP;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(left) {
            Ok(line) if line.contains(needle) => return line,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    panic!("idbseqd never printed a line containing {needle:?}");
}

#[test]
fn a_first_start_with_the_console_prints_a_setup_link_that_creates_the_admin() {
    let dir = tempfile::tempdir().expect("temp dir");
    let bind = free_address();
    let mut child = Command::new(BIN)
        .arg("--ui")
        .env("IDBSEQ_DATA_DIR", dir.path())
        .env("IDBSEQ_BIND", &bind)
        .env("IDBSEQ_LOG", "error")
        .env_remove("IDBSEQ_ADMIN_PASSWORD")
        .env_remove("IDBSEQ_CONFIG")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn idbseqd");

    let line = wait_for_stderr_line(&mut child, "/ui/#/setup?token=");
    let prefix = format!("http://{bind}/ui/#/setup?token=");
    let token = line
        .trim()
        .strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("unexpected link line: {line}"));
    let http = Client::new();
    let base = format!("http://{bind}");
    let deadline = Instant::now() + STARTUP;
    while http.get(format!("{base}/healthz")).send().is_err() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    let reply: Value = http
        .post(format!("{base}/v1/auth/setup"))
        .json(&json!({"token": token, "password": PASSWORD}))
        .send()
        .expect("setup")
        .json()
        .expect("json");
    let page = http.get(format!("{base}/ui/")).send().expect("console");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(reply["success"], json!(true), "{reply}");
    assert!(reply["data"]["token"].is_string());
    assert!(page.status().is_success(), "--ui serves the console");
}
