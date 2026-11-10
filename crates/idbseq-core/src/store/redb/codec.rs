//! Value encoding for redb tables (JSON: readable, and tolerant of added fields).

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::store::StoreError;

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(value).map_err(backend)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, StoreError> {
    serde_json::from_slice(bytes).map_err(backend)
}

/// Any backend failure, with its message.
#[allow(clippy::needless_pass_by_value)] // used as `map_err(backend)`
pub(crate) fn backend(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}
