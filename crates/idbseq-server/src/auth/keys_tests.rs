use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::*;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000)
}

fn store() -> (KeyStore, Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(redb::Database::create(dir.path().join("t.redb")).expect("db"));
    crate::db::init(&db).expect("init");
    (KeyStore::load(Arc::clone(&db)).expect("load"), db, dir)
}

fn new_key(scopes: &[Scope], collections: &[&str]) -> NewKey {
    NewKey {
        name: "billing".into(),
        scopes: scopes.to_vec(),
        collections: collections.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn a_created_key_authenticates_and_its_secret_is_not_stored() {
    let (keys, db, _dir) = store();

    let (record, full) = keys
        .create(new_key(&[Scope::Read, Scope::Generate], &[]), now())
        .expect("create");
    let raw = crate::db::get::<serde_json::Value>(&db, API_KEYS, &record.id)
        .expect("get")
        .expect("stored");

    assert!(full.starts_with(&format!("idb_{}_", record.id)));
    assert_eq!(
        keys.authenticate(&full, now()).map(|k| k.id),
        Some(record.id.clone())
    );
    let secret = full.rsplit('_').next().expect("secret");
    assert!(!raw.to_string().contains(secret));
}

#[test]
fn wrong_or_malformed_tokens_do_not_authenticate() {
    let (keys, _db, _dir) = store();
    let (record, full) = keys
        .create(new_key(&[Scope::Read], &[]), now())
        .expect("create");

    let tampered = format!("{}x", &full[..full.len() - 1]);
    for token in [
        tampered.as_str(),
        "idb_nope_secret",
        "garbage",
        &format!("xyz_{}_abc", record.id),
    ] {
        assert!(keys.authenticate(token, now()).is_none(), "{token}");
    }
}

#[test]
fn revoked_keys_stop_working_immediately() {
    let (keys, _db, _dir) = store();
    let (record, full) = keys
        .create(new_key(&[Scope::Read], &[]), now())
        .expect("create");

    keys.revoke(&record.id).expect("revoke");

    assert!(keys.authenticate(&full, now()).is_none());
    assert!(matches!(
        keys.revoke(&record.id),
        Err(KeyError::NotFound(_))
    ));
    assert_eq!(keys.list(), Vec::new());
}

#[test]
fn last_used_is_reported_and_survives_a_flush_and_reload() {
    let (keys, db, _dir) = store();
    let (record, full) = keys
        .create(new_key(&[Scope::Read], &[]), now())
        .expect("create");
    keys.authenticate(&full, now() + Duration::from_secs(5))
        .expect("auth");

    let listed = keys.list();
    let flushed = keys.flush_last_used().expect("flush");
    let reloaded = KeyStore::load(db).expect("reload").list();

    assert_eq!(listed[0].last_used_at, Some(now() + Duration::from_secs(5)));
    assert_eq!(flushed, 1);
    assert_eq!(reloaded[0].id, record.id);
    assert_eq!(
        reloaded[0].last_used_at,
        Some(now() + Duration::from_secs(5))
    );
    assert_eq!(keys.flush_last_used().expect("flush"), 0);
}

#[test]
fn allow_lists_match_exact_names_and_prefixes() {
    let (keys, _db, _dir) = store();
    let (open, _) = keys
        .create(new_key(&[Scope::Read], &[]), now())
        .expect("create");
    let (limited, _) = keys
        .create(new_key(&[Scope::Read], &["orders", "eu.*"]), now())
        .expect("create");

    assert!(open.allows("anything"));
    assert!(limited.allows("orders"));
    assert!(limited.allows("eu.invoices"));
    assert!(!limited.allows("orders2"));
    assert!(!limited.allows("us.orders"));
}

#[test]
fn invalid_requests_report_every_problem() {
    let (keys, _db, _dir) = store();
    let bad = NewKey {
        name: String::new(),
        scopes: vec![],
        collections: vec!["Bad Name".into()],
    };

    let Err(KeyError::Invalid(errors)) = keys.create(bad, now()) else {
        panic!("expected validation errors");
    };

    let fields: Vec<&str> = errors.fields().keys().map(String::as_str).collect();
    assert_eq!(fields, ["collections", "name", "scopes"]);
}

#[test]
fn duplicate_scopes_are_collapsed() {
    let (keys, _db, _dir) = store();

    let (record, _) = keys
        .create(
            new_key(&[Scope::Manage, Scope::Read, Scope::Read], &[]),
            now(),
        )
        .expect("create");

    assert_eq!(record.scopes, [Scope::Read, Scope::Manage]);
    assert!(record.has_scope(Scope::Manage));
    assert!(!record.has_scope(Scope::Generate));
}
