//! `idbseq keys …`, `idbseq login`, `idbseq logout`, `idbseq audit`.

use serde_json::{Value, json};

use super::{Context, Outcome, items};
use crate::cli::{KeysCommand, Page, Scope};
use crate::config::{Session, load_session, remove_session, save_session};
use crate::error::CliError;
use crate::format::{field, page_footer, table};
use crate::http::{Client, Request, encode};
use crate::prompt::confirm;

pub fn keys(command: KeysCommand, ctx: &mut Context<'_>) -> Result<Outcome, CliError> {
    match command {
        KeysCommand::Create {
            name,
            scopes,
            collections,
        } => {
            let scopes: Vec<&str> = scopes.into_iter().map(Scope::as_str).collect();
            let body = if collections.is_empty() {
                json!({ "name": name, "scopes": scopes })
            } else {
                json!({ "name": name, "scopes": scopes, "collections": collections })
            };
            let data = ctx.client.send(&Request::post("/v1/keys").body(body))?.data;
            let text = format!(
                "Created key {} ({}), scopes {}, collections {}\nKey: {}\nStore it now; it is not shown again.",
                field(&data, "id"),
                field(&data, "name"),
                field(&data, "scopes"),
                field(&data, "collections"),
                field(&data, "key"),
            );
            Ok(Outcome::new(data, text))
        }
        KeysCommand::List => {
            let data = ctx.client.send(&Request::get("/v1/keys"))?.data;
            let text = table(
                &[
                    "ID",
                    "NAME",
                    "SCOPES",
                    "COLLECTIONS",
                    "CREATED_AT",
                    "LAST_USED_AT",
                ],
                items(&data),
                |key| {
                    [
                        "id",
                        "name",
                        "scopes",
                        "collections",
                        "created_at",
                        "last_used_at",
                    ]
                    .iter()
                    .map(|name| field(key, name))
                    .collect()
                },
            );
            Ok(Outcome::new(data, text))
        }
        KeysCommand::Revoke { id, confirm: flag } => {
            if id.is_empty() {
                return Err(CliError::Usage("the key id must not be empty".to_owned()));
            }
            if !flag.yes {
                let action =
                    format!("Revoke API key `{id}`? Clients using it stop working immediately.");
                confirm(&mut *ctx.prompt, &action, &id)?;
            }
            let data = ctx
                .client
                .send(&Request::delete(format!("/v1/keys/{}", encode(&id))))?
                .data;
            Ok(Outcome::new(data, format!("Revoked key {id}")))
        }
    }
}

fn require_config_dir(ctx: &Context<'_>) -> Result<std::path::PathBuf, CliError> {
    ctx.config_dir.clone().ok_or_else(|| {
        CliError::Usage("cannot find a config directory: set IDBSEQ_CONFIG_DIR or HOME".to_owned())
    })
}

pub fn login(ctx: &mut Context<'_>, username: &str) -> Result<Outcome, CliError> {
    let dir = require_config_dir(ctx)?;
    let password = ctx.prompt.secret(&format!("Password for {username}: "))?;
    let request = Request::post("/v1/auth/login")
        .anonymous()
        .body(json!({ "username": username, "password": password }));
    let data = ctx.client.send(&request)?.data;
    let session = Session {
        url: ctx.client.base_url().to_owned(),
        token: data
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::Unexpected {
                status: 200,
                message: "the login response has no token".to_owned(),
            })?
            .to_owned(),
        expires_at: field(&data, "expires_at"),
    };
    save_session(&dir, &session)?;
    let text = format!(
        "Logged in to {} as {username}; the session expires at {}",
        session.url, session.expires_at
    );
    Ok(Outcome::new(data, text))
}

pub fn logout(ctx: &mut Context<'_>) -> Result<Outcome, CliError> {
    let dir = require_config_dir(ctx)?;
    let Some(session) = load_session(&dir)? else {
        return Ok(Outcome::new(Value::Null, "Not logged in."));
    };
    let client = Client::new(&session.url, Some(session.token.clone()), ctx.timeout)?;
    match client.send(&Request::post("/v1/auth/logout")) {
        // An expired or already revoked session is as good as logged out.
        Ok(_) | Err(CliError::Api { status: 401, .. }) => {}
        Err(error) => return Err(error),
    }
    remove_session(&dir)?;
    Ok(Outcome::new(
        Value::Null,
        format!("Logged out of {}", session.url),
    ))
}

pub fn audit(
    client: &Client,
    page: &Page,
    collection: Option<String>,
    action: Option<String>,
) -> Result<Outcome, CliError> {
    let request = Request::get("/v1/audit")
        .query("limit", page.limit)
        .query("cursor", page.cursor.clone())
        .query("collection", collection)
        .query("action", action);
    let response = client.send(&request)?;
    let entries = items(&response.data);
    let rendered = table(
        &["AT", "ACTOR", "ACTION", "COLLECTION", "REQUEST_ID"],
        entries,
        |entry| {
            ["at", "actor", "action", "collection", "request_id"]
                .iter()
                .map(|name| field(entry, name))
                .collect()
        },
    );
    let text = format!(
        "{rendered}\n{}",
        page_footer(entries.len(), &response.meta, "entries")
    );
    Ok(Outcome::new(response.data, text))
}
