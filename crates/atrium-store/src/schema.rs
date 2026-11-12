// SPDX-License-Identifier: GPL-3.0-or-later
//! Connection setup and migrations.
//!
//! Migrations are plain SQL, applied in order by the writer connection at open time and
//! tracked in `PRAGMA user_version`. A migration is never edited after it ships; changes
//! are new migrations. Every table with rows that users edit carries a `row_version`
//! column (docs/arquitetura/05-definicoes-de-implementacao.md, §3).

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::StoreError;

/// Ordered migrations; migration `n` (1-based) moves `user_version` from `n - 1` to `n`.
const MIGRATIONS: &[&str] = &[
    // 1: infrastructure tables.
    "CREATE TABLE audit_log (
         seq        INTEGER PRIMARY KEY,
         kind       TEXT NOT NULL,
         command    TEXT NOT NULL CHECK (json_valid(command))
     ) STRICT;
     CREATE TABLE effect_outbox (
         id         INTEGER PRIMARY KEY,
         audit_seq  INTEGER NOT NULL REFERENCES audit_log (seq),
         kind       TEXT NOT NULL,
         payload    TEXT NOT NULL CHECK (json_valid(payload)),
         done       INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1))
     ) STRICT;
     CREATE INDEX effect_outbox_pending ON effect_outbox (id) WHERE done = 0;",
    // 2: staff accounts and their permissions.
    "CREATE TABLE staff_accounts (
         id            INTEGER PRIMARY KEY,
         login         TEXT NOT NULL UNIQUE,
         password_hash TEXT NOT NULL,
         row_version   INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE TABLE staff_permissions (
         staff_id   INTEGER NOT NULL REFERENCES staff_accounts (id) ON DELETE CASCADE,
         permission TEXT NOT NULL,
         PRIMARY KEY (staff_id, permission)
     ) STRICT, WITHOUT ROWID;",
];

/// How long a connection waits on a lock before failing. With a single writer this only
/// covers checkpoints and external tools; it is not part of normal operation.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens the writable connection, sets the PRAGMAs and applies pending migrations.
pub(crate) fn open_writer(path: &Path) -> Result<Connection, StoreError> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
    )?;
    migrate(&mut connection)?;
    Ok(connection)
}

/// Opens a read-only connection for the pool.
pub(crate) fn open_reader(path: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA temp_store = MEMORY;")?;
    Ok(connection)
}

/// Newest schema version this build knows.
fn latest_version() -> i64 {
    i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX)
}

fn migrate(connection: &mut Connection) -> Result<(), StoreError> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let supported = latest_version();
    if current > supported {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported,
        });
    }
    let tx = connection.transaction()?;
    for (version, sql) in (1_i64..).zip(MIGRATIONS) {
        if version > current {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atrium.db");
        drop(open_writer(&path).unwrap());
        let connection = open_writer(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, latest_version());
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atrium.db");
        let connection = open_writer(&path).unwrap();
        connection
            .pragma_update(None, "user_version", latest_version() + 1)
            .unwrap();
        drop(connection);
        assert!(matches!(
            open_writer(&path),
            Err(StoreError::SchemaTooNew { .. })
        ));
    }
}
