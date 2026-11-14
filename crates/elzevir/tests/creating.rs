// SPDX-License-Identifier: GPL-3.0-or-later
//! A record created from the staff screen exports valid MARC and is found by the search
//! (docs/specs/catalogo/criacao.md, the done criterion of task 2.6).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::num::NonZeroUsize;

use elzevir_app::catalog::{
    CatalogOutput, CatalogRefusal, InstanceChange, NewContribution, NewIdentifier,
    NewInstanceOfWork, WorkChange,
};
use elzevir_app::creating::{new_instance, new_record};
use elzevir_app::editing::{CatalogEditor, Edit, EditFailed};
use elzevir_app::export::{RecordFormat, export_record};
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, SearchIndex, index_pending};
use elzevir_app::staff::StaffId;
use elzevir_domain::catalog::{AgentKind, IdentifierKind, InstanceId, MaterialType, WorkId};
use elzevir_marc::{Limits, MarcReader};
use elzevir_search::TantivyIndex;
use elzevir_store::ElzevirStore;

use common::imported;

const CATALOGUER: StaffId = StaffId(7);

fn create(store: &ElzevirStore, edit: Edit) -> CatalogOutput {
    store.apply(CATALOGUER, edit).unwrap()
}

/// Tag and text of each field of one exported ISO 2709 record.
fn exported_fields(store: &ElzevirStore, instance: InstanceId) -> Vec<(String, String)> {
    let bytes = export_record(store, instance, RecordFormat::Marc21, &Limits::default())
        .unwrap()
        .unwrap();
    let mut reader = MarcReader::new(bytes.as_slice(), Limits::default());
    let record = reader.next_record().unwrap().unwrap();
    let fields = record
        .fields()
        .map(|field| {
            (
                field.tag().to_string(),
                String::from_utf8_lossy(field.raw()).into_owned(),
            )
        })
        .collect();
    assert!(reader.next_record().is_none(), "one record per export");
    fields
}

fn text_of(fields: &[(String, String)], tag: &str) -> String {
    fields
        .iter()
        .filter(|field| field.0 == tag)
        .map(|field| field.1.as_str())
        .collect()
}

#[test]
fn a_family_and_an_instance_created_on_screen_export_and_are_found() {
    let (_dir, store) = imported();
    let record = new_record(
        vec![
            WorkChange::Title("Esaú e Jacó".to_owned()),
            WorkChange::MaterialType(MaterialType::Book),
            WorkChange::Languages(vec!["por".to_owned()]),
            WorkChange::Contributions(vec![NewContribution {
                kind: AgentKind::Person,
                heading: "Assis, Machado de, 1839-1908".to_owned(),
                role: Some("aut".to_owned()),
                primary: true,
            }]),
        ],
        vec![
            InstanceChange::Publisher(Some("Garnier".to_owned())),
            InstanceChange::Date(Some("1904".to_owned())),
            InstanceChange::Identifiers(vec![NewIdentifier {
                kind: IdentifierKind::Isbn,
                value: "978-85-359-1066-7".to_owned(),
                qualifier: None,
            }]),
        ],
    )
    .unwrap();
    let CatalogOutput::Record { work, instance, .. } =
        create(&store, Edit::NewRecord(Box::new(record)))
    else {
        panic!("a new family answers with its record")
    };
    let second = new_instance(vec![
        InstanceChange::Title("Esaú e Jacó".to_owned()),
        InstanceChange::Edition(Some("2. ed.".to_owned())),
        InstanceChange::Identifiers(vec![NewIdentifier {
            kind: IdentifierKind::Isbn,
            value: "85-7139-058-4".to_owned(),
            qualifier: None,
        }]),
    ])
    .unwrap();
    let CatalogOutput::InstanceAdded {
        work: of,
        instance: added,
    } = create(
        &store,
        Edit::NewInstance(Box::new(NewInstanceOfWork {
            work,
            instance: second,
        })),
    )
    else {
        panic!("a new Instance answers with its id")
    };
    assert_eq!(of, work);
    // The new Instance's ISBN is a match key, as on import.
    let keys: i64 = store
        .readers()
        .read(|c| {
            c.query_row(
                "SELECT count(*) FROM match_keys WHERE instance_id = ?1",
                [added.0],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(keys, 1);

    // Valid MARC: it parses, and the mapped fields are there.
    let first = exported_fields(&store, instance);
    assert!(text_of(&first, "245").contains("Esaú e Jacó"));
    assert!(text_of(&first, "100").contains("Assis, Machado de"));
    assert!(text_of(&first, "020").contains("9788535910667"));
    let other = exported_fields(&store, added);
    assert!(text_of(&other, "250").contains("2. ed."));
    assert!(text_of(&other, "100").contains("Assis, Machado de"));

    // Both are queued for the index and found once it runs.
    let index = TantivyIndex::in_memory(Parallelism::Serial).unwrap();
    let options = IndexOptions {
        parallelism: Parallelism::Serial,
        batch: NonZeroUsize::new(1024).unwrap(),
    };
    index_pending(&store, &store, &index, &options).unwrap();
    let found: Vec<InstanceId> = index
        .search("esau jaco", 0, 10)
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.instance)
        .collect();
    assert!(
        found.contains(&instance) && found.contains(&added),
        "{found:?}"
    );
}

#[test]
fn an_instance_of_an_unknown_work_is_refused() {
    let (_dir, store) = imported();
    let instance = new_instance(vec![InstanceChange::Title("Sem Obra".to_owned())]).unwrap();
    let refused = store.apply(
        CATALOGUER,
        Edit::NewInstance(Box::new(NewInstanceOfWork {
            work: WorkId(999_999),
            instance,
        })),
    );
    assert_eq!(
        refused,
        Err(EditFailed::Refused(CatalogRefusal::UnknownWork(WorkId(
            999_999
        ))))
    );
}
