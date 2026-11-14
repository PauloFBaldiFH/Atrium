// SPDX-License-Identifier: GPL-3.0-or-later
//! An Instance's Items saved as one batch (docs/specs/catalogo/exemplares.md).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_app::catalog::{CatalogCommand, CatalogOutput, CatalogReader, CatalogRefusal};
use elzevir_app::commands::{WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::export::{ExportOptions, export_marc21, item_codes};
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_app::items::{
    ItemColumn, ItemFields, ItemGridSource, ItemRow, ItemsUpdate, RowProblem, RowRefusal,
};
use elzevir_domain::catalog::{InstanceId, ItemId, Money};
use elzevir_marc::{Limits, MarcReader};
use elzevir_store::{CommandFailed, ElzevirHandler, ElzevirStore, StoreConfig};

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
        run(&store, command).unwrap();
    }
    import_marc21(STAFF_EXPORT, &store, &ImportOptions::default()).unwrap();
    store
}

fn run(
    store: &ElzevirStore,
    command: CatalogCommand,
) -> Result<CatalogOutput, CommandFailed<WriteRefusal>> {
    store
        .submit(WriteCommand::Catalog(command))
        .unwrap()
        .wait()
        .map(|output| {
            let WriteOutput::Catalog(output) = output else {
                panic!("unexpected output {output:?}");
            };
            output
        })
}

fn count(store: &ElzevirStore, sql: &str) -> i64 {
    store
        .readers()
        .read(|c| c.query_row(sql, [], |row| row.get(0)))
        .unwrap()
}

/// The first Instance with at least two Items that have barcodes.
fn instance_with_two_items(store: &ElzevirStore) -> InstanceId {
    InstanceId(count(
        store,
        "SELECT instance_id FROM items WHERE barcode IS NOT NULL
         GROUP BY instance_id HAVING count(*) >= 2 ORDER BY instance_id LIMIT 1",
    ))
}

fn save(
    store: &ElzevirStore,
    instance: InstanceId,
    rows: Vec<ItemRow>,
) -> Result<CatalogOutput, CommandFailed<WriteRefusal>> {
    let row_version = store.instance(instance).unwrap().unwrap().version;
    run(
        store,
        CatalogCommand::SaveItems(Box::new(ItemsUpdate {
            instance,
            row_version,
            rows,
        })),
    )
}

fn existing(store: &ElzevirStore, instance: InstanceId) -> Vec<(ItemId, ItemFields)> {
    store
        .grid_items(instance)
        .unwrap()
        .into_iter()
        .map(|item| (item.id, item.fields))
        .collect()
}

fn export_952(store: &ElzevirStore, instance: InstanceId) -> Vec<String> {
    let mut out = Vec::new();
    export_marc21(store, &mut out, &ExportOptions::default()).unwrap();
    let mut reader = MarcReader::new(out.as_slice(), Limits::default());
    let mut position = 0;
    while let Some(record) = reader.next_record() {
        let record = record.unwrap();
        position += 1;
        if position == instance.0 {
            return record
                .fields()
                .filter(|field| field.tag().to_string() == "952")
                .map(|field| String::from_utf8_lossy(field.raw()).into_owned())
                .collect();
        }
    }
    panic!("instance {} not exported", instance.0)
}

#[test]
fn a_batch_edits_and_adds_items_and_the_export_follows() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let instance = instance_with_two_items(&store);
    let before = store.instance(instance).unwrap().unwrap().version;
    let fields_before = export_952(&store, instance);
    let items = existing(&store, instance);
    let (first, mut changed) = items[0].clone();
    changed.call_number = Some("869.3 X1".to_owned());
    let mut new = items[0].1.clone();
    new.barcode = Some("NOVO-0001".to_owned());
    new.inventory_number = Some("2026/0001".to_owned());
    new.price = Some(Money(4590));
    let CatalogOutput::ItemsSaved {
        row_version,
        items: ids,
        ..
    } = save(
        &store,
        instance,
        vec![
            ItemRow {
                id: Some(first),
                fields: changed,
            },
            ItemRow {
                id: None,
                fields: new,
            },
        ],
    )
    .unwrap()
    else {
        panic!("a batch answers with its items")
    };
    assert_eq!(row_version, before + 1);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], first);
    assert_eq!(existing(&store, instance).len(), items.len() + 1);

    let fields_after = export_952(&store, instance);
    assert_eq!(fields_after.len(), fields_before.len() + 1);
    // The edited 952 changed its call number only; the others are as they were.
    assert!(fields_after[0].contains("869.3 X1"));
    assert_eq!(fields_after[1..fields_before.len()], fields_before[1..]);
    assert!(fields_after.last().unwrap().contains("NOVO-0001"));
    assert!(fields_after.last().unwrap().contains("45.90"));
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM audit_log WHERE kind = 'catalog.save_items'"
        ),
        1
    );
}

#[test]
fn a_batch_with_a_bad_cell_writes_nothing_and_names_every_cell() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let instance = instance_with_two_items(&store);
    let items = existing(&store, instance);
    let other: String = store
        .readers()
        .read(|c| {
            c.query_row(
                "SELECT barcode FROM items WHERE barcode IS NOT NULL AND instance_id != ?1
                 ORDER BY id LIMIT 1",
                [instance.0],
                |row| row.get(0),
            )
        })
        .unwrap();
    let mut wrong_branch = items[0].1.clone();
    wrong_branch.home_branch = "NOWHERE".to_owned();
    let mut duplicate = items[1].1.clone();
    duplicate.barcode = Some(other);
    let foreign = ItemId(count(
        &store,
        &format!(
            // The holder of `other`: claiming it must not free its barcode.
            "SELECT id FROM items WHERE barcode IS NOT NULL AND instance_id != {}
             ORDER BY id LIMIT 1",
            instance.0
        ),
    ));
    let total = count(&store, "SELECT count(*) FROM items");
    let refused = save(
        &store,
        instance,
        vec![
            ItemRow {
                id: Some(items[0].0),
                fields: wrong_branch,
            },
            ItemRow {
                id: None,
                fields: duplicate,
            },
            ItemRow {
                id: Some(foreign),
                fields: ItemFields {
                    barcode: None,
                    ..items[1].1.clone()
                },
            },
        ],
    );
    let at = |row, column, problem| RowRefusal {
        row,
        column,
        problem,
    };
    assert_eq!(
        refused,
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::ItemRows(vec![
                at(0, ItemColumn::HomeBranch, RowProblem::UnknownBranch),
                at(1, ItemColumn::Barcode, RowProblem::BarcodeTaken),
                at(2, ItemColumn::Row, RowProblem::UnknownItem),
            ])
        )))
    );
    assert_eq!(count(&store, "SELECT count(*) FROM items"), total);
    assert_eq!(existing(&store, instance), items);
}

#[test]
fn two_items_of_a_batch_may_swap_barcodes() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let instance = instance_with_two_items(&store);
    let items = existing(&store, instance);
    let (a, mut fields_a) = items[0].clone();
    let (b, mut fields_b) = items[1].clone();
    std::mem::swap(&mut fields_a.barcode, &mut fields_b.barcode);
    save(
        &store,
        instance,
        vec![
            ItemRow {
                id: Some(a),
                fields: fields_a.clone(),
            },
            ItemRow {
                id: Some(b),
                fields: fields_b.clone(),
            },
        ],
    )
    .unwrap();
    let after = existing(&store, instance);
    assert_eq!(after[0], (a, fields_a));
    assert_eq!(after[1], (b, fields_b));
}

#[test]
fn a_batch_read_before_another_change_is_stale() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let instance = instance_with_two_items(&store);
    let read = store.instance(instance).unwrap().unwrap().version;
    let items = existing(&store, instance);
    let row = ItemRow {
        id: Some(items[0].0),
        fields: items[0].1.clone(),
    };
    save(&store, instance, vec![row.clone()]).unwrap();
    let stale = run(
        &store,
        CatalogCommand::SaveItems(Box::new(ItemsUpdate {
            instance,
            row_version: read,
            rows: vec![row],
        })),
    );
    assert_eq!(
        stale,
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::Stale {
                read,
                current: read + 1,
            }
        )))
    );
}
