// SPDX-License-Identifier: GPL-3.0-or-later
//! Pool of read-only connections.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Condvar, Mutex, PoisonError};

use rusqlite::Connection;

use crate::{StoreError, schema};

/// A fixed set of read-only connections. A caller borrows one for the duration of a
/// closure; when all are busy, the caller waits for the next one to come back.
///
/// Blocking: call it from a blocking context (`spawn_blocking` in async code).
pub struct ReadPool {
    idle: Mutex<Vec<Connection>>,
    returned: Condvar,
}

impl ReadPool {
    pub(crate) fn open(path: &Path, size: NonZeroUsize) -> Result<Self, StoreError> {
        let connections = (0..size.get())
            .map(|_| schema::open_reader(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            idle: Mutex::new(connections),
            returned: Condvar::new(),
        })
    }

    /// Runs `read` with a pooled read-only connection.
    ///
    /// # Errors
    ///
    /// Returns whatever `read` returns; the pool itself does not fail.
    pub fn read<T, E>(&self, read: impl FnOnce(&Connection) -> Result<T, E>) -> Result<T, E> {
        let lease = Lease {
            pool: self,
            connection: Some(self.take()),
        };
        read(lease.connection())
    }

    fn take(&self) -> Connection {
        // A poisoned lock only means another reader panicked while holding it; the Vec of
        // connections is still consistent, so keep using it.
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(connection) = idle.pop() {
                return connection;
            }
            idle = self
                .returned
                .wait(idle)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Returns a connection to the pool and wakes one waiting reader.
    fn give_back(&self, connection: Connection) {
        self.idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(connection);
        self.returned.notify_one();
    }
}

/// Hands the connection back even if the reader closure panics, so the pool never shrinks.
struct Lease<'a> {
    pool: &'a ReadPool,
    connection: Option<Connection>,
}

impl Lease<'_> {
    #[expect(
        clippy::expect_used,
        reason = "`connection` is Some from construction until `drop`, the only place that takes it"
    )]
    fn connection(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("a lease holds its connection until dropped")
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.pool.give_back(connection);
        }
    }
}
