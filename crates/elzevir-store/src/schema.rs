// SPDX-License-Identifier: GPL-3.0-or-later
//! Connection setup and migrations.
//!
//! Migrations are plain SQL, applied in order by the writer connection at open time and
//! tracked in `PRAGMA user_version`. A migration is never edited after it ships; changes
//! are new migrations. Every table with rows that users edit carries a `row_version`
//! column (docs/arquitetura/05-definicoes-de-implementacao.md, §3).

use std::path::Path;
use std::time::Duration;

use elzevir_app::marc_mapping::{source_audience, source_notes};
use elzevir_app::matching::{identifier_key, koha_biblionumber_key};
use elzevir_domain::catalog::IdentifierKind;
use rusqlite::{Connection, OpenFlags, params};

use crate::StoreError;
use crate::catalog::insert_notes;

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
    // 3: the bibliographic model (docs/specs/catalogo/modelo-bibliografico.md). Small
    // integer columns named `kind`, `vocabulary` or `format` hold the stable numbers of
    // the enums in `elzevir_domain::catalog`.
    "CREATE TABLE branches (
         id          INTEGER PRIMARY KEY,
         code        TEXT NOT NULL UNIQUE,
         name        TEXT NOT NULL,
         row_version INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE TABLE item_types (
         id          INTEGER PRIMARY KEY,
         code        TEXT NOT NULL UNIQUE,
         description TEXT NOT NULL,
         row_version INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE TABLE terms (
         id          INTEGER PRIMARY KEY,
         vocabulary  INTEGER NOT NULL CHECK (vocabulary BETWEEN 1 AND 255),
         code        TEXT NOT NULL,
         label       TEXT NOT NULL,
         UNIQUE (vocabulary, code)
     ) STRICT;
     CREATE TABLE agents (
         id          INTEGER PRIMARY KEY,
         kind        INTEGER NOT NULL CHECK (kind BETWEEN 1 AND 255),
         heading     TEXT NOT NULL,
         UNIQUE (kind, heading)
     ) STRICT;
     CREATE TABLE subjects (
         id          INTEGER PRIMARY KEY,
         kind        INTEGER NOT NULL CHECK (kind BETWEEN 1 AND 255),
         heading     TEXT NOT NULL,
         thesaurus   INTEGER REFERENCES terms (id)
     ) STRICT;
     -- Two headings without a thesaurus are the same subject; a plain UNIQUE would let
     -- them both in, because NULLs never collide.
     CREATE UNIQUE INDEX subjects_identity ON subjects (kind, heading, coalesce(thesaurus, 0));
     CREATE TABLE works (
         id             INTEGER PRIMARY KEY,
         title          TEXT NOT NULL,
         classification TEXT,
         classification_scheme INTEGER REFERENCES terms (id),
         summary        TEXT,
         extra          TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(extra)),
         row_version    INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE TABLE work_languages (
         work_id   INTEGER NOT NULL REFERENCES works (id) ON DELETE CASCADE,
         position  INTEGER NOT NULL,
         language  INTEGER NOT NULL REFERENCES terms (id),
         PRIMARY KEY (work_id, position)
     ) STRICT, WITHOUT ROWID;
     CREATE TABLE contributions (
         work_id   INTEGER NOT NULL REFERENCES works (id) ON DELETE CASCADE,
         position  INTEGER NOT NULL,
         agent_id  INTEGER NOT NULL REFERENCES agents (id),
         role      INTEGER REFERENCES terms (id),
         is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
         PRIMARY KEY (work_id, position)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX contributions_by_agent ON contributions (agent_id);
     CREATE TABLE work_subjects (
         work_id    INTEGER NOT NULL REFERENCES works (id) ON DELETE CASCADE,
         position   INTEGER NOT NULL,
         subject_id INTEGER NOT NULL REFERENCES subjects (id),
         PRIMARY KEY (work_id, position)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX work_subjects_by_subject ON work_subjects (subject_id);
     CREATE TABLE instances (
         id                 INTEGER PRIMARY KEY,
         work_id            INTEGER NOT NULL REFERENCES works (id),
         title              TEXT NOT NULL,
         edition            TEXT,
         place              TEXT,
         publisher          TEXT,
         date               TEXT,
         extent             TEXT,
         series             TEXT,
         extra              TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(extra)),
         row_version        INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE INDEX instances_by_work ON instances (work_id);
     CREATE TABLE instance_identifiers (
         instance_id INTEGER NOT NULL REFERENCES instances (id) ON DELETE CASCADE,
         position    INTEGER NOT NULL,
         kind        INTEGER NOT NULL CHECK (kind BETWEEN 1 AND 255),
         value       TEXT NOT NULL,
         qualifier   TEXT,
         PRIMARY KEY (instance_id, position)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX instance_identifiers_by_value ON instance_identifiers (kind, value);
     CREATE TABLE source_records (
         instance_id INTEGER PRIMARY KEY REFERENCES instances (id) ON DELETE CASCADE,
         format      INTEGER NOT NULL CHECK (format BETWEEN 1 AND 255),
         record      BLOB NOT NULL
     ) STRICT;
     CREATE TABLE items (
         id                INTEGER PRIMARY KEY,
         instance_id       INTEGER NOT NULL REFERENCES instances (id),
         barcode           TEXT UNIQUE,
         home_branch       INTEGER NOT NULL REFERENCES branches (id),
         holding_branch    INTEGER NOT NULL REFERENCES branches (id),
         item_type         INTEGER NOT NULL REFERENCES item_types (id),
         call_number       TEXT,
         location          INTEGER REFERENCES terms (id),
         collection        INTEGER REFERENCES terms (id),
         copy_number       TEXT,
         not_for_loan      INTEGER NOT NULL DEFAULT 0,
         damaged           INTEGER NOT NULL DEFAULT 0,
         lost              INTEGER NOT NULL DEFAULT 0,
         withdrawn         INTEGER NOT NULL DEFAULT 0,
         restricted        INTEGER NOT NULL DEFAULT 0,
         price_cents       INTEGER,
         replacement_price_cents INTEGER,
         acquired_on       TEXT,
         extra             TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(extra)),
         row_version       INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     CREATE INDEX items_by_instance ON items (instance_id);",
    // 4: Hubs, relations between Works, material types and inventory numbers (ADR 0002).
    "CREATE TABLE hubs (
         id          INTEGER PRIMARY KEY,
         title       TEXT NOT NULL,
         extra       TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(extra)),
         row_version INTEGER NOT NULL DEFAULT 1
     ) STRICT;
     ALTER TABLE works ADD COLUMN hub_id INTEGER REFERENCES hubs (id);
     CREATE INDEX works_by_hub ON works (hub_id) WHERE hub_id IS NOT NULL;
     ALTER TABLE works ADD COLUMN material_type INTEGER;
     ALTER TABLE instances ADD COLUMN material_type INTEGER;
     CREATE TABLE work_relations (
         work_id         INTEGER NOT NULL REFERENCES works (id) ON DELETE CASCADE,
         position        INTEGER NOT NULL,
         kind            INTEGER NOT NULL,
         related_work_id INTEGER NOT NULL REFERENCES works (id),
         CHECK (work_id <> related_work_id),
         PRIMARY KEY (work_id, position)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX work_relations_by_related ON work_relations (related_work_id);
     ALTER TABLE items ADD COLUMN inventory_number TEXT;
     CREATE INDEX items_by_inventory_number ON items (inventory_number)
         WHERE inventory_number IS NOT NULL;",
    // 5: match keys for importing without duplicates (docs/specs/catalogo/casamento.md).
    // The keys of records stored before this migration are filled in by
    // `backfill_match_keys`, because computing them needs Rust (ISBN-13, MARC 999).
    "CREATE TABLE match_keys (
         rule        INTEGER NOT NULL CHECK (rule BETWEEN 1 AND 255),
         key         TEXT NOT NULL,
         instance_id INTEGER NOT NULL REFERENCES instances (id) ON DELETE CASCADE,
         PRIMARY KEY (rule, key, instance_id)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX match_keys_by_instance ON match_keys (instance_id);",
    // 6: notes of an Instance (MARC21 500, 505, 590), task 2.1.
    "CREATE TABLE instance_notes (
         instance_id INTEGER NOT NULL REFERENCES instances (id) ON DELETE CASCADE,
         position    INTEGER NOT NULL,
         kind        INTEGER NOT NULL CHECK (kind BETWEEN 1 AND 255),
         text        TEXT NOT NULL,
         PRIMARY KEY (instance_id, position)
     ) STRICT, WITHOUT ROWID;",
    // 7: target audience of a Work (MARC21 521), task 2.1.
    "ALTER TABLE works ADD COLUMN audience TEXT;",
];

/// The migration that creates `match_keys`.
const MATCH_KEYS_VERSION: i64 = 5;

/// The migration that creates `instance_notes`.
const NOTES_VERSION: i64 = 6;

/// The migration that adds `works.audience`.
const AUDIENCE_VERSION: i64 = 7;

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
            if version == MATCH_KEYS_VERSION {
                backfill_match_keys(&tx)?;
            }
            if version == NOTES_VERSION {
                backfill_notes(&tx)?;
            }
            if version == AUDIENCE_VERSION {
                backfill_audience(&tx)?;
            }
            tx.pragma_update(None, "user_version", version)?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Computes the match keys of every record already stored when `match_keys` is created.
fn backfill_match_keys(connection: &Connection) -> Result<(), StoreError> {
    let mut insert = connection
        .prepare("INSERT OR IGNORE INTO match_keys (rule, key, instance_id) VALUES (?1, ?2, ?3)")?;
    let mut identifiers =
        connection.prepare("SELECT instance_id, kind, value FROM instance_identifiers")?;
    let mut rows = identifiers.query([])?;
    while let Some(row) = rows.next()? {
        let kind: u8 = row.get(1)?;
        let value: String = row.get(2)?;
        let key = IdentifierKind::from_code(kind).and_then(|kind| identifier_key(kind, &value));
        if let Some(key) = key {
            insert.execute(params![key.rule.code(), key.key, row.get::<_, i64>(0)?])?;
        }
    }
    let mut sources = connection.prepare("SELECT instance_id, record FROM source_records")?;
    let mut rows = sources.query([])?;
    while let Some(row) = rows.next()? {
        let record: Vec<u8> = row.get(1)?;
        if let Some(key) = koha_biblionumber_key(&record) {
            insert.execute(params![key.rule.code(), key.key, row.get::<_, i64>(0)?])?;
        }
    }
    Ok(())
}

/// Reads the notes of every record already stored when `instance_notes` is created. Without
/// them, the export would see the notes of the original as removed and drop them.
fn backfill_notes(connection: &Connection) -> Result<(), StoreError> {
    let mut sources = connection.prepare("SELECT instance_id, record FROM source_records")?;
    let mut rows = sources.query([])?;
    while let Some(row) = rows.next()? {
        let record: Vec<u8> = row.get(1)?;
        insert_notes(connection, row.get(0)?, &source_notes(&record))?;
    }
    Ok(())
}

/// Reads the target audience of every Work already stored when `works.audience` is
/// added, from the source record of its first Instance, for the same reason as the notes.
fn backfill_audience(connection: &Connection) -> Result<(), StoreError> {
    let mut sources = connection.prepare(
        "SELECT instances.work_id, source_records.record FROM source_records
         JOIN instances ON instances.id = source_records.instance_id
         ORDER BY instances.id",
    )?;
    let mut update =
        connection.prepare("UPDATE works SET audience = ?2 WHERE id = ?1 AND audience IS NULL")?;
    let mut rows = sources.query([])?;
    while let Some(row) = rows.next()? {
        let record: Vec<u8> = row.get(1)?;
        if let Some(audience) = source_audience(&record) {
            update.execute(params![row.get::<_, i64>(0)?, audience])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("elzevir.db");
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
        let path = dir.path().join("elzevir.db");
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

#[cfg(test)]
mod catalog_tests {
    use rusqlite::{Connection, params};

    use super::*;

    #[test]
    fn migrations_6_and_7_read_notes_and_audience_from_the_source_record() {
        use elzevir_marc::{Field, Record, Subfield, Tag};
        let field = |tag: &[u8; 3], text: &str| Field::Data {
            tag: Tag::new(*tag),
            indicators: *b"  ",
            subfields: vec![Subfield {
                code: b'a',
                value: text.as_bytes().to_vec(),
            }],
        };
        let mut source = Vec::new();
        Record {
            leader: *b"00000nam a2200000 a 4500",
            fields: vec![
                field(b"245", "Dom Casmurro"),
                field(b"500", "Inclui índice."),
                field(b"521", "Ensino médio."),
            ],
        }
        .write_iso2709(&mut source)
        .unwrap();
        let (_dir, mut connection) = catalog();
        connection
            .execute(
                "UPDATE source_records SET record = ?1 WHERE instance_id = 1",
                params![source],
            )
            .unwrap();
        connection
            .execute_batch(
                "DROP TABLE instance_notes; ALTER TABLE works DROP COLUMN audience;
                 PRAGMA user_version = 5;",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let read =
            |sql: &str| -> String { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
        assert_eq!(read("SELECT text FROM instance_notes"), "Inclui índice.");
        assert_eq!(read("SELECT audience FROM works"), "Ensino médio");
    }

    fn catalog() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let connection = open_writer(&dir.path().join("elzevir.db")).unwrap();
        connection
            .execute_batch(
                "INSERT INTO branches (id, code, name) VALUES (1, 'MAIN', 'Main');
                 INSERT INTO item_types (id, code, description) VALUES (1, 'BK', 'Books');
                 INSERT INTO works (id, title) VALUES (1, 'Dom Casmurro');
                 INSERT INTO instances (id, work_id, title) VALUES (1, 1, 'Dom Casmurro');
                 INSERT INTO instance_identifiers (instance_id, position, kind, value)
                     VALUES (1, 0, 1, '9788535910667');
                 INSERT INTO source_records (instance_id, format, record) VALUES (1, 1, x'00');",
            )
            .unwrap();
        (dir, connection)
    }

    fn item(
        connection: &Connection,
        instance: i64,
        barcode: Option<&str>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO items (instance_id, barcode, home_branch, holding_branch, item_type)
             VALUES (?1, ?2, 1, 1, 1)",
            params![instance, barcode],
        )
    }

    #[test]
    fn deleting_what_is_in_use_is_refused() {
        let (_dir, connection) = catalog();
        item(&connection, 1, Some("1511")).unwrap();
        assert!(
            connection
                .execute("DELETE FROM works WHERE id = 1", [])
                .is_err()
        );
        assert!(
            connection
                .execute("DELETE FROM instances WHERE id = 1", [])
                .is_err()
        );
        assert!(
            connection
                .execute("DELETE FROM branches WHERE id = 1", [])
                .is_err()
        );
        assert!(
            connection
                .execute("DELETE FROM item_types WHERE id = 1", [])
                .is_err()
        );
    }

    #[test]
    fn deleting_an_instance_without_items_takes_its_identifiers_and_source() {
        let (_dir, connection) = catalog();
        connection
            .execute("DELETE FROM instances WHERE id = 1", [])
            .unwrap();
        for table in ["instance_identifiers", "source_records"] {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn barcodes_are_unique_but_optional() {
        let (_dir, connection) = catalog();
        item(&connection, 1, Some("1511")).unwrap();
        assert!(item(&connection, 1, Some("1511")).is_err());
        item(&connection, 1, None).unwrap();
        item(&connection, 1, None).unwrap();
    }

    #[test]
    fn identities_of_terms_and_subjects() {
        let (_dir, connection) = catalog();
        // The same code in two vocabularies is two terms.
        connection
            .execute_batch(
                "INSERT INTO terms (vocabulary, code, label) VALUES (3, 'REF', 'Reference');
                 INSERT INTO terms (id, vocabulary, code, label) VALUES (9, 4, 'REF', 'Reference');",
            )
            .unwrap();
        assert!(
            connection
                .execute(
                    "INSERT INTO terms (vocabulary, code, label) VALUES (3, 'REF', 'x')",
                    []
                )
                .is_err()
        );
        let subject = |thesaurus: Option<i64>| {
            connection.execute(
                "INSERT INTO subjects (kind, heading, thesaurus) VALUES (1, 'Literatura', ?1)",
                params![thesaurus],
            )
        };
        subject(None).unwrap();
        subject(Some(9)).unwrap();
        assert!(
            subject(None).is_err(),
            "two subjects without thesaurus are the same"
        );
        assert!(subject(Some(9)).is_err());
    }

    #[test]
    fn work_relations_and_hubs_keep_the_graph_consistent() {
        let (_dir, connection) = catalog();
        connection
            .execute_batch(
                "INSERT INTO hubs (id, title) VALUES (1, 'Dom Casmurro');
                 UPDATE works SET hub_id = 1 WHERE id = 1;
                 INSERT INTO works (id, title, hub_id) VALUES (2, 'Dom Casmurro (inglês)', 1);
                 INSERT INTO work_relations (work_id, position, kind, related_work_id)
                     VALUES (2, 0, 1, 1);",
            )
            .unwrap();
        let relate = |from: i64, to: i64| {
            connection.execute(
                "INSERT INTO work_relations (work_id, position, kind, related_work_id)
                 VALUES (?1, 1, 1, ?2)",
                params![from, to],
            )
        };
        assert!(relate(1, 1).is_err(), "a Work never relates to itself");
        assert!(relate(1, 99).is_err(), "the related Work must exist");
        assert!(
            connection
                .execute("DELETE FROM works WHERE id = 1", [])
                .is_err(),
            "a Work that others relate to cannot be deleted"
        );
        assert!(
            connection
                .execute("DELETE FROM hubs WHERE id = 1", [])
                .is_err(),
            "a Hub with Works cannot be deleted"
        );
        connection
            .execute("DELETE FROM works WHERE id = 2", [])
            .unwrap();
        let relations: i64 = connection
            .query_row("SELECT count(*) FROM work_relations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(relations, 0, "deleting a Work takes its own relations");
    }

    #[test]
    fn inventory_numbers_may_repeat() {
        let (_dir, connection) = catalog();
        for barcode in ["1", "2"] {
            connection
                .execute(
                    "INSERT INTO items (instance_id, barcode, inventory_number, home_branch,
                                        holding_branch, item_type)
                     VALUES (1, ?1, '2024/0001', 1, 1, 1)",
                    params![barcode],
                )
                .unwrap();
        }
    }

    #[test]
    fn extra_must_be_json() {
        let (_dir, connection) = catalog();
        assert!(
            connection
                .execute(
                    "INSERT INTO works (title, extra) VALUES ('x', 'not json')",
                    []
                )
                .is_err()
        );
    }
}
