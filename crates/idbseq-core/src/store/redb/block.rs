//! Block reservation for collections with `block` durability (US-0035).
//!
//! The disk records a high-water mark (`reserved_until`, stored as the
//! collection's `last_id`). IDs up to that mark are served from memory. When a
//! request needs more, a new block is reserved and committed before any ID in it
//! is returned. After a crash the store restarts from the mark, so the unused rest
//! of a block is skipped (a gap) and never reissued.
//!
//! One mutex guards the reservations. `update` and `delete` take it too, so a
//! change to a block collection can never race with serving from its block.
//! Strict collections belong to the committer: this path never writes one.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ::redb::{Database, ReadableDatabase, ReadableTable};

use super::codec::{backend, decode, encode};
use super::{COLLECTIONS, IDEMPOTENCY};
use crate::collection::{Collection, Durability, Id};
use crate::store::idempotency::{self, Record};
use crate::store::{AdvanceRequest, Issued, StoreError};

/// What has been handed out from a block collection's current reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Reservation {
    last_issued: Option<Id>,
    reserved_until: Option<Id>,
}

pub(super) type Cache = HashMap<String, Reservation>;

#[derive(Debug, Default)]
pub(super) struct Blocks {
    cache: Mutex<Cache>,
}

impl Blocks {
    fn lock(&self) -> MutexGuard<'_, Cache> {
        // Entries are replaced whole after the disk commit, so a poisoned lock
        // still holds consistent data.
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Run `f` while holding the reservation lock.
    pub fn with_lock<T>(&self, f: impl FnOnce(&mut Cache) -> T) -> T {
        f(&mut self.lock())
    }

    /// The collection as callers see it: `last_id` is the last ID actually issued.
    pub fn overlay(&self, persisted: Collection) -> Collection {
        Self::overlay_with(&self.lock(), persisted)
    }

    pub fn overlay_with(cache: &Cache, persisted: Collection) -> Collection {
        match cache.get(persisted.name()) {
            Some(reservation) => persisted.with_last_id(reservation.last_issued),
            None => persisted,
        }
    }

    /// Issue from a block collection's reservation, with the reservation lock held
    /// (see [`Blocks::with_lock`]). `None` when the collection is strict now: the
    /// caller hands it back to the committer while still holding the lock.
    pub fn advance(
        cache: &mut Cache,
        db: &Database,
        request: &AdvanceRequest,
    ) -> Result<Option<Issued>, StoreError> {
        let name = request.name.as_str();
        let persisted = read_collection(db, name)?;
        // Block collections are only written under this lock, so this read stays
        // current until the commit below.
        let Durability::Block { size: block_size } = persisted.spec().durability else {
            return Ok(None);
        };
        if let Some(range) = replayed(db, request)? {
            return Ok(Some(Issued {
                range,
                replayed: true,
            }));
        }
        let current = cache.get(name).copied().unwrap_or(Reservation {
            last_issued: persisted.last_id(),
            reserved_until: persisted.last_id(),
        });
        let view = persisted.with_last_id(current.last_issued);
        let (_, range) = view
            .advance(request.count)
            .map_err(|_| StoreError::Exhausted(name.to_owned()))?;

        let covered = current
            .reserved_until
            .is_some_and(|mark| range.last <= mark);
        let reserved_until = if covered {
            current.reserved_until
        } else {
            let size = block_size.max(request.count);
            let block_end = view
                .advance(size)
                // A full block does not fit: reserve up to the last ID on the grid.
                .map_or_else(|_| view.highest_id(), |(_, block)| block.last);
            Some(block_end)
        };
        if !covered || request.idempotency.is_some() {
            let mark = (!covered).then(|| persisted.with_last_id(reserved_until));
            if let Some(range) = persist(db, mark.as_ref(), request, range)? {
                // The same key was just used on a strict collection.
                return Ok(Some(Issued {
                    range,
                    replayed: true,
                }));
            }
        }
        let reservation = Reservation {
            last_issued: Some(range.last),
            reserved_until,
        };
        cache.insert(name.to_owned(), reservation);
        Ok(Some(Issued {
            range,
            replayed: false,
        }))
    }
}

fn read_collection(db: &Database, name: &str) -> Result<Collection, StoreError> {
    let txn = db.begin_read().map_err(backend)?;
    let table = txn.open_table(COLLECTIONS).map_err(backend)?;
    let bytes = table.get(name).map_err(backend)?;
    bytes
        .map(|guard| decode(guard.value()))
        .transpose()?
        .ok_or_else(|| StoreError::NotFound(name.to_owned()))
}

fn replayed(
    db: &Database,
    request: &AdvanceRequest,
) -> Result<Option<crate::collection::IssuedRange>, StoreError> {
    let Some(key) = &request.idempotency else {
        return Ok(None);
    };
    let txn = db.begin_read().map_err(backend)?;
    let table = txn.open_table(IDEMPOTENCY).map_err(backend)?;
    let existing: Option<Record> = table
        .get(key.key.as_str())
        .map_err(backend)?
        .map(|guard| decode(guard.value()))
        .transpose()?;
    idempotency::replay(existing.as_ref(), key)
}

/// Commit a new reservation mark and/or the idempotency record in one transaction.
/// The key is checked again inside it, because the committer may have recorded it
/// for a strict collection since [`replayed`] looked: then nothing is written and
/// that earlier range is the answer (or a mismatch error).
fn persist(
    db: &Database,
    mark: Option<&Collection>,
    request: &AdvanceRequest,
    range: crate::collection::IssuedRange,
) -> Result<Option<crate::collection::IssuedRange>, StoreError> {
    let txn = db.begin_write().map_err(backend)?;
    if let Some(key) = &request.idempotency {
        let existing: Option<Record> = txn
            .open_table(IDEMPOTENCY)
            .map_err(backend)?
            .get(key.key.as_str())
            .map_err(backend)?
            .map(|guard| decode(guard.value()))
            .transpose()?;
        if let Some(range) = idempotency::replay(existing.as_ref(), key)? {
            return Ok(Some(range));
        }
    }
    {
        if let Some(collection) = mark {
            txn.open_table(COLLECTIONS)
                .map_err(backend)?
                .insert(collection.name(), encode(collection)?.as_slice())
                .map_err(backend)?;
        }
        if let Some(key) = &request.idempotency {
            txn.open_table(IDEMPOTENCY)
                .map_err(backend)?
                .insert(
                    key.key.as_str(),
                    encode(&Record::new(key, range))?.as_slice(),
                )
                .map_err(backend)?;
        }
    }
    txn.commit().map_err(backend)?;
    Ok(None)
}
