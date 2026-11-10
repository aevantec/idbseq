//! Append-only audit log of destructive and admin operations (US-0025).

use std::time::{Duration, SystemTime};

use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::{AUDIT, Db, DbError, db_err};

/// Audited actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum Action {
    #[serde(rename = "collection.create")]
    CollectionCreate,
    #[serde(rename = "collection.update")]
    CollectionUpdate,
    #[serde(rename = "collection.reset")]
    CollectionReset,
    #[serde(rename = "collection.delete")]
    CollectionDelete,
    #[serde(rename = "key.create")]
    KeyCreate,
    #[serde(rename = "key.revoke")]
    KeyRevoke,
    #[serde(rename = "auth.login")]
    Login,
    #[serde(rename = "auth.login_failed")]
    LoginFailed,
    #[serde(rename = "admin.password_change")]
    PasswordChange,
    #[serde(rename = "admin.init")]
    AdminInit,
}

impl Action {
    /// Parse the wire name (`collection.create`, …).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        serde_json::from_value(Value::String(name.to_owned())).ok()
    }
}

/// What happened, who did it, and where from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub actor: String,
    pub action: Action,
    pub collection: Option<String>,
    pub previous: Option<Value>,
    pub new: Option<Value>,
    pub request_id: Option<String>,
    pub client_ip: Option<String>,
}

/// A stored event with its sequence number and time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub at: SystemTime,
    #[serde(flatten)]
    pub event: Event,
}

/// A page of entries, newest first. `next_cursor` is the id to pass as `cursor`.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditPage {
    pub items: Vec<Entry>,
    pub next_cursor: Option<u64>,
    pub total: usize,
}

/// Filters for [`AuditLog::list`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub collection: Option<String>,
    pub action: Option<Action>,
}

impl Filter {
    fn matches(&self, entry: &Entry) -> bool {
        self.collection
            .as_ref()
            .is_none_or(|c| entry.event.collection.as_ref() == Some(c))
            && self.action.is_none_or(|a| entry.event.action == a)
    }
}

#[derive(Debug, Clone)]
pub struct AuditLog {
    db: Db,
    retention: Duration,
}

impl AuditLog {
    #[must_use]
    pub fn new(db: Db, retention: Duration) -> Self {
        Self { db, retention }
    }

    /// Append an event. Returns its id.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn append(&self, event: Event, at: SystemTime) -> Result<u64, DbError> {
        let txn = self.db.begin_write().map_err(db_err)?;
        let id = {
            let mut table = txn.open_table(AUDIT).map_err(db_err)?;
            let last = table.last().map_err(db_err)?.map(|(k, _)| k.value());
            let id = last.map_or(1, |l| l + 1);
            let entry = Entry { id, at, event };
            let bytes = serde_json::to_vec(&entry).map_err(db_err)?;
            table.insert(id, bytes.as_slice()).map_err(db_err)?;
            id
        };
        txn.commit().map_err(db_err)?;
        Ok(id)
    }

    /// Entries newest first, filtered, `limit` per page, before `cursor` (exclusive).
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn list(
        &self,
        filter: &Filter,
        limit: usize,
        cursor: Option<u64>,
    ) -> Result<AuditPage, DbError> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let table = txn.open_table(AUDIT).map_err(db_err)?;
        let entries: Vec<Entry> = table
            .iter()
            .map_err(db_err)?
            .rev()
            .map(|row| {
                let (_, value) = row.map_err(db_err)?;
                serde_json::from_slice::<Entry>(value.value()).map_err(db_err)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|e| filter.matches(e))
            .collect();
        let total = entries.len();
        let mut after = entries
            .into_iter()
            .filter(|e| cursor.is_none_or(|c| e.id < c));
        let items: Vec<Entry> = after.by_ref().take(limit).collect();
        let next_cursor = after.next().and(items.last()).map(|e| e.id);
        Ok(AuditPage {
            items,
            next_cursor,
            total,
        })
    }

    /// Delete entries older than the retention period. Returns how many.
    ///
    /// # Errors
    ///
    /// Storage failure.
    pub fn purge(&self, now: SystemTime) -> Result<usize, DbError> {
        let cutoff = now
            .checked_sub(self.retention)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let txn = self.db.begin_write().map_err(db_err)?;
        let removed = {
            let mut table = txn.open_table(AUDIT).map_err(db_err)?;
            let old: Vec<u64> = table
                .iter()
                .map_err(db_err)?
                .filter_map(|row| {
                    let (key, value) = row.ok()?;
                    let entry: Entry = serde_json::from_slice(value.value()).ok()?;
                    (entry.at < cutoff).then(|| key.value())
                })
                .collect();
            for id in &old {
                table.remove(*id).map_err(db_err)?;
            }
            old.len()
        };
        txn.commit().map_err(db_err)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn log() -> (AuditLog, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = redb::Database::create(dir.path().join("t.redb")).expect("db");
        crate::db::init(&db).expect("init");
        (AuditLog::new(Arc::new(db), Duration::from_hours(24)), dir)
    }

    fn event(action: Action, collection: Option<&str>) -> Event {
        Event {
            actor: "admin".into(),
            action,
            collection: collection.map(str::to_owned),
            previous: None,
            new: None,
            request_id: Some("req-1".into()),
            client_ip: Some("127.0.0.1".into()),
        }
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn entries_are_listed_newest_first_with_filters_and_pages() {
        let (log, _dir) = log();
        log.append(event(Action::CollectionCreate, Some("a")), at(1))
            .expect("1");
        log.append(event(Action::CollectionReset, Some("a")), at(2))
            .expect("2");
        log.append(event(Action::KeyCreate, None), at(3))
            .expect("3");
        log.append(event(Action::CollectionDelete, Some("a")), at(4))
            .expect("4");

        let first = log
            .list(
                &Filter {
                    collection: Some("a".into()),
                    action: None,
                },
                2,
                None,
            )
            .expect("list");
        let second = log
            .list(
                &Filter {
                    collection: Some("a".into()),
                    action: None,
                },
                2,
                first.next_cursor,
            )
            .expect("list");
        let resets = log
            .list(
                &Filter {
                    action: Action::parse("collection.reset"),
                    collection: None,
                },
                10,
                None,
            )
            .expect("list");

        assert_eq!(first.items.iter().map(|e| e.id).collect::<Vec<_>>(), [4, 2]);
        assert_eq!((first.total, first.next_cursor), (3, Some(2)));
        assert_eq!(second.items.iter().map(|e| e.id).collect::<Vec<_>>(), [1]);
        assert_eq!(second.next_cursor, None);
        assert_eq!(resets.items.len(), 1);
        assert_eq!(Action::parse("nope"), None);
    }

    #[test]
    fn purge_drops_entries_older_than_the_retention() {
        let (log, _dir) = log();
        log.append(event(Action::Login, None), at(1_000))
            .expect("old");
        log.append(event(Action::Login, None), at(100_000))
            .expect("new");

        let removed = log
            .purge(at(100_000) + Duration::from_secs(60))
            .expect("purge");

        assert_eq!(removed, 1);
        assert_eq!(
            log.list(&Filter::default(), 10, None).expect("list").total,
            1
        );
    }

    #[test]
    fn ids_keep_increasing_after_a_purge() {
        let (log, _dir) = log();
        log.append(event(Action::Login, None), at(1)).expect("1");
        let second = log
            .append(event(Action::Login, None), at(100_000))
            .expect("2");
        log.purge(at(100_000)).expect("purge");

        let third = log
            .append(event(Action::Login, None), at(100_001))
            .expect("3");

        assert_eq!((second, third), (2, 3));
    }
}
