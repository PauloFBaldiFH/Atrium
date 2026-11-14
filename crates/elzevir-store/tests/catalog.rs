// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog commands and the catalog read port, through the real writer
//! (docs/specs/catalogo/modelo-bibliografico.md).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_app::catalog::{
    CatalogCommand, CatalogOutput, CatalogReader, CatalogRefusal, ItemData, ItemStatus, ItemView,
    NewContribution, NewIdentifier, NewInstance, NewItem, NewRecord, NewSubject, NewWorkRelation,
};
use elzevir_app::commands::{WriteCommand, WriteOutput, WriteRefusal};
use elzevir_domain::catalog::{
    AgentKind, HubId, IdentifierKind, InstanceId, MaterialType, Money, SubjectKind, WorkId,
    WorkRelationKind,
};
use elzevir_store::{CommandFailed, ElzevirHandler, ElzevirStore, StoreConfig};

fn open(dir: &tempfile::TempDir) -> ElzevirStore {
    ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap()
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

fn refusal(refusal: CatalogRefusal) -> Result<CatalogOutput, CommandFailed<WriteRefusal>> {
    Err(CommandFailed::Rejected(WriteRefusal::Catalog(refusal)))
}

fn setup(store: &ElzevirStore) {
    run(
        store,
        CatalogCommand::CreateBranch {
            code: "MAIN".to_owned(),
            name: "Castro Alves".to_owned(),
        },
    )
    .unwrap();
    run(
        store,
        CatalogCommand::CreateItemType {
            code: "BK".to_owned(),
            description: "Livro".to_owned(),
        },
    )
    .unwrap();
}

fn contribution(heading: &str, primary: bool) -> NewContribution {
    NewContribution {
        kind: AgentKind::Person,
        heading: heading.to_owned(),
        role: Some("aut".to_owned()),
        primary,
    }
}

fn subject(heading: &str) -> NewSubject {
    NewSubject {
        kind: SubjectKind::Topic,
        heading: heading.to_owned(),
        thesaurus: Some("lcsh".to_owned()),
    }
}

fn record(title: &str) -> NewRecord {
    NewRecord {
        work_title: title.to_owned(),
        material_type: MaterialType::Book,
        hub: None,
        relations: Vec::new(),
        classification: Some("869.3".to_owned()),
        classification_scheme: Some("ddc".to_owned()),
        summary: None,
        audience: None,
        languages: vec!["por".to_owned(), "eng".to_owned()],
        contributions: vec![
            contribution("Assis, Machado de, 1839-1908", true),
            contribution("Bosi, Alfredo", false),
        ],
        subjects: vec![subject("Ficção brasileira"), subject("Rio de Janeiro (RJ)")],
        items: Vec::new(),
        source: None,
        instance: NewInstance {
            title: title.to_owned(),
            publisher: Some("Garnier".to_owned()),
            date: Some("1899".to_owned()),
            identifiers: vec![
                NewIdentifier {
                    kind: IdentifierKind::Isbn,
                    value: "9788535910667".to_owned(),
                    qualifier: None,
                },
                NewIdentifier {
                    kind: IdentifierKind::SystemControlNumber,
                    value: "(OCoLC)123".to_owned(),
                    qualifier: None,
                },
            ],
            ..NewInstance::default()
        },
    }
}

fn add_record(store: &ElzevirStore, title: &str) -> (WorkId, InstanceId) {
    match run(store, CatalogCommand::AddRecord(Box::new(record(title)))).unwrap() {
        CatalogOutput::Record { work, instance, .. } => (work, instance),
        other => panic!("unexpected output {other:?}"),
    }
}

fn item(instance: InstanceId, barcode: &str) -> NewItem {
    NewItem {
        instance,
        item: item_data(barcode),
    }
}

fn item_data(barcode: &str) -> ItemData {
    ItemData {
        barcode: Some(barcode.to_owned()),
        inventory_number: Some(format!("2026/{barcode}")),
        home_branch: "MAIN".to_owned(),
        holding_branch: "MAIN".to_owned(),
        item_type: "BK".to_owned(),
        call_number: Some("869.3 A848d".to_owned()),
        location: Some("GEN".to_owned()),
        collection: None,
        price: Some(Money(4590)),
        replacement_price: None,
        copy_number: None,
        status: ItemStatus::default(),
        acquired_on: None,
    }
}

fn changed(instance: InstanceId, barcode: &str, change: impl FnOnce(&mut ItemData)) -> NewItem {
    let mut new = item(instance, barcode);
    change(&mut new.item);
    new
}

#[test]
fn record_reads_back_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (work, instance) = add_record(&store, "Dom Casmurro");

    let view = store.instance(instance).unwrap().unwrap();
    assert_eq!(view.id, instance);
    assert_eq!(view.work, work);
    assert_eq!(view.work_title, "Dom Casmurro");
    assert_eq!(view.publisher.as_deref(), Some("Garnier"));
    assert_eq!(view.date.as_deref(), Some("1899"));
    assert_eq!(view.languages, ["por", "eng"]);
    assert_eq!(
        view.contributors,
        [
            (AgentKind::Person, "Assis, Machado de, 1839-1908".to_owned()),
            (AgentKind::Person, "Bosi, Alfredo".to_owned()),
        ]
    );
    assert_eq!(
        view.subjects,
        [
            (SubjectKind::Topic, "Ficção brasileira".to_owned()),
            (SubjectKind::Topic, "Rio de Janeiro (RJ)".to_owned()),
        ]
    );
    assert_eq!(
        view.identifiers,
        [
            (IdentifierKind::Isbn, "9788535910667".to_owned()),
            (IdentifierKind::SystemControlNumber, "(OCoLC)123".to_owned()),
        ]
    );
    assert_eq!(store.instance(InstanceId(instance.0 + 1)).unwrap(), None);
}

#[test]
fn agents_subjects_and_terms_are_shared_between_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let (first_work, first) = add_record(&store, "Dom Casmurro");
    let (second_work, second) = add_record(&store, "Quincas Borba");
    assert_ne!(first_work, second_work);
    assert_ne!(first, second);
    drop(store);

    let connection = rusqlite::Connection::open(dir.path().join("elzevir.db")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(count("SELECT count(*) FROM agents"), 2);
    assert_eq!(count("SELECT count(*) FROM subjects"), 2);
    assert_eq!(count("SELECT count(*) FROM contributions"), 4);
    // por, eng, aut, lcsh and ddc, each created once with its code as label.
    assert_eq!(count("SELECT count(*) FROM terms"), 5);
    assert_eq!(count("SELECT count(*) FROM terms WHERE label <> code"), 0);
}

#[test]
fn items_are_added_and_listed_in_creation_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let (_, instance) = add_record(&store, "Dom Casmurro");

    let first = run(
        &store,
        CatalogCommand::AddItem(Box::new(item(instance, "0001"))),
    )
    .unwrap();
    let second = run(
        &store,
        CatalogCommand::AddItem(Box::new(item(instance, "0002"))),
    )
    .unwrap();
    let (CatalogOutput::Item(first), CatalogOutput::Item(second)) = (first, second) else {
        panic!("unexpected outputs");
    };

    let items = store.items(instance).unwrap();
    let expected = |id, barcode: &str| ItemView {
        id,
        barcode: Some(barcode.to_owned()),
        inventory_number: Some(format!("2026/{barcode}")),
        home_branch: "MAIN".to_owned(),
        item_type: "BK".to_owned(),
        call_number: Some("869.3 A848d".to_owned()),
        location: Some("GEN".to_owned()),
        status: ItemStatus::default(),
    };
    assert_eq!(items, [expected(first, "0001"), expected(second, "0002")]);
    assert!(store.items(InstanceId(instance.0 + 1)).unwrap().is_empty());
}

#[test]
fn adding_an_item_moves_the_record_version_and_statuses_come_per_page() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let (_, first) = add_record(&store, "Dom Casmurro");
    let (_, second) = add_record(&store, "Quincas Borba");
    let before = store.record_version(first).unwrap().unwrap();
    run(
        &store,
        CatalogCommand::AddItem(Box::new(item(first, "0001"))),
    )
    .unwrap();
    let lost = changed(first, "0002", |data| data.status.lost = 1);
    run(&store, CatalogCommand::AddItem(Box::new(lost))).unwrap();
    assert!(store.record_version(first).unwrap().unwrap() > before);
    assert_eq!(
        store.record_version(InstanceId(second.0 + 1)).unwrap(),
        None
    );

    let mut statuses = store.item_statuses(&[first, second]).unwrap();
    statuses.sort_by_key(|(_, status)| status.lost);
    assert_eq!(
        statuses,
        [
            (first, ItemStatus::default()),
            (
                first,
                ItemStatus {
                    lost: 1,
                    ..ItemStatus::default()
                }
            ),
        ]
    );
    assert!(store.item_statuses(&[]).unwrap().is_empty());
}

#[test]
fn item_refusals_name_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let (_, instance) = add_record(&store, "Dom Casmurro");
    run(
        &store,
        CatalogCommand::AddItem(Box::new(item(instance, "0001"))),
    )
    .unwrap();

    let add = |item: NewItem| run(&store, CatalogCommand::AddItem(Box::new(item)));
    assert_eq!(
        add(item(instance, "0001")),
        refusal(CatalogRefusal::BarcodeTaken("0001".to_owned()))
    );
    let missing = InstanceId(instance.0 + 1);
    assert_eq!(
        add(item(missing, "0002")),
        refusal(CatalogRefusal::UnknownInstance(missing))
    );
    assert_eq!(
        add(changed(instance, "0003", |i| i.home_branch = "NOPE".to_owned())),
        refusal(CatalogRefusal::UnknownBranch("NOPE".to_owned()))
    );
    assert_eq!(
        add(changed(instance, "0004", |i| i.holding_branch = "AWAY".to_owned())),
        refusal(CatalogRefusal::UnknownBranch("AWAY".to_owned()))
    );
    assert_eq!(
        add(changed(instance, "0005", |i| i.item_type = "DVD".to_owned())),
        refusal(CatalogRefusal::UnknownItemType("DVD".to_owned()))
    );
    assert_eq!(store.items(instance).unwrap().len(), 1);
}

#[test]
fn branch_and_item_type_codes_are_unique() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    assert_eq!(
        run(
            &store,
            CatalogCommand::CreateBranch {
                code: "MAIN".to_owned(),
                name: "Other".to_owned(),
            }
        ),
        refusal(CatalogRefusal::CodeTaken("MAIN".to_owned()))
    );
    assert_eq!(
        run(
            &store,
            CatalogCommand::CreateItemType {
                code: "BK".to_owned(),
                description: "Other".to_owned(),
            }
        ),
        refusal(CatalogRefusal::CodeTaken("BK".to_owned()))
    );
}

#[test]
fn hubs_relations_and_material_types_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let CatalogOutput::Hub(hub) = run(
        &store,
        CatalogCommand::CreateHub {
            title: "Dom Casmurro".to_owned(),
        },
    )
    .unwrap() else {
        panic!("expected a hub");
    };
    let (original, original_instance) = add_record(&store, "Dom Casmurro");
    let mut translation = record("Dom Casmurro (English)");
    translation.hub = Some(hub);
    translation.relations = vec![NewWorkRelation {
        kind: WorkRelationKind::TranslationOf,
        work: original,
    }];
    translation.instance.material_type = Some(MaterialType::Digital);
    let CatalogOutput::Record { work, instance, .. } =
        run(&store, CatalogCommand::AddRecord(Box::new(translation))).unwrap()
    else {
        panic!("expected a record");
    };

    let view = store.instance(instance).unwrap().unwrap();
    assert_eq!(view.work, work);
    assert_eq!(view.hub, Some(hub));
    assert_eq!(view.work_material_type, Some(MaterialType::Book));
    assert_eq!(view.material_type, Some(MaterialType::Digital));
    assert_eq!(
        view.relations,
        [(WorkRelationKind::TranslationOf, original)]
    );

    let plain = store.instance(original_instance).unwrap().unwrap();
    assert_eq!(plain.hub, None);
    assert_eq!(
        plain.material_type,
        Some(MaterialType::Book),
        "an Instance without its own category takes the Work's"
    );
    assert!(plain.relations.is_empty());
}

#[test]
fn records_pointing_to_missing_hubs_or_works_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let mut with_hub = record("x");
    with_hub.hub = Some(HubId(7));
    assert_eq!(
        run(&store, CatalogCommand::AddRecord(Box::new(with_hub))),
        refusal(CatalogRefusal::UnknownHub(HubId(7)))
    );
    let mut with_relation = record("x");
    with_relation.relations = vec![NewWorkRelation {
        kind: WorkRelationKind::AdaptationOf,
        work: WorkId(7),
    }];
    assert_eq!(
        run(&store, CatalogCommand::AddRecord(Box::new(with_relation))),
        refusal(CatalogRefusal::UnknownWork(WorkId(7)))
    );
    // Nothing from the refused commands stayed behind.
    let (work, _) = add_record(&store, "Dom Casmurro");
    assert_eq!(work, WorkId(1));
}

#[test]
fn a_record_brings_its_items_and_source_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    setup(&store);
    let mut new = record("Dom Casmurro");
    new.items = vec![item_data("0001"), item_data("0002")];
    new.source = Some(b"00026nam a2200025 a 4500\x1e\x1d".to_vec());
    let CatalogOutput::Record {
        instance, items, ..
    } = run(&store, CatalogCommand::AddRecord(Box::new(new.clone()))).unwrap()
    else {
        panic!("expected a record");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(
        store
            .items(instance)
            .unwrap()
            .iter()
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        items
    );

    // A repeated barcode refuses the whole record: no second Work, Instance or source.
    new.work_title = "Outra".to_owned();
    assert_eq!(
        run(&store, CatalogCommand::AddRecord(Box::new(new))),
        refusal(CatalogRefusal::BarcodeTaken("0001".to_owned()))
    );
    drop(store);
    let connection = rusqlite::Connection::open(dir.path().join("elzevir.db")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(count("SELECT count(*) FROM works"), 1);
    assert_eq!(count("SELECT count(*) FROM source_records"), 1);
    assert_eq!(count("SELECT count(*) FROM items"), 2);
}
