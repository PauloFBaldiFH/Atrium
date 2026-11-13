// SPDX-License-Identifier: GPL-3.0-or-later
//! Import → outbox → index → search, end to end (docs/specs/catalogo/busca.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use elzevir_app::catalog::{CatalogCommand, CatalogReader};
use elzevir_app::commands::WriteCommand;
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, SearchIndex, index_pending};
use elzevir_domain::catalog::InstanceId;
use elzevir_marc::{Limits, Record, RecordRef};
use elzevir_search::TantivyIndex;
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");

/// A store with the 200 records of the staff export, and the temporary directory it lives in.
fn imported() -> (tempfile::TempDir, ElzevirStore) {
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

fn options(parallelism: Parallelism, batch: usize) -> IndexOptions {
    IndexOptions {
        parallelism,
        batch: NonZeroUsize::new(batch).unwrap(),
    }
}

fn threads(n: usize) -> Parallelism {
    Parallelism::Threads(NonZeroUsize::new(n).unwrap())
}

fn indexed(store: &ElzevirStore, parallelism: Parallelism, batch: usize) -> TantivyIndex {
    let index = TantivyIndex::in_memory(parallelism).unwrap();
    let report = index_pending(store, store, &index, &options(parallelism, batch)).unwrap();
    assert_eq!(
        (report.effects, report.indexed, report.removed),
        (200, 200, 0)
    );
    index
}

fn titles(store: &ElzevirStore, index: &TantivyIndex, query: &str) -> Vec<String> {
    index
        .search(query, 10)
        .unwrap()
        .into_iter()
        .map(|hit| store.instance(hit.instance).unwrap().unwrap().title)
        .collect()
}

#[test]
fn serial_and_parallel_indexing_store_the_same_documents() {
    let (_dir, store) = imported();
    let serial = indexed(&store, Parallelism::Serial, 1024)
        .stored_documents()
        .unwrap();
    assert_eq!(serial.len(), 200);
    for (parallelism, batch) in [(threads(4), 7), (threads(8), 64), (Parallelism::Serial, 3)] {
        assert_eq!(
            indexed(&store, parallelism, batch)
                .stored_documents()
                .unwrap(),
            serial
        );
    }
}

#[test]
fn indexing_again_finds_nothing_new() {
    let (_dir, store) = imported();
    let index = indexed(&store, threads(4), 50);
    let last: i64 = store
        .readers()
        .read(|c| c.query_row("SELECT max(id) FROM effect_outbox", [], |row| row.get(0)))
        .unwrap();
    assert_eq!(index.cursor().unwrap(), last);
    let again = index_pending(&store, &store, &index, &options(threads(4), 50)).unwrap();
    assert_eq!((again.effects, again.indexed), (0, 0));
}

#[test]
fn words_match_without_accents_or_case() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let found = titles(&store, &index, "anuario ESTATISTICO");
    assert_eq!(
        found.first().map(String::as_str),
        Some("Perfil - Anuário Estatístico")
    );
    // Subjects and contributors are searched too.
    assert!(titles(&store, &index, "iguacu").contains(&"Perfil - Anuário Estatístico".to_owned()));
    assert!(
        titles(&store, &index, "prefeitura municipal foz")
            .contains(&"Perfil - Anuário Estatístico".to_owned())
    );
    assert!(titles(&store, &index, "zzzzqqq").is_empty());
}

#[test]
fn codes_match_exactly_and_come_first() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let first = |query: &str| -> InstanceId { index.search(query, 5).unwrap()[0].instance };
    let perfil = first("anuario estatistico");
    // Barcode, call number (any case) and classification of the first record.
    assert_eq!(first("1511"), perfil);
    assert_eq!(first("310.62 p94"), perfil);
    assert_eq!(first("310.62"), perfil);
    // ISBN with or without hyphens.
    let by_isbn = first("978-85-60342-01-3");
    assert_eq!(first("9788560342013"), by_isbn);
    let view = store.instance(by_isbn).unwrap().unwrap();
    assert!(
        view.identifiers
            .iter()
            .any(|(_, value)| value == "9788560342013")
    );
}

#[test]
fn a_classification_prefix_finds_the_class() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let hits = index.search("310*", 50).unwrap();
    assert!(!hits.is_empty());
    for hit in hits {
        let classification = store
            .instance(hit.instance)
            .unwrap()
            .unwrap()
            .classification;
        assert!(classification.unwrap().starts_with("310"));
    }
}

#[test]
fn the_index_and_its_cursor_survive_a_restart() {
    let (dir, store) = imported();
    let path = dir.path().join("index");
    std::fs::create_dir(&path).unwrap();
    let before = {
        let index = TantivyIndex::open(&path, threads(2)).unwrap();
        index_pending(&store, &store, &index, &options(threads(2), 64)).unwrap();
        (index.cursor().unwrap(), index.stored_documents().unwrap())
    };
    let index = TantivyIndex::open(&path, threads(2)).unwrap();
    assert_eq!(
        (index.cursor().unwrap(), index.stored_documents().unwrap()),
        before
    );
    let again = index_pending(&store, &store, &index, &options(threads(2), 64)).unwrap();
    assert_eq!(again.effects, 0);
}
