//! Node.js binding for embedded idbseq.
//!
//! One class with one async method: `call(op, args)` runs [`idbseq::json::call`]
//! and resolves to its JSON envelope. The TypeScript wrapper in the `idbseq`
//! npm package (`IdbSeqDatabase`) decodes the envelope with the same code as
//! HTTP responses. Calls never reject: every failure is an error envelope,
//! a panic included (a damaged file must not take the app down with it).
//!
//! Calls run on Tokio's blocking threads, not on Node's own thread pool: a
//! call may wait seconds for a file another process holds, and Node's four
//! pool threads also serve the app's file reads, DNS lookups and crypto.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use idbseq::{Database, Mode, Options, json};
use napi::Env;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;

/// The handle's life: not opened yet, open, or closed for good.
enum State {
    New,
    Open(Arc<Database>),
    Closed,
}

#[napi]
pub struct NativeDatabase {
    path: String,
    options: Options,
    state: Arc<Mutex<State>>,
}

#[napi]
impl NativeDatabase {
    /// Nothing is opened until `call("open", "{}")`.
    #[napi(constructor)]
    #[must_use]
    pub fn new(path: String, shared: bool, busy_timeout_ms: u32) -> Self {
        let mode = if shared {
            Mode::Shared
        } else {
            Mode::Exclusive
        };
        let options = Options {
            mode,
            busy_timeout: Duration::from_millis(u64::from(busy_timeout_ms)),
        };
        Self {
            path,
            options,
            state: Arc::new(Mutex::new(State::New)),
        }
    }

    /// Run `op` with JSON `args`; resolves to a JSON envelope. Besides the
    /// operations of `idbseq::json`, `open` opens the file and `close` lets go.
    ///
    /// # Errors
    ///
    /// Only if the promise cannot be created.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn call<'env>(
        &self,
        env: &'env Env,
        op: String,
        args: String,
    ) -> napi::Result<PromiseRaw<'env, String>> {
        let call = Call {
            path: self.path.clone(),
            options: self.options,
            state: Arc::clone(&self.state),
            op,
            args,
        };
        env.spawn_future(async move {
            let envelope = napi::tokio::task::spawn_blocking(move || call.run())
                .await
                .unwrap_or_else(|failed| {
                    json::error_with_code("INTERNAL", &crash_message(&failed))
                });
            Ok(envelope)
        })
    }
}

/// What a call that panicked (or was cancelled) reports.
fn crash_message(failed: &napi::tokio::task::JoinError) -> String {
    if !failed.is_panic() {
        return "the call was cancelled".to_owned();
    }
    "idbseq failed unexpectedly; the file may be damaged (restore it from a backup)".to_owned()
}

struct Call {
    path: String,
    options: Options,
    state: Arc<Mutex<State>>,
    op: String,
    args: String,
}

impl Call {
    fn run(&self) -> String {
        match self.op.as_str() {
            "open" => self.open(),
            "close" => {
                // Calls already running keep their handle; the file closes after them.
                *self.lock() = State::Closed;
                json::empty()
            }
            op => match self.database() {
                Ok(db) => json::call(&db, op, &self.args),
                Err(envelope) => envelope,
            },
        }
    }

    fn open(&self) -> String {
        let mut state = self.lock();
        match *state {
            State::New => match Database::open_with(&self.path, self.options) {
                Ok(db) => {
                    *state = State::Open(Arc::new(db));
                    json::empty()
                }
                Err(error) => json::error(&error),
            },
            State::Open(_) => json::empty(),
            State::Closed => closed(),
        }
    }

    fn database(&self) -> Result<Arc<Database>, String> {
        match &*self.lock() {
            State::Open(db) => Ok(Arc::clone(db)),
            State::New => Err(json::error_with_code(
                "DATABASE_CLOSED",
                "the database is not open",
            )),
            State::Closed => Err(closed()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn closed() -> String {
    json::error_with_code("DATABASE_CLOSED", "the database is closed")
}
