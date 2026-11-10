//! Idempotency records shared by every backend.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::{IdempotencyKey, StoreError};
use crate::collection::IssuedRange;

/// What a backend stores per idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Record {
    pub fingerprint: String,
    pub range: IssuedRange,
    pub expires_at: SystemTime,
}

impl Record {
    pub fn new(key: &IdempotencyKey, range: IssuedRange) -> Self {
        Self {
            fingerprint: key.fingerprint.clone(),
            range,
            expires_at: key.now + key.ttl,
        }
    }

    pub fn is_expired(&self, now: SystemTime) -> bool {
        self.expires_at <= now
    }
}

/// The range to replay for `key`, `None` to issue fresh IDs, or a mismatch error.
pub(crate) fn replay(
    existing: Option<&Record>,
    key: &IdempotencyKey,
) -> Result<Option<IssuedRange>, StoreError> {
    match existing {
        Some(record) if !record.is_expired(key.now) => {
            if record.fingerprint == key.fingerprint {
                Ok(Some(record.range))
            } else {
                Err(StoreError::IdempotencyMismatch)
            }
        }
        _ => Ok(None),
    }
}
