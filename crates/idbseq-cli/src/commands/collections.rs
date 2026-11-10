//! `idbseq collections …`

use serde_json::{Map, Value, json};

use super::{Context, Outcome, collection_path, items};
use crate::cli::{CollectionSettings, CollectionsCommand, DurabilityMode, Page};
use crate::error::CliError;
use crate::format::{durability, field, page_footer, pairs, table};
use crate::http::{Client, Request};
use crate::prompt::confirm;

pub fn run(command: CollectionsCommand, ctx: &mut Context<'_>) -> Result<Outcome, CliError> {
    match command {
        CollectionsCommand::List { page, prefix, all } => {
            list(&ctx.client, &page, prefix.as_deref(), all)
        }
        CollectionsCommand::Create {
            name,
            settings,
            protected,
        } => {
            let body = settings_body(Some(&name), &settings, protected.then_some(true))?;
            let data = ctx
                .client
                .send(&Request::post("/v1/collections").body(body))?
                .data;
            Ok(Outcome::new(
                data.clone(),
                format!("Created collection\n{}", describe(&data)),
            ))
        }
        CollectionsCommand::Info { name } => {
            let data = ctx
                .client
                .send(&Request::get(collection_path(&name, "")?))?
                .data;
            Ok(Outcome::new(data.clone(), describe(&data)))
        }
        CollectionsCommand::Update {
            name,
            settings,
            protected,
        } => {
            let body = settings_body(None, &settings, protected)?;
            let request = Request::patch(collection_path(&name, "")?).body(body);
            let data = ctx.client.send(&request)?.data;
            Ok(Outcome::new(
                data.clone(),
                format!("Updated collection\n{}", describe(&data)),
            ))
        }
        CollectionsCommand::Delete {
            name,
            confirm: flag,
        } => {
            let path = collection_path(&name, "")?;
            if !flag.yes {
                let action =
                    format!("Delete collection `{name}` and its counter? This cannot be undone.");
                confirm(&mut *ctx.prompt, &action, &name)?;
            }
            let data = ctx
                .client
                .send(&Request::delete(path).query("confirm", Some(&name)))?
                .data;
            let text = format!(
                "Deleted collection `{name}` (last_id {})",
                field(&data, "last_id")
            );
            Ok(Outcome::new(data, text))
        }
    }
}

fn list(
    client: &Client,
    page: &Page,
    prefix: Option<&str>,
    all: bool,
) -> Result<Outcome, CliError> {
    let fetch = |cursor: Option<String>| {
        client.send(
            &Request::get("/v1/collections")
                .query("limit", page.limit)
                .query("cursor", cursor)
                .query("prefix", prefix),
        )
    };
    let first = fetch(page.cursor.clone())?;
    let (collected, meta) = if all {
        let mut collected = items(&first.data).to_vec();
        let mut cursor = next_cursor(&first.meta);
        let mut meta = first.meta;
        while let Some(next) = cursor {
            let page = fetch(Some(next))?;
            collected.extend_from_slice(items(&page.data));
            cursor = next_cursor(&page.meta);
            meta = page.meta;
        }
        (collected, meta)
    } else {
        (items(&first.data).to_vec(), first.meta)
    };
    let rendered = table(
        &["NAME", "LAST_ID", "NEXT_ID", "PROTECTED", "DURABILITY"],
        &collected,
        |c| {
            vec![
                field(c, "name"),
                field(c, "last_id"),
                field(c, "next_id"),
                field(c, "protected"),
                durability(c.get("durability")),
            ]
        },
    );
    let text = format!(
        "{rendered}\n{}",
        page_footer(collected.len(), &meta, "collections")
    );
    Ok(Outcome::new(Value::Array(collected), text))
}

fn next_cursor(meta: &Value) -> Option<String> {
    meta.get("next_cursor")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn durability_body(settings: &CollectionSettings) -> Result<Option<Value>, CliError> {
    match (settings.durability, settings.block_size) {
        (None, _) => Ok(None),
        (Some(DurabilityMode::Strict), None) => Ok(Some(json!({ "mode": "strict" }))),
        (Some(DurabilityMode::Block), Some(size)) => {
            Ok(Some(json!({ "mode": "block", "size": size })))
        }
        (Some(DurabilityMode::Strict), Some(_)) => Err(CliError::Usage(
            "--block-size only applies to --durability block".to_owned(),
        )),
        (Some(DurabilityMode::Block), None) => Err(CliError::Usage(
            "--durability block needs --block-size".to_owned(),
        )),
    }
}

/// The JSON body for create (with a name) or update (without).
fn settings_body(
    name: Option<&str>,
    settings: &CollectionSettings,
    protected: Option<bool>,
) -> Result<Value, CliError> {
    let fields = [
        ("name", name.map(Value::from)),
        ("start", settings.start.map(Value::from)),
        ("step", settings.step.map(Value::from)),
        ("max", settings.max.map(Value::from)),
        ("protected", protected.map(Value::from)),
        ("durability", durability_body(settings)?),
    ];
    let body: Map<String, Value> = fields
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    Ok(Value::Object(body))
}

/// One collection as aligned `field: value` lines.
pub fn describe(collection: &Value) -> String {
    let next = match collection.get("next_id") {
        Some(Value::Null) => "- (exhausted)".to_owned(),
        _ => field(collection, "next_id"),
    };
    pairs(&[
        ("name", field(collection, "name")),
        ("start", field(collection, "start")),
        ("step", field(collection, "step")),
        ("max", field(collection, "max")),
        ("protected", field(collection, "protected")),
        ("durability", durability(collection.get("durability"))),
        ("created_at", field(collection, "created_at")),
        ("last_id", field(collection, "last_id")),
        ("next_id", next),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(durability: Option<DurabilityMode>, block_size: Option<u64>) -> CollectionSettings {
        CollectionSettings {
            start: Some(10),
            step: None,
            max: None,
            durability,
            block_size,
        }
    }

    #[test]
    fn builds_create_and_update_bodies() {
        let create = settings_body(
            Some("orders"),
            &settings(Some(DurabilityMode::Block), Some(5)),
            Some(true),
        );
        let update = settings_body(None, &settings(None, None), None);

        assert_eq!(
            create.unwrap(),
            json!({ "name": "orders", "start": 10, "protected": true, "durability": { "mode": "block", "size": 5 } })
        );
        assert_eq!(update.unwrap(), json!({ "start": 10 }));
    }

    #[test]
    fn rejects_inconsistent_durability_flags() {
        assert!(settings_body(None, &settings(Some(DurabilityMode::Block), None), None).is_err());
        assert!(
            settings_body(None, &settings(Some(DurabilityMode::Strict), Some(3)), None).is_err()
        );
        assert_eq!(
            settings_body(None, &settings(Some(DurabilityMode::Strict), None), None).unwrap()["durability"],
            json!({ "mode": "strict" })
        );
    }

    #[test]
    fn describes_an_exhausted_collection() {
        let text = describe(&json!({ "name": "a", "next_id": null, "last_id": 5 }));
        assert!(text.contains("next_id:    - (exhausted)"), "{text}");
    }
}
