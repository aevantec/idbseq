use std::time::Duration;

pub use idbseq_core::store::DEFAULT_BUSY_TIMEOUT;

/// Who may use the file while a [`crate::Database`] is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The handle holds the file until it is dropped. Fastest: strict
    /// collections share disk writes between threads, and block collections
    /// issue IDs from memory. Another process waits, then gets [`crate::Error::Busy`].
    #[default]
    Exclusive,
    /// Every call opens the file, writes and closes it, so several processes
    /// (or handles) can take turns on one file. Each ID costs a file open and a
    /// disk write; block collections are written like strict ones.
    Shared,
}

/// Options for [`crate::Database::open_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    /// How long to wait while another process holds the file.
    pub busy_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::Exclusive,
            busy_timeout: DEFAULT_BUSY_TIMEOUT,
        }
    }
}

impl Options {
    /// [`Mode::Shared`] with the default busy timeout.
    #[must_use]
    pub fn shared() -> Self {
        Self {
            mode: Mode::Shared,
            ..Self::default()
        }
    }

    /// The same options with another busy timeout.
    #[must_use]
    pub fn busy_timeout(self, busy_timeout: Duration) -> Self {
        Self {
            busy_timeout,
            ..self
        }
    }
}
