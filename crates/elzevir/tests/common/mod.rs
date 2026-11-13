// SPDX-License-Identifier: GPL-3.0-or-later
//! Setup shared by the end-to-end tests: the 200 records of the staff export, imported.

use std::collections::BTreeSet;

use elzevir_app::catalog::CatalogCommand;
use elzevir_app::commands::WriteCommand;
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_marc::{Limits, Record, RecordRef};
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");

/// A store with the 200 records of the staff export, and the temporary directory it lives in.
pub(crate) fn imported() -> (tempfile::TempDir, ElzevirStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    let mut branches = BTreeSet::new();
    let mut item_types = BTreeSet::new();
    for raw in STAFF_EXPORT
        .split_inclusive(|&b| b == 0x1d)
        .filter(|raw| raw.len() > 1)
    {
        let mut record = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
        record.marc21_to_utf8();
        for item in marc_to_bibframe(&record, None).unwrap().items {
            branches.insert(item.home_branch);
            branches.insert(item.holding_branch);
            item_types.insert(item.item_type);
        }
    }
    let setup = branches
        .into_iter()
        .map(|code| CatalogCommand::CreateBranch {
            name: code.clone(),
            code,
        })
        .chain(
            item_types
                .into_iter()
                .map(|code| CatalogCommand::CreateItemType {
                    description: code.clone(),
                    code,
                }),
        );
    for command in setup {
        store
            .submit(WriteCommand::Catalog(command))
            .unwrap()
            .wait()
            .unwrap();
    }
    let report = import_marc21(STAFF_EXPORT, &store, &ImportOptions::default()).unwrap();
    assert_eq!(report.imported, 200);
    (dir, store)
}
