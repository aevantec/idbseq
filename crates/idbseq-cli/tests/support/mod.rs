//! Test helpers: a canned-response HTTP server and an in-process CLI runner.

// Shared by several test crates; each uses a different subset.
#![allow(dead_code)]
// Test code: a failed setup step should panic.
#![allow(clippy::expect_used)]
// Helpers take `json!` values by value so call sites stay short.
#![allow(clippy::needless_pass_by_value)]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use clap::Parser;
use idbseq_cli::cli::Cli;
use idbseq_cli::error::CliError;
use idbseq_cli::prompt::Prompt;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    /// Path and query, e.g. `/v1/collections?limit=5`.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

#[derive(Debug, Clone)]
pub struct Reply {
    status: u16,
    body: String,
}

pub fn ok(data: Value) -> Reply {
    Reply {
        status: 200,
        body: json!({ "success": true, "data": data, "error": null }).to_string(),
    }
}

pub fn ok_page(data: Value, meta: Value) -> Reply {
    Reply {
        status: 200,
        body: json!({ "success": true, "data": data, "error": null, "meta": meta }).to_string(),
    }
}

pub fn raw(status: u16, body: &str) -> Reply {
    Reply {
        status,
        body: body.to_owned(),
    }
}

pub fn fail(status: u16, code: &str, message: &str) -> Reply {
    Reply {
        status,
        body: json!({ "success": false, "data": null, "error": { "code": code, "message": message, "details": null } })
            .to_string(),
    }
}

#[derive(Default)]
struct State {
    replies: VecDeque<Reply>,
    requests: Vec<Recorded>,
}

/// Answers each connection with the next queued reply and records the request.
pub struct MockServer {
    pub url: String,
    state: Arc<Mutex<State>>,
}

impl MockServer {
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let state = Arc::new(Mutex::new(State {
            replies: replies.into(),
            requests: Vec::new(),
        }));
        let shared = Arc::clone(&state);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle(stream, &shared);
            }
        });
        Self { url, state }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .requests
            .clone()
    }

    pub fn only_request(&self) -> Recorded {
        let requests = self.requests();
        assert_eq!(
            requests.len(),
            1,
            "expected exactly one request: {requests:?}"
        );
        requests[0].clone()
    }
}

fn handle(stream: TcpStream, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("header line");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("body");
    let reply = {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        state.requests.push(Recorded {
            method,
            target,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        });
        state
            .replies
            .pop_front()
            .unwrap_or_else(|| fail(599, "NO_REPLY_QUEUED", "the test queued no reply"))
    };
    let response = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nX-Request-Id: req-test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    );
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Answers prompts from a script and records what was asked.
#[derive(Default)]
pub struct Scripted {
    pub answers: VecDeque<String>,
    pub asked: Vec<String>,
}

impl Scripted {
    pub fn with(answers: &[&str]) -> Self {
        Self {
            answers: answers.iter().map(|a| (*a).to_owned()).collect(),
            asked: Vec::new(),
        }
    }
}

impl Prompt for Scripted {
    fn line(&mut self, message: &str) -> Result<String, CliError> {
        self.asked.push(message.to_owned());
        self.answers.pop_front().ok_or(CliError::Aborted)
    }

    fn secret(&mut self, message: &str) -> Result<String, CliError> {
        self.line(message)
    }
}

pub struct Run {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
    pub asked: Vec<String>,
}

/// Runs `idbseq <args>` in-process with `IDBSEQ_CONFIG_DIR=config_dir` (or no
/// config directory at all) and scripted prompt answers.
pub fn run_cli(args: &[&str], config_dir: Option<&Path>, answers: &[&str]) -> Run {
    let cli = Cli::try_parse_from(std::iter::once("idbseq").chain(args.iter().copied()))
        .expect("valid arguments");
    let dir = config_dir.map(|d| d.to_string_lossy().into_owned());
    let env = move |name: &str| {
        if name == "IDBSEQ_CONFIG_DIR" {
            dir.clone()
        } else {
            None
        }
    };
    let mut prompt = Scripted::with(answers);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = idbseq_cli::run(cli, &env, &mut prompt, &mut stdout, &mut stderr);
    Run {
        code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        asked: prompt.asked,
    }
}
