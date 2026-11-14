// SPDX-License-Identifier: GPL-3.0-or-later
//! Matching imported records with stored ones (docs/specs/catalogo/casamento.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use std::collections::BTreeSet;

use elzevir_app::catalog::{CatalogCommand, CatalogRefusal};
use elzevir_app::commands::WriteCommand;
use elzevir_app::import::{ImportOptions, ImportReport, import_marc21};
use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_app::matching::{ItemAction, Matching, OnMatch, OnNoMatch};
use elzevir_domain::catalog::MatchRule;
use elzevir_marc::{Field, Limits, Record, RecordRef};
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");

fn open(dir: &tempfile::TempDir) -> ElzevirStore {
    ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap()
}

fn records(file: &[u8]) -> Vec<&[u8]> {
    file.split_inclusive(|&b| b == 0x1d)
        .filter(|raw| raw.len() > 1)
        .collect()
}

/// A store with the branches and item types the export refers to.
fn ready(dir: &tempfile::TempDir) -> ElzevirStore {
    let store = open(dir);
    let mut branches = BTreeSet::new();
    let mut item_types = BTreeSet::new();
    for raw in records(STAFF_EXPORT) {
        let mut record = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
        record.marc21_to_utf8();
        for item in marc_to_bibframe(&record, None).unwrap().items {
            branches.insert(item.home_branch);
            branches.insert(item.holding_branch);
            item_types.insert(item.item_type);
        }
    }
    let commands = branches
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
    for command in commands {
        store
            .submit(WriteCommand::Catalog(command))
            .unwrap()
            .wait()
            .unwrap();
    }
    store
}

fn import(store: &ElzevirStore, file: &[u8], matching: Option<Matching>) -> ImportReport {
    let options = ImportOptions {
        matching,
        ..ImportOptions::default()
    };
    let report = import_marc21(file, store, &options).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    report
}

fn count(store: &ElzevirStore, sql: &str) -> i64 {
    store
        .readers()
        .read(|c| c.query_row(sql, [], |row| row.get(0)))
        .unwrap()
}

/// Rows of every catalog table, and the highest record version.
fn catalog_state(store: &ElzevirStore) -> Vec<i64> {
    [
        "SELECT count(*) FROM works",
        "SELECT count(*) FROM instances",
        "SELECT count(*) FROM items",
        "SELECT count(*) FROM source_records",
        "SELECT count(*) FROM match_keys",
        "SELECT coalesce(sum(row_version), 0) FROM instances",
    ]
    .iter()
    .map(|sql| count(store, sql))
    .collect()
}

/// The export with its `952` fields removed: the same records, without Items.
fn without_items(file: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for raw in records(file) {
        let mut record = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
        record
            .fields
            .retain(|field| !matches!(field, Field::Data { tag, .. } if tag.as_bytes() == b"952"));
        record.write_iso2709(&mut out).unwrap();
    }
    out
}

fn duplicate_barcodes(report: &ImportReport) -> usize {
    report
        .item_failures
        .iter()
        .filter(|failed| matches!(failed.refusal, CatalogRefusal::BarcodeTaken(_)))
        .count()
}

#[test]
fn reimporting_the_same_file_by_koha_biblionumber_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    let first = import(&store, STAFF_EXPORT, None);
    assert_eq!(first.imported, 200);
    assert_eq!(
        count(&store, "SELECT count(*) FROM match_keys WHERE rule = 3"),
        200
    );
    let before = catalog_state(&store);

    let again = import(
        &store,
        STAFF_EXPORT,
        Some(Matching::new(MatchRule::KohaBiblionumber)),
    );
    assert_eq!(again.read, 200);
    assert_eq!(again.imported, 0);
    assert_eq!(again.items, 0);
    assert_eq!(again.matched.len(), 200);
    // Every Item is offered again and refused alone, because its barcode is taken.
    assert_eq!(again.item_failures.len(), 253);
    assert_eq!(duplicate_barcodes(&again), 253);
    assert_eq!(catalog_state(&store), before);
    // Each record matched the Instance stored for it, in file order.
    let instances: Vec<i64> = again.matched.iter().map(|m| m.instance.0).collect();
    assert_eq!(instances, (1..=200).collect::<Vec<_>>());
}

#[test]
fn by_isbn_only_the_records_without_one_are_stored_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, STAFF_EXPORT, None);
    let without_isbn = count(
        &store,
        "SELECT count(*) FROM instances
         WHERE id NOT IN (SELECT instance_id FROM match_keys WHERE rule = 1)",
    );
    assert!(without_isbn > 0 && without_isbn < 200, "{without_isbn}");

    let again = import(&store, STAFF_EXPORT, Some(Matching::new(MatchRule::Isbn)));
    assert_eq!(i64::try_from(again.imported).unwrap(), without_isbn);
    assert_eq!(
        i64::try_from(again.matched.len()).unwrap(),
        200 - without_isbn
    );
    assert_eq!(
        count(&store, "SELECT count(*) FROM instances"),
        200 + without_isbn
    );
    // The new records come without Items: every barcode is taken.
    assert_eq!(again.items, 0);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 253);
}

#[test]
fn items_are_added_to_matched_records_only_when_their_barcode_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, &without_items(STAFF_EXPORT), None);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 0);
    let versions = count(&store, "SELECT sum(row_version) FROM instances");
    let matching = Matching {
        items: ItemAction::AddOnlyForMatches,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };

    let first = import(&store, STAFF_EXPORT, Some(matching));
    assert_eq!(first.imported, 0);
    assert_eq!(first.items, 253);
    assert!(first.item_failures.is_empty());
    assert_eq!(count(&store, "SELECT count(*) FROM instances"), 200);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 253);
    // Every Instance that gained Items has a new version, for the OPAC cache.
    let gained = count(&store, "SELECT count(DISTINCT instance_id) FROM items");
    assert_eq!(
        count(&store, "SELECT sum(row_version) FROM instances"),
        versions + gained
    );

    let second = import(&store, STAFF_EXPORT, Some(matching));
    assert_eq!(second.items, 0);
    assert_eq!(duplicate_barcodes(&second), 253);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 253);
}

#[test]
fn records_without_a_match_can_be_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    let half: Vec<u8> = records(STAFF_EXPORT)
        .into_iter()
        .take(100)
        .flatten()
        .copied()
        .collect();
    import(&store, &half, None);
    let matching = Matching {
        on_no_match: OnNoMatch::Ignore,
        items: ItemAction::Ignore,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };

    let report = import(&store, STAFF_EXPORT, Some(matching));
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped, 100);
    assert_eq!(report.matched.len(), 100);
    assert!(report.item_failures.is_empty());
    assert_eq!(count(&store, "SELECT count(*) FROM instances"), 100);
}

#[test]
fn a_match_can_still_be_stored_as_a_new_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, &without_items(STAFF_EXPORT), None);
    let matching = Matching {
        on_match: OnMatch::CreateNew,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };

    let report = import(&store, STAFF_EXPORT, Some(matching));
    assert_eq!(report.imported, 200);
    assert_eq!(report.matched.len(), 200);
    // The Items go with the new records.
    assert_eq!(report.items, 253);
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM items WHERE instance_id <= 200"
        ),
        0
    );
}

#[test]
fn two_copies_in_one_file_become_one_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    let first = *records(STAFF_EXPORT).first().unwrap();
    let twice = [first, first].concat();

    let report = import(
        &store,
        &twice,
        Some(Matching::new(MatchRule::KohaBiblionumber)),
    );
    assert_eq!(report.read, 2);
    assert_eq!(report.imported, 1);
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched.first().unwrap().index, 1);
    assert_eq!(count(&store, "SELECT count(*) FROM instances"), 1);
    // The copy's Items are refused one by one, with where they were.
    let items = usize::try_from(report.items).unwrap();
    assert!(items > 0);
    assert_eq!(report.item_failures.len(), items);
    let failed = report.item_failures.first().unwrap();
    assert_eq!((failed.index, failed.position), (1, 0));
}

#[test]
fn without_a_rule_the_import_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, &without_items(STAFF_EXPORT), None);
    let report = import(&store, &without_items(STAFF_EXPORT), None);
    assert_eq!(report.imported, 200);
    assert!(report.matched.is_empty());
    assert_eq!(count(&store, "SELECT count(*) FROM instances"), 400);
}

#[test]
fn migration_5_computes_the_keys_of_older_databases() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, STAFF_EXPORT, None);
    let keys = count(&store, "SELECT count(*) FROM match_keys");
    drop(store);
    // Back to schema 4, as a database created before matching existed.
    let connection = rusqlite::Connection::open(dir.path().join("elzevir.db")).unwrap();
    connection
        .execute_batch(
            "DROP TABLE match_keys; DROP TABLE instance_notes;
             ALTER TABLE works DROP COLUMN audience; DROP INDEX works_in_listing; DROP INDEX hubs_in_listing; ALTER TABLE works DROP COLUMN sort_key; ALTER TABLE hubs DROP COLUMN sort_key; PRAGMA user_version = 4;",
        )
        .unwrap();
    drop(connection);

    let store = open(&dir);
    assert_eq!(count(&store, "SELECT count(*) FROM match_keys"), keys);
    let before = catalog_state(&store);
    let again = import(
        &store,
        STAFF_EXPORT,
        Some(Matching::new(MatchRule::KohaBiblionumber)),
    );
    assert_eq!(again.imported, 0);
    assert_eq!(catalog_state(&store), before);
}

/// The export with every `tag $code` changed by `change`.
fn edited(file: &[u8], tag: [u8; 3], code: u8, change: impl Fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    for raw in records(file) {
        let mut record = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
        for field in &mut record.fields {
            if let Field::Data {
                tag: field_tag,
                subfields,
                ..
            } = field
                && *field_tag.as_bytes() == tag
            {
                for subfield in subfields.iter_mut().filter(|s| s.code == code) {
                    subfield.value = change(&subfield.value);
                }
            }
        }
        record.write_iso2709(&mut out).unwrap();
    }
    out
}

fn texts(store: &ElzevirStore, sql: &str) -> Vec<String> {
    store
        .readers()
        .read(|c| {
            c.prepare(sql)?
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()
        })
        .unwrap()
}

#[test]
fn a_match_can_replace_the_stored_description() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, STAFF_EXPORT, None);
    let versions = count(&store, "SELECT sum(row_version) FROM instances");
    let renamed = edited(STAFF_EXPORT, *b"245", b'a', |title| {
        [b"Revised: ".as_slice(), title].concat()
    });
    let matching = Matching {
        on_match: OnMatch::Replace,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };

    let report = import(&store, &renamed, Some(matching));
    assert_eq!(report.imported, 0);
    assert_eq!(report.replaced, 200);
    assert_eq!(report.matched.len(), 200);
    assert_eq!(duplicate_barcodes(&report), 253);
    // Same records and Items, new descriptions, new versions.
    assert_eq!(count(&store, "SELECT count(*) FROM works"), 200);
    assert_eq!(count(&store, "SELECT count(*) FROM instances"), 200);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 253);
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM instances WHERE title LIKE 'Revised: %'"
        ),
        200
    );
    assert_eq!(
        count(&store, "SELECT sum(row_version) FROM instances"),
        versions + 200
    );
    // The source record is the imported one, so its keys still match next time.
    let again = import(
        &store,
        &renamed,
        Some(Matching::new(MatchRule::KohaBiblionumber)),
    );
    assert_eq!(again.matched.len(), 200);
    assert_eq!(count(&store, "SELECT count(*) FROM source_records"), 200);
}

#[test]
fn replacing_items_overwrites_them_by_barcode_where_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, STAFF_EXPORT, None);
    let owners = texts(
        &store,
        "SELECT barcode || ':' || instance_id || ':' || id FROM items ORDER BY id",
    );
    let versions = count(&store, "SELECT sum(row_version) FROM instances");
    let recalled = edited(STAFF_EXPORT, *b"952", b'o', |_| b"NEW 001".to_vec());
    let matching = Matching {
        items: ItemAction::Replace,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };

    let report = import(&store, &recalled, Some(matching));
    assert_eq!(report.imported, 0);
    assert_eq!(report.items, 0);
    assert_eq!(report.replaced_items, 253);
    assert!(report.item_failures.is_empty());
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM items WHERE call_number = 'NEW 001'"
        ),
        253
    );
    // Same ids, same Instances; each Instance with Items has a new version.
    assert_eq!(
        texts(
            &store,
            "SELECT barcode || ':' || instance_id || ':' || id FROM items ORDER BY id",
        ),
        owners
    );
    let with_items = count(&store, "SELECT count(DISTINCT instance_id) FROM items");
    assert_eq!(
        count(&store, "SELECT sum(row_version) FROM instances"),
        versions + with_items
    );
}

#[test]
fn replacing_items_of_a_record_without_a_match_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    let matching = Matching {
        items: ItemAction::Replace,
        ..Matching::new(MatchRule::KohaBiblionumber)
    };
    let report = import(&store, STAFF_EXPORT, Some(matching));
    assert_eq!(report.imported, 200);
    assert_eq!(report.items, 0);
    assert_eq!(count(&store, "SELECT count(*) FROM items"), 0);
}

#[test]
fn the_replace_command_keeps_ids_and_items_and_recomputes_keys() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready(&dir);
    import(&store, STAFF_EXPORT, None);
    let raw = *records(STAFF_EXPORT).get(1).unwrap();
    let mut marc = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
    marc.marc21_to_utf8();
    let mut source = Vec::new();
    marc.write_iso2709(&mut source).unwrap();
    let second = marc_to_bibframe(&marc, Some(source)).unwrap();
    let items_of_1 = count(&store, "SELECT count(*) FROM items WHERE instance_id = 1");

    let replace = |instance| {
        store
            .submit(WriteCommand::Catalog(CatalogCommand::ReplaceRecord {
                instance: elzevir_domain::catalog::InstanceId(instance),
                record: Box::new(second.clone()),
            }))
            .unwrap()
            .wait()
    };
    replace(1).unwrap();
    let title_of_2 = texts(&store, "SELECT title FROM instances WHERE id = 2");
    assert_eq!(
        texts(&store, "SELECT title FROM instances WHERE id = 1"),
        title_of_2
    );
    assert_eq!(
        count(&store, "SELECT count(*) FROM items WHERE instance_id = 1"),
        items_of_1
    );
    assert_eq!(
        texts(
            &store,
            "SELECT rule || ':' || key FROM match_keys WHERE instance_id = 1 ORDER BY 1"
        ),
        texts(
            &store,
            "SELECT rule || ':' || key FROM match_keys WHERE instance_id = 2 ORDER BY 1"
        )
    );
    assert!(matches!(
        replace(9999),
        Err(elzevir_store::CommandFailed::Rejected(
            elzevir_app::commands::WriteRefusal::Catalog(CatalogRefusal::UnknownInstance(_))
        ))
    ));
}
