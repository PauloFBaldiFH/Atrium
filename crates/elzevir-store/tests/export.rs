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
