//! HTTP latency report for `POST /v1/collections/{name}/next` (US-0017, US-0038).
//!
//! By default it starts `idbseqd`'s router in-process on a loopback port over a
//! temp data dir (redb, real fsync, rate limiting off) and drives it with
//! 1, 16 and 64 concurrent HTTP/1.1 keep-alive clients, for a `strict` and a
//! `block` (size 1000) collection. It prints requests/s and p50/p99/max
//! latency as a Markdown table.
//!
//! To load a server on another machine instead, point it there:
//!
//! ```sh
//! IDBSEQ_BENCH_URL=http://10.0.0.5:7878 IDBSEQ_BENCH_ADMIN_PASSWORD=… \
//!   cargo bench -p idbseq-server --bench http_next
//! ```
//!
//! The remote server's `rate_limit.per_second` must be 0 (off) or high enough,
//! or requests fail with 429. `IDBSEQ_BENCH_SECS` sets the time per scenario.

#![allow(clippy::expect_used)] // bench setup: a failed step should abort the run
#![allow(clippy::cast_precision_loss)] // reporting rates as f64 is fine

use std::net::SocketAddr;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use idbseq_server::config::{Config, RateLimitConfig};
use reqwest::blocking::Client;
use serde_json::{Value, json};

const DEFAULT_SECS: u64 = 3;
const CALLERS: [usize; 3] = [1, 16, 64];
const LOCAL_ADMIN_PASSWORD: &str = "benchmark admin password";
const BLOCK_SIZE: u32 = 1_000;

/// An in-process server; dropping it stops serving and closes the store.
struct LocalServer {
    url: String,
    _runtime: tokio::runtime::Runtime,
    _dir: tempfile::TempDir,
}

fn start_local() -> LocalServer {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        rate_limit: RateLimitConfig {
            per_second: 0,
            burst: 0,
        },
        ..Config::default()
    };
    let (state, _store) =
        idbseq_server::serve::open_state(&config, Some(LOCAL_ADMIN_PASSWORD)).expect("open state");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let app = idbseq_server::app::router(state).into_make_service_with_connect_info::<SocketAddr>();
    runtime.spawn(async move { axum::serve(listener, app).await });
    LocalServer {
        url: format!("http://{addr}"),
        _runtime: runtime,
        _dir: dir,
    }
}

fn call(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    body: Option<&Value>,
) -> Value {
    let request = client.request(method, url).bearer_auth(token);
    let request = match body {
        Some(body) => request.json(body),
        None => request,
    };
    let response = request.send().expect("request");
    let status = response.status();
    let envelope: Value = response.json().expect("JSON envelope");
    assert!(status.is_success(), "{url}: HTTP {status}: {envelope}");
    envelope["data"].clone()
}

/// Admin token, a `generate` key and one collection per durability mode.
struct Setup {
    admin: String,
    key: String,
    key_id: String,
    strict: String,
    block: String,
}

fn setup(client: &Client, url: &str, password: &str) -> Setup {
    let login = client
        .post(format!("{url}/v1/auth/login"))
        .json(&json!({ "username": "admin", "password": password }))
        .send()
        .expect("login");
    assert!(
        login.status().is_success(),
        "login failed: HTTP {}",
        login.status()
    );
    let admin = login.json::<Value>().expect("login JSON")["data"]["token"]
        .as_str()
        .expect("token")
        .to_owned();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let key = call(
        client,
        reqwest::Method::POST,
        &format!("{url}/v1/keys"),
        &admin,
        Some(&json!({ "name": format!("bench-{suffix}"), "scopes": ["generate"] })),
    );
    let strict = format!("bench-{suffix}-strict");
    let block = format!("bench-{suffix}-block");
    for (name, durability) in [
        (&strict, json!({ "mode": "strict" })),
        (&block, json!({ "mode": "block", "size": BLOCK_SIZE })),
    ] {
        call(
            client,
            reqwest::Method::POST,
            &format!("{url}/v1/collections"),
            &admin,
            Some(&json!({ "name": name, "durability": durability })),
        );
    }
    Setup {
        admin,
        key: key["key"].as_str().expect("key").to_owned(),
        key_id: key["id"].as_str().expect("key id").to_owned(),
        strict,
        block,
    }
}

fn cleanup(client: &Client, url: &str, setup: &Setup) {
    for name in [&setup.strict, &setup.block] {
        call(
            client,
            reqwest::Method::DELETE,
            &format!("{url}/v1/collections/{name}?confirm={name}"),
            &setup.admin,
            None,
        );
    }
    call(
        client,
        reqwest::Method::DELETE,
        &format!("{url}/v1/keys/{}", setup.key_id),
        &setup.admin,
        None,
    );
}

struct Sample {
    requests: usize,
    elapsed: Duration,
    sorted: Vec<Duration>,
}

fn caller(url: &str, key: &str, start: &Barrier, duration: Duration) -> Vec<Duration> {
    let client = Client::new();
    start.wait();
    let began = Instant::now();
    let mut latencies = Vec::new();
    while began.elapsed() < duration {
        let sent = Instant::now();
        let response = client.post(url).bearer_auth(key).send().expect("POST next");
        let status = response.status();
        let _ = response.bytes().expect("body");
        assert!(status.is_success(), "POST next: HTTP {status}");
        latencies.push(sent.elapsed());
    }
    latencies
}

fn drive(url: &str, key: &str, callers: usize, duration: Duration) -> Sample {
    let start = Arc::new(Barrier::new(callers + 1));
    let (mut sorted, elapsed) = thread::scope(|scope| {
        let workers: Vec<_> = (0..callers)
            .map(|_| {
                let start = Arc::clone(&start);
                scope.spawn(move || caller(url, key, &start, duration))
            })
            .collect();
        start.wait();
        let began = Instant::now();
        let all: Vec<Duration> = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("caller thread"))
            .collect();
        (all, began.elapsed())
    });
    sorted.sort();
    Sample {
        requests: sorted.len(),
        elapsed,
        sorted,
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // rank is within 1..=len
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1e3)
}

fn report(url: &str, password: &str, duration: Duration) {
    let client = Client::new();
    let setup = setup(&client, url, password);
    println!("| collection | callers | requests/s | p50 ms | p99 ms | max ms |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: |");
    for (label, name) in [("strict", &setup.strict), ("block-1000", &setup.block)] {
        let next = format!("{url}/v1/collections/{name}/next");
        for callers in CALLERS {
            let sample = drive(&next, &setup.key, callers, duration);
            println!(
                "| {label} | {callers} | {:.0} | {} | {} | {} |",
                sample.requests as f64 / sample.elapsed.as_secs_f64(),
                ms(percentile(&sample.sorted, 0.50)),
                ms(percentile(&sample.sorted, 0.99)),
                ms(sample.sorted.last().copied().unwrap_or_default()),
            );
        }
    }
    cleanup(&client, url, &setup);
}

fn main() {
    let secs = std::env::var("IDBSEQ_BENCH_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SECS);
    let duration = Duration::from_secs(secs);
    if let Ok(url) = std::env::var("IDBSEQ_BENCH_URL") {
        let password = std::env::var("IDBSEQ_BENCH_ADMIN_PASSWORD")
            .expect("IDBSEQ_BENCH_ADMIN_PASSWORD is required with IDBSEQ_BENCH_URL");
        println!("Target: {url} (remote), {secs} s per scenario.\n");
        report(url.trim_end_matches('/'), &password, duration);
    } else {
        let server = start_local();
        println!(
            "Target: in-process idbseqd on {} (loopback), {secs} s per scenario.\n",
            server.url
        );
        report(&server.url, LOCAL_ADMIN_PASSWORD, duration);
    }
}
