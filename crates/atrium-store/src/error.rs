// SPDX-License-Identifier: GPL-3.0-or-later
//! Store errors.

/// A storage failure. Cloneable so one failed commit can be reported to every command in
/// the batch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// SQLite returned an error.
    #[error("sqlite: {0}")]
    Sqlite(String),
    /// A command or effect payload could not be serialized.
    #[error("serialization: {0}")]
    Serialize(String),
    /// The database was written by a newer Atrium than this one.
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew {
        /// Version found in `PRAGMA user_version`.
        found: i64,
        /// Newest version this build knows.
        supported: i64,
    },
    /// The writer thread could not be started.
    #[error("cannot start the writer thread: {0}")]
    Thread(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error.to_string())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialize(error.to_string())
    }
}

/// Why a command could not be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    /// The queue is full. Callers answer 503 with `Retry-After`; nothing was queued.
    #[error("the writer queue is full")]
    Busy,
    /// The writer has stopped (the store is closing).
    #[error("the writer has stopped")]
    Closed,
}

/// Why a queued command did not take effect. In every case nothing it wrote was kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandFailed<E> {
    /// The handler refused the command (a business rule said no).
    #[error("command rejected: {0}")]
    Rejected(E),
    /// Storage failed while applying or committing the command.
    #[error(transparent)]
    Storage(#[from] StoreError),
    /// The writer stopped before answering.
    #[error("the writer stopped before answering")]
    WriterGone,
}
