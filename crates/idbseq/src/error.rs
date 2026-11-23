use std::path::PathBuf;
use std::time::Duration;

use idbseq_core::store::{OpenError, StoreError};

/// Why a [`crate::Database`] call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Another process held the file for the whole busy timeout.
    #[error("{} stayed in use by another process for {waited:?}", path.display())]
    Busy { path: PathBuf, waited: Duration },
    /// The file could not be opened or created: a missing folder, no
    /// permission, or a file that is not an idbseq file.
    #[error(transparent)]
    Open(OpenError),
    /// The call itself failed: unknown or existing collection, exhausted
    /// sequence, invalid input, or a storage failure.
    #[error(transparent)]
    Store(StoreError),
}

impl From<StoreError> for Error {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Busy { path, waited } => Self::Busy { path, waited },
            StoreError::Unavailable(open) => Self::Open(open),
            other => Self::Store(other),
        }
    }
}
