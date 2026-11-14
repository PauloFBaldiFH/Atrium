// SPDX-License-Identifier: GPL-3.0-or-later
//! The staff listing read from SQLite (docs/specs/catalogo/listagem.md).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test: helper functions fail the test on error"
)]

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use elzevir_app::catalog::{
    CatalogCommand, CatalogOutput, CatalogReader, ItemData, ItemStatus, NewContribution,
    NewIdentifier, NewInstance, NewRecord, ShelfSignal, WorkChange, WorkUpdate,
};
use elzevir_app::commands::{WriteCommand, WriteOutput};
use elzevir_app::export::item_codes;
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_app::listing::{CatalogListing, FamilyCursor, FamilyHead, browse, sort_key};
use elzevir_domain::catalog::{AgentKind, HubId, IdentifierKind, InstanceId, MaterialType, WorkId};
use elzevir_marc::Limits;
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

fn run(store: &ElzevirStore, command: CatalogCommand) -> CatalogOutput {
    match store.submit(WriteCommand::Catalog(command)).unwrap().wait() {
        Ok(WriteOutput::Catalog(output)) => output,
        other => panic!("unexpected {other:?}"),
    }
}

fn imported(dir: &tempfile::TempDir) -> ElzevirStore {
    let store = open(dir);
    let codes = item_codes(STAFF_EXPORT, Limits::default()).unwrap();
    for code in codes.branches {
        run(
            &store,
            CatalogCommand::CreateBranch {
                name: code.clone(),
                code,
            },
        );
    }
    for code in codes.item_types {
        run(
            &store,
            CatalogCommand::CreateItemType {
                description: code.clone(),
                code,
            },
        );
    }
    import_marc21(STAFF_EXPORT, &store, &ImportOptions::default()).unwrap();
    store
}

/// Every family head, one page after another.
fn all_heads(store: &ElzevirStore) -> Vec<FamilyCursor> {
    let mut heads = Vec::new();
    let mut after = None;
    loop {
        let page = browse(store, after.as_ref()).unwrap();
        heads.extend(page.rows.into_iter().map(|row| row.cursor));
        match page.next {
            Some(next) => after = Some(next),
            None => return heads,
        }
    }
}

#[test]
fn the_listing_pages_through_every_record_in_title_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let heads = all_heads(&store);
    assert_eq!(heads.len(), 200);
    assert!(heads.is_sorted(), "families out of order");
    let distinct: BTreeSet<_> = heads.iter().map(|cursor| cursor.head).collect();
    assert_eq!(distinct.len(), 200);
    // The key is the Work's title as the listing sorts it.
    let first = &heads[0];
    let FamilyHead::Work(work) = first.head else {
        panic!("no Hubs in the import");
    };
    let instance = store
        .readers()
        .read(|c| {
            c.query_row(
                "SELECT id FROM instances WHERE work_id = ?1",
                [work.0],
                |row| row.get(0),
            )
        })
        .unwrap();
    let view = store.instance(InstanceId(instance)).unwrap().unwrap();
    assert_eq!(first.sort_key, sort_key(&view.work_title));
}

fn record(title: &str, hub: Option<HubId>) -> NewRecord {
    NewRecord {
        work_title: title.to_owned(),
        material_type: MaterialType::Book,
        hub,
        relations: Vec::new(),
        classification: Some("869.3".to_owned()),
        classification_scheme: Some("ddc".to_owned()),
        summary: None,
        audience: None,
        languages: vec!["por".to_owned()],
        contributions: vec![
            NewContribution {
                kind: AgentKind::Person,
                heading: "Bosi, Alfredo".to_owned(),
                role: None,
                primary: false,
            },
            NewContribution {
                kind: AgentKind::Person,
                heading: "Assis, Machado de".to_owned(),
                role: None,
                primary: true,
            },
        ],
        subjects: Vec::new(),
        items: Vec::new(),
        source: None,
        instance: NewInstance {
            title: title.to_owned(),
            identifiers: vec![NewIdentifier {
                kind: IdentifierKind::Isbn,
                value: "9788535910667".to_owned(),
                qualifier: None,
            }],
            ..NewInstance::default()
        },
    }
}

fn item(inventory: &str, status: ItemStatus) -> ItemData {
    ItemData {
        barcode: None,
        inventory_number: Some(inventory.to_owned()),
        home_branch: "MAIN".to_owned(),
        holding_branch: "MAIN".to_owned(),
        item_type: "BK".to_owned(),
        call_number: Some("869.3 A848d".to_owned()),
        location: None,
        collection: None,
        price: None,
        replacement_price: None,
        copy_number: None,
        status,
        acquired_on: None,
    }
}

fn setup(store: &ElzevirStore) {
    run(
        store,
        CatalogCommand::CreateBranch {
            code: "MAIN".to_owned(),
            name: "Main".to_owned(),
        },
    );
    run(
        store,
        CatalogCommand::CreateItemType {
            code: "BK".to_owned(),
            description: "Book".to_owned(),
        },
    );
}

fn add(store: &ElzevirStore, record: NewRecord) -> WorkId {
    match run(store, CatalogCommand::AddRecord(Box::new(record))) {
        CatalogOutput::Record { work, .. } => work,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_hub_is_one_family_with_all_its_works_before_a_work_of_the_same_title() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let CatalogOutput::Hub(hub) = run(
        &store,
        CatalogCommand::CreateHub {
            title: "Dom Casmurro".to_owned(),
        },
    ) else {
        panic!("no hub");
    };
    let lone = add(&store, record("Dom Casmurro", None));
    let portuguese = add(&store, record("Dom Casmurro", Some(hub)));
    let english = add(&store, record("Dom Casmurro (inglês)", Some(hub)));
    let before = add(&store, record("Ática", None));

    let heads: Vec<FamilyHead> = all_heads(&store)
        .into_iter()
        .map(|cursor| cursor.head)
        .collect();
    assert_eq!(
        heads,
        [
            FamilyHead::Work(before),
            FamilyHead::Hub(hub),
            FamilyHead::Work(lone),
        ]
    );
    let page = browse(&store, None).unwrap();
    let family = &page.rows[1];
    assert_eq!(family.hub_title.as_deref(), Some("Dom Casmurro"));
    let works: Vec<WorkId> = family.works.iter().map(|work| work.id).collect();
    assert_eq!(works, [portuguese, english]);
    // The main entry is shown even when it is not the first contributor.
    assert_eq!(
        family.works[0].main_contributor.as_deref(),
        Some("Assis, Machado de")
    );
    assert_eq!(
        family.works[0].instances[0].isbn.as_deref(),
        Some("9788535910667")
    );

    // One family at a time crosses the Hub/Work boundary on the same key.
    let mut after: Option<FamilyCursor> = None;
    let mut one_by_one = Vec::new();
    loop {
        let next = store
            .families_after(after.as_ref(), NonZeroUsize::MIN)
            .unwrap();
        let Some(family) = next.into_iter().next() else {
            break;
        };
        one_by_one.push(family.cursor.head);
        after = Some(family.cursor);
    }
    assert_eq!(one_by_one, heads);
}

#[test]
fn pills_carry_the_signal_of_each_item() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let mut with_items = record("Memórias póstumas", None);
    let lost = ItemStatus {
        lost: 1,
        ..ItemStatus::default()
    };
    with_items.items = vec![item("0002", lost), item("0001", ItemStatus::default())];
    add(&store, with_items);
    let page = browse(&store, None).unwrap();
    let instance = &page.rows[0].works[0].instances[0];
    let signals: Vec<(Option<&str>, ShelfSignal)> = instance
        .pills
        .iter()
        .map(|pill| (pill.inventory_number.as_deref(), pill.signal))
        .collect();
    assert_eq!(
        signals,
        [
            (Some("0001"), ShelfSignal::Available),
            (Some("0002"), ShelfSignal::OutOfCirculation),
        ]
    );
    assert_eq!((instance.items, instance.available), (2, 1));
}

#[test]
fn renaming_a_work_moves_its_family() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let first = add(&store, record("Alpha", None));
    let second = add(&store, record("Beta", None));
    run(
        &store,
        CatalogCommand::UpdateWork(Box::new(WorkUpdate {
            work: first,
            row_version: 1,
            changes: vec![WorkChange::Title("Zeta".to_owned())],
        })),
    );
    let heads: Vec<FamilyHead> = all_heads(&store)
        .into_iter()
        .map(|cursor| cursor.head)
        .collect();
    assert_eq!(heads, [FamilyHead::Work(second), FamilyHead::Work(first)]);
}

#[test]
fn migration_8_computes_the_keys_of_older_databases() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let before = all_heads(&store);
    drop(store);
    let connection = rusqlite::Connection::open(dir.path().join("elzevir.db")).unwrap();
    connection
        .execute_batch(
            "DROP INDEX works_in_listing; DROP INDEX hubs_in_listing;
             ALTER TABLE works DROP COLUMN sort_key; ALTER TABLE hubs DROP COLUMN sort_key;
             PRAGMA user_version = 7;",
        )
        .unwrap();
    drop(connection);
    let store = open(&dir);
    assert_eq!(all_heads(&store), before);
}
