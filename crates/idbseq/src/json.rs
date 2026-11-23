//! A JSON interface to a [`Database`], for bindings to other languages.
//!
//! [`call`] takes an operation name and its arguments as JSON and returns the
//! same envelope as the HTTP API: `{"data": …}` (plus `"meta"` for a list) or
//! `{"error": {"code", "message", "details"}}`, with the same field names and
//! error codes. A client that reads the HTTP API can read this unchanged.
//!
//! | `op`     | Arguments                                                        | `data`              |
//! | -------- | ---------------------------------------------------------------- | ------------------- |
//! | `create` | `name`, and optional `start`, `step`, `max`, `protected`, `durability` | a collection  |
//! | `get`    | `name`                                                           | a collection        |
//! | `list`   | optional `limit`, `cursor`, `prefix`                             | collections + `meta` |
//! | `update` | `name`, and any of `start`, `step`, `max`, `protected`, `durability` | a collection    |
//! | `reset`  | `name`, optional `value`, `allow_rewind`                         | previous, last and next ID |
//! | `delete` | `name`                                                           | the deleted collection |
//! | `last`   | `name`                                                           | `last_id`, `next_id` |
//! | `next`   | `name`, optional `count`, `idempotency_key`                      | `{id}`, or a batch with `count` |

use std::num::{NonZeroU32, NonZeroU64};
use std::time::SystemTime;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{
    Collection, CollectionPatch, CollectionSpec, Database, Durability, Error, Id, IssuedRange,
    NextRequest, ValidationErrors,
};
use idbseq_core::store::StoreError;

/// Largest `step`: the API declares it an int64.
const MAX_STEP: u64 = i64::MAX.unsigned_abs();

/// Run `op` with `args` (a JSON object) and return the response envelope.
#[must_use]
pub fn call(db: &Database, op: &str, args: &str) -> String {
    let reply = dispatch(db, op, args).map_or_else(Failure::into_envelope, |(data, meta)| {
        if let Some(meta) = meta {
            json!({ "data": data, "meta": meta })
        } else {
            json!({ "data": data })
        }
    });
    reply.to_string()
}

/// The error envelope for a failure outside [`call`], such as opening the file.
#[must_use]
pub fn error(error: &Error) -> String {
    Failure::from(error).into_envelope().to_string()
}

/// An error envelope with a code of the caller's own, such as `DATABASE_CLOSED`.
#[must_use]
pub fn error_with_code(code: &str, message: &str) -> String {
    json!({ "error": { "code": code, "message": message, "details": null } }).to_string()
}

/// `{"data": null}`, for calls that return nothing.
#[must_use]
pub fn empty() -> String {
    json!({ "data": null }).to_string()
}

type Reply = (Value, Option<Value>);

fn dispatch(db: &Database, op: &str, args: &str) -> Result<Reply, Failure> {
    let data = match op {
        "create" => collection(&db.create_collection(parse::<CreateArgs>(args)?.into_spec()?)?),
        "get" => collection(&db.collection(&parse::<NameArgs>(args)?.name)?),
        "list" => return list(db, parse(args)?),
        "update" => {
            let args = parse::<UpdateArgs>(args)?;
            let name = args.name.clone();
            collection(&db.update_collection(&name, &args.patch()?)?)
        }
        "reset" => {
            let ResetArgs {
                name,
                value,
                allow_rewind,
            } = parse(args)?;
            let reset = db.reset_collection(&name, value, allow_rewind)?;
            json!({
                "previous_last_id": reset.previous_last_id,
                "last_id": reset.last_id,
                "next_id": reset.next_id,
            })
        }
        "delete" => collection(&db.delete_collection(&parse::<NameArgs>(args)?.name)?),
        "last" => {
            let last = db.last_id(&parse::<NameArgs>(args)?.name)?;
            json!({ "last_id": last.last_id, "next_id": last.next_id })
        }
        "next" => next(db, parse(args)?)?,
        other => {
            return Err(Failure::invalid(
                "op",
                &format!("unknown operation `{other}`"),
            ));
        }
    };
    Ok((data, None))
}

fn list(db: &Database, args: ListArgs) -> Result<Reply, Failure> {
    let page = db.collections(args.limit, args.cursor, args.prefix)?;
    let limit = args
        .limit
        .unwrap_or(idbseq_core::engine::DEFAULT_PAGE_LIMIT);
    let items = page.items.iter().map(collection).collect::<Vec<_>>();
    let meta = json!({ "total": page.total, "limit": limit, "next_cursor": page.next_cursor });
    Ok((Value::Array(items), Some(meta)))
}

fn next(db: &Database, args: NextArgs) -> Result<Value, Failure> {
    let issued = db.issue(&NextRequest {
        name: args.name,
        count: args.count,
        idempotency_key: args.idempotency_key,
    })?;
    Ok(if args.count.is_some() {
        batch(issued.range)
    } else {
        json!({ "id": issued.range.first })
    })
}

fn collection(c: &Collection) -> Value {
    let spec = c.spec();
    json!({
        "name": spec.name,
        "start": spec.start,
        "step": spec.step.get(),
        "max": spec.max,
        "protected": spec.protected,
        "durability": durability(spec.durability),
        "created_at": rfc3339(c.created_at()),
        "last_id": c.last_id(),
        "next_id": c.next_id(),
    })
}

fn durability(d: Durability) -> Value {
    match d {
        Durability::Strict => json!({ "mode": "strict" }),
        Durability::Block { size } => json!({ "mode": "block", "size": size.get() }),
    }
}

fn batch(range: IssuedRange) -> Value {
    json!({ "first": range.first, "last": range.last, "count": range.count.get(), "step": range.step.get() })
}

/// `2026-11-10T08:15:30Z`-style UTC timestamp (whole seconds), as the server writes it.
fn rfc3339(at: SystemTime) -> String {
    // Checked: a damaged file can hold any time, and `OffsetDateTime::from`
    // panics outside the years -9999..=9999.
    let seconds = match at.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs()).ok().map(|s| -s),
    };
    seconds
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
        .and_then(|at| at.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned())
}

// ── Arguments ───────────────────────────────────────────────────────────

fn parse<T: DeserializeOwned>(args: &str) -> Result<T, Failure> {
    serde_json::from_str(args).map_err(|e| Failure::invalid("args", &e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    limit: Option<usize>,
    cursor: Option<String>,
    prefix: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NextArgs {
    name: String,
    count: Option<u32>,
    idempotency_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetArgs {
    name: String,
    value: Option<Id>,
    #[serde(default)]
    allow_rewind: bool,
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum DurabilityArgs {
    Strict,
    Block { size: u32 },
}

impl DurabilityArgs {
    fn into_core(self) -> Result<Durability, ValidationErrors> {
        match self {
            Self::Strict => Ok(Durability::Strict),
            Self::Block { size } => NonZeroU32::new(size)
                .map(|size| Durability::Block { size })
                .ok_or_else(|| {
                    ValidationErrors::field("durability", "block size must be greater than 0")
                }),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    name: String,
    start: Option<Id>,
    step: Option<u64>,
    max: Option<Id>,
    protected: Option<bool>,
    durability: Option<DurabilityArgs>,
}

impl CreateArgs {
    fn into_spec(self) -> Result<CollectionSpec, ValidationErrors> {
        let defaults = CollectionSpec::named(self.name);
        Ok(CollectionSpec {
            start: self.start.unwrap_or(defaults.start),
            step: step_of(self.step)?.unwrap_or(defaults.step),
            max: self.max.unwrap_or(defaults.max),
            protected: self.protected.unwrap_or(defaults.protected),
            durability: self
                .durability
                .map(DurabilityArgs::into_core)
                .transpose()?
                .unwrap_or_default(),
            ..defaults
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    name: String,
    start: Option<Id>,
    step: Option<u64>,
    max: Option<Id>,
    protected: Option<bool>,
    durability: Option<DurabilityArgs>,
}

impl UpdateArgs {
    fn patch(self) -> Result<CollectionPatch, ValidationErrors> {
        Ok(CollectionPatch {
            start: self.start,
            step: step_of(self.step)?,
            max: self.max,
            protected: self.protected,
            durability: self.durability.map(DurabilityArgs::into_core).transpose()?,
        })
    }
}

fn step_of(step: Option<u64>) -> Result<Option<NonZeroU64>, ValidationErrors> {
    step.map(|s| {
        if s > MAX_STEP {
            return Err(ValidationErrors::field(
                "step",
                format!("must be at most {MAX_STEP}"),
            ));
        }
        NonZeroU64::new(s).ok_or_else(|| ValidationErrors::field("step", "must be greater than 0"))
    })
    .transpose()
}

// ── Errors ──────────────────────────────────────────────────────────────

/// An error envelope's `code`, `message` and `details`.
struct Failure {
    code: &'static str,
    message: String,
    details: Value,
}

impl Failure {
    fn invalid(field: &str, problem: &str) -> Self {
        Self::from_validation(&ValidationErrors::field(field, problem))
    }

    fn from_validation(errors: &ValidationErrors) -> Self {
        Self {
            code: "VALIDATION_FAILED",
            message: errors.to_string(),
            details: json!({ "fields": errors.fields() }),
        }
    }

    fn plain(code: &'static str, message: String) -> Self {
        Self {
            code,
            message,
            details: Value::Null,
        }
    }

    fn into_envelope(self) -> Value {
        json!({ "error": { "code": self.code, "message": self.message, "details": self.details } })
    }
}

impl From<ValidationErrors> for Failure {
    fn from(errors: ValidationErrors) -> Self {
        Self::from_validation(&errors)
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::from(&error)
    }
}

impl From<&Error> for Failure {
    fn from(error: &Error) -> Self {
        let message = error.to_string();
        match error {
            // `From<StoreError>` turns the store's busy and unavailable errors
            // into `Error::Busy` / `Error::Open`; a raw one maps the same way.
            Error::Busy { .. } | Error::Store(StoreError::Busy { .. }) => {
                Self::plain("DATABASE_BUSY", message)
            }
            Error::Open(_) | Error::Store(StoreError::Unavailable(_)) => {
                Self::plain("DATABASE_UNAVAILABLE", message)
            }
            Error::Store(StoreError::NotFound(_)) => Self::plain("COLLECTION_NOT_FOUND", message),
            Error::Store(StoreError::AlreadyExists(_)) => Self::plain("COLLECTION_EXISTS", message),
            Error::Store(StoreError::Exhausted(_)) => Self::plain("SEQUENCE_EXHAUSTED", message),
            Error::Store(StoreError::IdempotencyMismatch) => Self::invalid(
                "idempotency_key",
                "was already used for a different request",
            ),
            Error::Store(StoreError::Invalid(errors)) => Self::from_validation(errors),
            Error::Store(StoreError::Backend(_)) => Self::plain("INTERNAL", message),
        }
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
