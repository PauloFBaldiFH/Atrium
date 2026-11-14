// SPDX-License-Identifier: GPL-3.0-or-later
//! Editing a stored Work or Instance in place (docs/specs/catalogo/edicao.md).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test: helper functions fail the test on error"
)]

use std::collections::BTreeSet;

use elzevir_app::catalog::{
    CatalogCommand, CatalogOutput, CatalogReader, CatalogRefusal, InstanceChange, InstanceUpdate,
    InstanceView, NewIdentifier, WorkChange, WorkUpdate,
};
use elzevir_app::commands::{WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::export::{ExportOptions, export_marc21, item_codes};
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_domain::catalog::{IdentifierKind, InstanceId, WorkId};
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

fn view(store: &ElzevirStore, id: i64) -> InstanceView {
    store.instance(InstanceId(id)).unwrap().unwrap()
}

fn update_work(view: &InstanceView, changes: Vec<WorkChange>) -> CatalogCommand {
    CatalogCommand::UpdateWork(Box::new(WorkUpdate {
        work: view.work,
        row_version: view.work_version,
        changes,
    }))
}

fn update_instance(view: &InstanceView, changes: Vec<InstanceChange>) -> CatalogCommand {
    CatalogCommand::UpdateInstance(Box::new(InstanceUpdate {
        instance: view.id,
        row_version: view.version,
        changes,
    }))
}

fn export(store: &ElzevirStore) -> Vec<u8> {
    let mut out = Vec::new();
    let report = export_marc21(store, &mut out, &ExportOptions::default()).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    out
}

/// Each record of an ISO 2709 file as its fields: tag and raw bytes, in order.
fn fields(file: &[u8]) -> Vec<Vec<(String, Vec<u8>)>> {
    let mut reader = MarcReader::new(file, Limits::default());
    let mut records = Vec::new();
    while let Some(record) = reader.next_record() {
        let record = record.unwrap();
        records.push(
            record
                .fields()
                .map(|field| (field.tag().to_string(), field.raw().to_vec()))
                .collect(),
        );
    }
    records
}

fn count(store: &ElzevirStore, sql: &str) -> i64 {
    store
        .readers()
        .read(|c| c.query_row(sql, [], |row| row.get(0)))
        .unwrap()
}

#[test]
fn an_edit_changes_only_the_edited_fields_of_the_exported_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let before = view(&store, 1);
    run(
        &store,
        update_work(
            &before,
            vec![WorkChange::Summary(Some("Resumo revisto.".to_owned()))],
        ),
    )
    .unwrap();
    let before = view(&store, 1);
    run(
        &store,
        update_instance(
            &before,
            vec![InstanceChange::Publisher(Some(
                "Editora Revista".to_owned(),
            ))],
        ),
    )
    .unwrap();

    let original = fields(STAFF_EXPORT);
    let edited = fields(&export(&store));
    assert_eq!(original.len(), edited.len());
    // Every other record goes out byte for byte.
    assert_eq!(original.get(1..), edited.get(1..));
    // In the edited one, only the summary and the publication fields moved.
    let touched = |field: &&(String, Vec<u8>)| ["520", "260", "264"].contains(&field.0.as_str());
    let untouched = |record: &Vec<(String, Vec<u8>)>| {
        record
            .iter()
            .filter(|field| !touched(field))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(untouched(&original[0]), untouched(&edited[0]));
    let text = |record: &Vec<(String, Vec<u8>)>| {
        record
            .iter()
            .filter(touched)
            .map(|field| String::from_utf8_lossy(&field.1).into_owned())
            .collect::<String>()
    };
    assert!(text(&edited[0]).contains("Resumo revisto."));
    assert!(text(&edited[0]).contains("Editora Revista"));
}

#[test]
fn an_edit_read_before_another_one_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let read = view(&store, 1);
    let first = update_work(&read, vec![WorkChange::Title("Primeira".to_owned())]);
    let second = update_work(&read, vec![WorkChange::Title("Segunda".to_owned())]);
    assert_eq!(
        run(&store, first).unwrap(),
        CatalogOutput::WorkUpdated {
            work: read.work,
            row_version: read.work_version + 1,
        }
    );
    let effects = count(&store, "SELECT count(*) FROM effect_outbox");
    let logged = count(&store, "SELECT count(*) FROM audit_log");
    assert_eq!(
        run(&store, second),
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::Stale {
                read: read.work_version,
                current: read.work_version + 1,
            }
        )))
    );
    assert_eq!(view(&store, 1).work_title, "Primeira");
    assert_eq!(count(&store, "SELECT count(*) FROM effect_outbox"), effects);
    assert_eq!(count(&store, "SELECT count(*) FROM audit_log"), logged);

    // The same holds for Instances: the record moved with the Work edit.
    let stale = update_instance(&read, vec![InstanceChange::Date(Some("2001".to_owned()))]);
    assert_eq!(
        run(&store, stale),
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::Stale {
                read: read.version,
                current: read.version + 1,
            }
        )))
    );
}

#[test]
fn a_work_edit_moves_the_version_of_its_record_and_queues_a_reindex() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let read = view(&store, 2);
    let reindexed = || {
        count(
            &store,
            "SELECT count(*) FROM effect_outbox
             WHERE kind = 'catalog.reindex_instance' AND payload = '{\"instance\":2}'",
        )
    };
    let queued = reindexed();
    run(
        &store,
        update_work(
            &read,
            vec![
                WorkChange::Languages(vec!["eng".to_owned()]),
                WorkChange::Audience(Some("Adulto".to_owned())),
            ],
        ),
    )
    .unwrap();
    let after = view(&store, 2);
    assert_eq!(after.version, read.version + 1);
    assert_eq!(after.languages, ["eng"]);
    assert_eq!(after.audience.as_deref(), Some("Adulto"));
    assert_eq!(reindexed(), queued + 1);
    let kind = count(
        &store,
        "SELECT count(*) FROM audit_log WHERE kind = 'catalog.update_work'",
    );
    assert_eq!(kind, 1);
}

#[test]
fn an_identifier_edit_moves_the_match_keys() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let read = view(&store, 3);
    let biblionumber = "SELECT count(*) FROM match_keys WHERE instance_id = 3 AND rule = 3";
    let kept = count(&store, biblionumber);
    run(
        &store,
        update_instance(
            &read,
            vec![InstanceChange::Identifiers(vec![NewIdentifier {
                kind: IdentifierKind::Isbn,
                value: "85-359-0277-5".to_owned(),
                qualifier: None,
            }])],
        ),
    )
    .unwrap();
    let keys = store
        .readers()
        .read(|c| {
            let mut statement = c.prepare(
                "SELECT key FROM match_keys WHERE instance_id = 3 AND rule <> 3 ORDER BY key",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<BTreeSet<_>, _>>()
        })
        .unwrap();
    assert_eq!(keys, ["9788535902778".to_owned()].into());
    assert_eq!(count(&store, biblionumber), kept);
}

#[test]
fn editing_what_does_not_exist_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let missing = CatalogCommand::UpdateWork(Box::new(WorkUpdate {
        work: WorkId(9999),
        row_version: 1,
        changes: vec![WorkChange::Summary(None)],
    }));
    assert_eq!(
        run(&store, missing),
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::UnknownWork(WorkId(9999))
        )))
    );
    let missing = CatalogCommand::UpdateInstance(Box::new(InstanceUpdate {
        instance: InstanceId(9999),
        row_version: 1,
        changes: Vec::new(),
    }));
    assert_eq!(
        run(&store, missing),
        Err(CommandFailed::Rejected(WriteRefusal::Catalog(
            CatalogRefusal::UnknownInstance(InstanceId(9999))
        )))
    );
}

#[test]
fn an_edit_from_the_screen_records_who_made_it() {
    use elzevir_app::editing::{CatalogEditor, Edit, EditFailed};
    use elzevir_app::staff::StaffId;

    let dir = tempfile::tempdir().unwrap();
    let store = imported(&dir);
    let first = view(&store, 1);
    let edit = Edit::Work(WorkUpdate {
        work: first.work,
        row_version: first.work_version,
        changes: vec![WorkChange::Summary(Some("Resumo novo.".to_owned()))],
    });
    let output = store.apply(StaffId(7), edit.clone()).unwrap();
    assert!(matches!(output, CatalogOutput::WorkUpdated { .. }));
    assert_eq!(
        count(
            &store,
            "SELECT actor FROM audit_log WHERE kind = 'catalog.update_work'"
        ),
        7
    );
    // Imports have no actor.
    assert!(count(&store, "SELECT count(*) FROM audit_log WHERE actor IS NULL") > 0);
    // The same edit again was read at the old version.
    assert!(matches!(
        store.apply(StaffId(7), edit),
        Err(EditFailed::Refused(CatalogRefusal::Stale { .. }))
    ));
}
