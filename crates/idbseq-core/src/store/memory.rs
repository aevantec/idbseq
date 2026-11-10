//! In-memory [`SequenceStore`]: fast and non-durable, for tests.
//!
//! `block` durability behaves like `strict` here: there is no disk to save.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::SystemTime;

use super::idempotency::{self, Record};
use super::{
    AdvanceRequest, Change, Issued, Page, PageRequest, SequenceStore, StoreError, Updated, page_of,
};
use crate::collection::Collection;

#[derive(Debug, Default)]
struct State {
    collections: BTreeMap<String, Collection>,
    idempotency: HashMap<String, Record>,
}

/// A [`SequenceStore`] that keeps everything in memory. Data is lost on drop.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    state: RwLock<State>,
}

impl InMemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // A panic while the lock is held cannot leave a half-applied change: each
    // write computes the new value first and then stores it in one step. So a
    // poisoned lock still guards consistent data and is safe to keep using.
    fn read(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }
}

fn not_found(name: &str) -> StoreError {
    StoreError::NotFound(name.to_owned())
}

impl SequenceStore for InMemoryStore {
    fn create(&self, collection: Collection) -> Result<Collection, StoreError> {
        match self.write().collections.entry(collection.name().to_owned()) {
            Entry::Occupied(taken) => Err(StoreError::AlreadyExists(taken.key().clone())),
            Entry::Vacant(slot) => Ok(slot.insert(collection).clone()),
        }
    }

    fn get(&self, name: &str) -> Result<Collection, StoreError> {
        self.read()
            .collections
            .get(name)
            .cloned()
            .ok_or_else(|| not_found(name))
    }

    fn list(&self, request: &PageRequest) -> Result<Page, StoreError> {
        let state = self.read();
        let (names, next_cursor, total) =
            page_of(state.collections.keys().map(String::as_str), request);
        let items = names
            .into_iter()
            .filter_map(|name| state.collections.get(name).cloned())
            .collect();
        Ok(Page {
            items,
            next_cursor,
            total,
        })
    }

    fn delete(&self, name: &str) -> Result<Collection, StoreError> {
        self.write()
            .collections
            .remove(name)
            .ok_or_else(|| not_found(name))
    }

    fn update(&self, name: &str, change: Change<'_>) -> Result<Updated, StoreError> {
        let mut state = self.write();
        let before = state
            .collections
            .get(name)
            .cloned()
            .ok_or_else(|| not_found(name))?;
        let after = change(&before)?;
        state.collections.insert(name.to_owned(), after.clone());
        Ok(Updated { before, after })
    }

    fn advance(&self, request: &AdvanceRequest) -> Result<Issued, StoreError> {
        let mut state = self.write();
        if let Some(key) = &request.idempotency
            && let Some(range) = idempotency::replay(state.idempotency.get(&key.key), key)?
        {
            return Ok(Issued {
                range,
                replayed: true,
            });
        }
        let name = &request.name;
        let current = state.collections.get(name).ok_or_else(|| not_found(name))?;
        let (updated, range) = current
            .advance(request.count)
            .map_err(|_| StoreError::Exhausted(name.clone()))?;
        state.collections.insert(name.clone(), updated);
        if let Some(key) = &request.idempotency {
            state
                .idempotency
                .insert(key.key.clone(), Record::new(key, range));
        }
        Ok(Issued {
            range,
            replayed: false,
        })
    }

    fn purge_idempotency(&self, now: SystemTime) -> Result<usize, StoreError> {
        let mut state = self.write();
        let before = state.idempotency.len();
        state
            .idempotency
            .retain(|_, record| !record.is_expired(now));
        Ok(before - state.idempotency.len())
    }
}

#[cfg(test)]
mod tests {
    use super::InMemoryStore;

    sequence_store_conformance!(|| Box::new(InMemoryStore::new()));
}
