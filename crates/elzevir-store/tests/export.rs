// SPDX-License-Identifier: GPL-3.0-or-later
//! Exporting the catalog as MARC21 (docs/specs/catalogo/linha-de-comando.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use std::num::NonZeroUsize;

use elzevir_app::catalog::{CatalogCommand, NewItem};
use elzevir_app::commands::WriteCommand;
use elzevir_app::export::{ExportOptions, export_marc21, item_codes};
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_domain::catalog::InstanceId;
use elzevir_marc::Limits;
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");

fn imported(dir: &tempfile::TempDir) -> ElzevirStore {
    let store = ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    let codes = item_codes(STAFF_EXPORT, Limits::default()).unwrap();
    let branches = codes
        .branches
        .into_iter()
        .map(|code| CatalogCommand::CreateBranch {
            name: code.clone(),
            code,
        });
    let types = codes
        .item_types
        .into_iter()
        .map(|code| CatalogCommand::CreateItemType {
            description: code.clone(),
            code,
        });
    for command in branches.chain(types) {
        store
            .submit(WriteCommand::Catalog(command))
            .unwrap()
            .wait()
            .unwrap();
    }
    let report = import_marc21(STAFF_EXPORT, &store, &ImportOptions::default()).unwrap();
    assert_eq!(report.imported, 200);
    store
}

fn export(store: &ElzevirStore, batch: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let options = ExportOptions {
        batch: NonZeroUsize::new(batch).unwrap(),
        ..ExportOptions::default()
    };
    let report = export_marc21(store, &mut out, &options).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.written, 200);
    out
}

#[test]
fn an_imported_koha_export_comes_back_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    assert_eq!(export(&store, 1024), STAFF_EXPORT);
    // Batches split anywhere give the same file.
    assert_eq!(export(&store, 7), STAFF_EXPORT);
}

#[test]
fn an_item_added_after_the_import_is_exported() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let mut first =
        elzevir_app::export::RecordSource::records_after(&store, None, NonZeroUsize::MIN).unwrap();
    let mut item = first.pop().unwrap().record.items.remove(0);
    item.barcode = Some("ELZ-NEW-1".to_owned());
    store
        .submit(WriteCommand::Catalog(CatalogCommand::AddItem(Box::new(
            NewItem {
                instance: InstanceId(1),
                item,
            },
        ))))
        .unwrap()
        .wait()
        .unwrap();
    let out = export(&store, 1024);
    assert_ne!(out, STAFF_EXPORT);
    let found = out.windows(9).filter(|w| *w == b"ELZ-NEW-1").count();
    assert_eq!(found, 1);
}

#[test]
fn an_unchanged_record_goes_out_as_stored_even_when_it_could_not_be_rewritten() {
    // An LC record (Books All 2016, part 1) whose 001 ends with a subfield delimiter:
    // readers accept it, the ISO 2709 writer does not.
    let lc: &[u8] = include_bytes!("data/lc-001-delimiter.mrc");
    let dir = tempfile::tempdir().unwrap();
    let store = ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    assert_eq!(
        import_marc21(lc, &store, &ImportOptions::default())
            .unwrap()
            .imported,
        1
    );
    let mut out = Vec::new();
    let report = export_marc21(&store, &mut out, &ExportOptions::default()).unwrap();
    assert_eq!(report.written, 1);
    assert_eq!(out, lc);
}

#[test]
fn migrations_6_and_7_read_the_notes_and_audience_of_older_databases() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let notes = |store: &ElzevirStore| {
        store
            .readers()
            .read(|c| {
                c.query_row(
                    "SELECT (SELECT count(*) FROM instance_notes),
                            (SELECT count(audience) FROM works)",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
            })
            .unwrap()
    };
    let before = notes(&store);
    // The staff export has notes but no `521`; the audience backfill has a unit test.
    assert!(before.0 > 0);
    drop(store);
    // Back to schema 5, as a database created before notes and audience existed.
    let connection = rusqlite::Connection::open(dir.path().join("elzevir.db")).unwrap();
    connection
        .execute_batch(
            "DROP TABLE instance_notes; ALTER TABLE works DROP COLUMN audience;
             DROP INDEX works_in_listing; DROP INDEX hubs_in_listing; ALTER TABLE works DROP COLUMN sort_key; ALTER TABLE hubs DROP COLUMN sort_key; ALTER TABLE audit_log DROP COLUMN actor; DROP INDEX agents_by_heading; DROP INDEX subjects_by_heading; PRAGMA user_version = 5;",
        )
        .unwrap();
    drop(connection);

    let store = ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    assert_eq!(notes(&store), before);
    // Without them, the export would drop the 500, 505, 590 and 521 of every record.
    assert_eq!(export(&store, 1024), STAFF_EXPORT);
}

#[test]
fn one_record_exports_as_the_batch_does_in_marc21_and_marcxml() {
    use elzevir_app::export::{RecordFormat, export_record};
    use elzevir_marc::{Record, RecordRef};

    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let first = store
        .readers()
        .read(|c| c.query_row("SELECT MIN(id) FROM instances", [], |row| row.get(0)))
        .unwrap();
    let instance = InstanceId(first);
    let marc21 = export_record(&store, instance, RecordFormat::Marc21, &Limits::default())
        .unwrap()
        .unwrap();
    // The first record of the batch export, byte for byte.
    let batch = export(&store, 7);
    assert!(batch.starts_with(&marc21));

    let xml = export_record(&store, instance, RecordFormat::MarcXml, &Limits::default())
        .unwrap()
        .unwrap();
    let xml = String::from_utf8(xml).unwrap();
    let mut expected = String::new();
    let record = RecordRef::parse(&marc21, &Limits::default()).unwrap();
    Record::from_ref(&record)
        .write_marcxml_document(&mut expected)
        .unwrap();
    assert_eq!(xml, expected);

    assert!(
        export_record(
            &store,
            InstanceId(i64::MAX),
            RecordFormat::Marc21,
            &Limits::default()
        )
        .unwrap()
        .is_none()
    );
}
