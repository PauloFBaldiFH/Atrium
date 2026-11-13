// SPDX-License-Identifier: GPL-3.0-or-later
//! Content digest of a database, to check that a parallel run equals a serial one.
//!
//! Every table is read in a fixed order (by name, then by all columns) and fed to a
//! 64-bit FNV-1a hash with each value's type and length, so `('ab', 'c')` and `('a', 'bc')`
//! differ. FNV is enough here: the digest detects accidental differences, not attacks,
//! and a stable, dependency-free function keeps digests comparable across builds.

use rusqlite::Connection;
use rusqlite::types::ValueRef;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    fn sized(&mut self, tag: u8, bytes: &[u8]) {
        self.bytes(&[tag]);
        self.bytes(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        self.bytes(bytes);
    }
}

/// The digest of every table, as 16 hex digits.
pub(crate) fn database(connection: &Connection) -> Result<String, crate::StoreError> {
    let mut hash = Fnv(FNV_OFFSET);
    for table in tables(connection)? {
        hash.sized(b'T', table.as_bytes());
        let columns = column_count(connection, &table)?;
        let order = (1..=columns)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        // Table names come from sqlite_master of our own schema, never from input; quoting
        // them is all an identifier needs (parameters cannot name tables).
        let sql = format!("SELECT * FROM \"{table}\" ORDER BY {order}");
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            for column in 0..columns {
                value(&mut hash, row.get_ref(column)?);
            }
        }
    }
    Ok(format!("{:016x}", hash.0))
}

fn tables(connection: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()
}

fn column_count(connection: &Connection, table: &str) -> Result<usize, rusqlite::Error> {
    // SQLite allows at most 32767 columns, so a `u16` always fits.
    connection
        .query_row(
            "SELECT count(*) FROM pragma_table_info(?1)",
            [table],
            |row| row.get::<_, u16>(0),
        )
        .map(usize::from)
}

fn value(hash: &mut Fnv, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => hash.bytes(b"N"),
        ValueRef::Integer(number) => {
            hash.bytes(b"I");
            hash.bytes(&number.to_le_bytes());
        }
        ValueRef::Real(number) => {
            hash.bytes(b"R");
            hash.bytes(&number.to_bits().to_le_bytes());
        }
        ValueRef::Text(text) => hash.sized(b'S', text),
        ValueRef::Blob(blob) => hash.sized(b'B', blob),
    }
}
