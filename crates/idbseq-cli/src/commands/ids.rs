//! `idbseq next`, `idbseq last`, `idbseq reset`.

use serde_json::{Map, Value};

use super::{Context, Outcome, collection_path};
use crate::error::CliError;
use crate::format::{field, pairs};
use crate::http::{Client, Request};
use crate::prompt::confirm;

pub fn next(
    client: &Client,
    name: &str,
    count: Option<u32>,
    idempotency_key: Option<String>,
) -> Result<Outcome, CliError> {
    let request = Request::post(collection_path(name, "/next")?)
        .query("count", count)
        .header("Idempotency-Key", idempotency_key);
    let data = client.send(&request)?.data;
    let text = if count.is_some() {
        format!(
            "{}..{} (count {}, step {})",
            field(&data, "first"),
            field(&data, "last"),
            field(&data, "count"),
            field(&data, "step")
        )
    } else {
        field(&data, "id")
    };
    Ok(Outcome::new(data, text))
}

pub fn last(client: &Client, name: &str) -> Result<Outcome, CliError> {
    let data = client
        .send(&Request::get(collection_path(name, "/last")?))?
        .data;
    let text = pairs(&[
        ("last_id", field(&data, "last_id")),
        ("next_id", field(&data, "next_id")),
    ]);
    Ok(Outcome::new(data, text))
}

pub fn reset(
    ctx: &mut Context<'_>,
    name: &str,
    value: Option<i64>,
    allow_rewind: bool,
    yes: bool,
) -> Result<Outcome, CliError> {
    let path = collection_path(name, "/reset")?;
    if !yes {
        let target = value.map_or_else(|| "its start".to_owned(), |v| v.to_string());
        let action = format!("Reset collection `{name}` so the next ID is {target}?");
        confirm(&mut *ctx.prompt, &action, name)?;
    }
    let fields = [
        ("confirm", Some(Value::from(name))),
        ("value", value.map(Value::from)),
        ("allow_rewind", allow_rewind.then_some(Value::Bool(true))),
    ];
    let body: Map<String, Value> = fields
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    let data = ctx
        .client
        .send(&Request::post(path).body(Value::Object(body)))?
        .data;
    let text = format!(
        "Reset `{name}`: previous last_id {}, next ID {}",
        field(&data, "previous_last_id"),
        field(&data, "next_id")
    );
    Ok(Outcome::new(data, text))
}
