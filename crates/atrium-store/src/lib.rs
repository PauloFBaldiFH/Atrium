// SPDX-License-Identifier: GPL-3.0-or-later
//! SQLite storage: the single writer thread, the read pool, audit log and effect outbox.
//!
//! One [`Store`] serves one institution's database (docs/arquitetura/04-decisoes.md, §4).
//!
//! - **Writes** go through one dedicated thread that owns the only writable connection.
//!   Callers submit typed commands ([`CommandHandler::Command`]) to a bounded queue and get
//!   the outcome back on a reply channel (docs/arquitetura/05-definicoes-de-implementacao.md,
//!   §1). The writer groups queued commands into one transaction, gives each command its own
//!   savepoint, records it in `audit_log`, and replies only after the commit.
//! - **Reads** use a small pool of read-only connections; WAL lets them run while the writer
//!   commits.
//! - **Effects** that must happen after a commit (reindexing, notices) are written to
//!   `effect_outbox` inside the same transaction as the change that causes them.

mod atrium;
mod config;
mod error;
mod readers;
mod schema;
mod writer;

pub use atrium::{AtriumHandler, AtriumStore};
pub use config::StoreConfig;
pub use error::{CommandFailed, StoreError, SubmitError};
pub use readers::ReadPool;
pub use writer::{CommandHandler, Pending, WriteTx};

use std::path::Path;

/// An open institution database: the writer thread plus the read pool.
///
/// Dropping the store closes the queue, lets the writer finish what is already queued,
/// and waits for it to optimize and checkpoint the database.
pub struct Store<H: CommandHandler> {
    writer: writer::WriterHandle<H>,
    readers: ReadPool,
}

impl<H: CommandHandler> Store<H> {
    /// Opens (creating if needed) the database at `path`, applies pending migrations and
    /// starts the writer thread with `handler`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be opened, a migration fails, or the thread cannot start.
    pub fn open(path: &Path, config: &StoreConfig, handler: H) -> Result<Self, StoreError> {
        let connection = schema::open_writer(path)?;
        let writer = writer::WriterHandle::start(connection, config, handler)?;
        let readers = ReadPool::open(path, config.readers)?;
        Ok(Self { writer, readers })
    }

    /// Queues `command` for the writer.
    ///
    /// # Errors
    ///
    /// [`SubmitError::Busy`] when the queue is full (answer 503 with `Retry-After`), and
    /// [`SubmitError::Closed`] when the writer has stopped. Never blocks.
    pub fn submit(&self, command: H::Command) -> Result<Pending<H>, SubmitError> {
        self.writer.submit(command)
    }

    /// The read-only connection pool.
    #[must_use]
    pub fn readers(&self) -> &ReadPool {
        &self.readers
    }
}
