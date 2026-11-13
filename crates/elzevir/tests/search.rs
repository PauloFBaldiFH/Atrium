// SPDX-License-Identifier: GPL-3.0-or-later
//! Import → outbox → index → search, end to end (docs/specs/catalogo/busca.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use elzevir_app::catalog::CatalogReader;
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, SearchIndex, index_pending};
use elzevir_domain::catalog::InstanceId;
use elzevir_search::TantivyIndex;
use elzevir_store::ElzevirStore;

use common::imported;

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
        .search(query, 0, 10)
        .unwrap()
        .hits
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
    // Portuguese plurals meet their singular.
    assert!(
        titles(&store, &index, "anuarios estatisticos")
            .contains(&"Perfil - Anuário Estatístico".to_owned())
    );
    assert!(titles(&store, &index, "zzzzqqq").is_empty());
}

#[test]
fn results_come_in_pages_with_the_total_and_the_list_fields() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let all = index.search("medici", 0, 50).unwrap();
    assert_eq!((all.total, all.hits.len()), (5, 5));
    let page = index.search("medici", 2, 2).unwrap();
    assert_eq!(page.total, 5);
    assert_eq!(page.hits.len(), 2);
    let first = &index.search("anuario estatistico", 0, 1).unwrap().hits[0];
    assert_eq!(first.title, "Perfil - Anuário Estatístico");
    assert_eq!(
        first.main_contributor.as_deref(),
        Some("PREFEITURA MUNICIPAL DE FOZ DO IGUACU")
    );
    assert_eq!(first.date.as_deref(), Some("1995"));
    assert_eq!(first.call_number.as_deref(), Some("310.62 P94"));
    assert!(first.material_type.is_some());
    assert_eq!(index.search("medici", 0, 0).unwrap().total, 5);
}

#[test]
fn codes_match_exactly_and_come_first() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let first = |query: &str| -> InstanceId { index.search(query, 0, 5).unwrap().hits[0].instance };
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
    let hits = index.search("310*", 0, 50).unwrap().hits;
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

/// Reference queries whose results differ from Koha's, with the biblionumbers only Koha
/// finds and only the Elzevir finds (docs/specs/catalogo/busca.md, §4). Every other query
/// must find exactly what Koha finds among the 200 records.
const KNOWN_DIFFERENCES: &[(&str, &[i64], &[i64])] = &[
    // 505 (contents note): notes are not in the model yet.
    ("karl marx", &[147], &[]),
    // 590 (local note) `reimpressão`: Koha also truncates `rei`.
    ("discurso do rei", &[187], &[]),
    // 520 `gramatical` (Koha cuts it to `gramat`) and 505 (contents note).
    ("gramatica", &[179, 188], &[]),
    // 520 `contextualização histórica`: the Portuguese stemmer joins it to `história`.
    ("historia da arte", &[], &[146]),
    // 500 (general note) `análise`: notes are not in the model yet.
    ("analise do discurso", &[12], &[]),
];

#[test]
fn reference_queries_find_what_koha_finds() {
    let (_dir, store) = imported();
    let index = indexed(&store, Parallelism::Serial, 1024);
    let ids = |list: &str| -> BTreeSet<i64> {
        list.split(',')
            .filter(|id| !id.is_empty())
            .map(|id| id.parse().unwrap())
            .collect()
    };
    let recorded = include_str!("data/koha-opac-search.tsv");
    assert_eq!(recorded.lines().count(), 50);
    for line in recorded.lines() {
        let [query, _koha_total, koha] = line.split('\t').collect::<Vec<_>>()[..] else {
            panic!("bad line {line:?}");
        };
        let koha = ids(koha);
        // Instance ids follow the file, and the file is biblionumbers 1 to 200 in order.
        let ours: BTreeSet<i64> = index
            .search(query, 0, 200)
            .unwrap()
            .hits
            .into_iter()
            .map(|hit| hit.instance.0)
            .collect();
        let only_koha: Vec<i64> = koha.difference(&ours).copied().collect();
        let only_ours: Vec<i64> = ours.difference(&koha).copied().collect();
        let expected = KNOWN_DIFFERENCES
            .iter()
            .find(|(known, _, _)| *known == query)
            .map_or((&[][..], &[][..]), |(_, koha, ours)| (*koha, *ours));
        assert_eq!(
            (only_koha.as_slice(), only_ours.as_slice()),
            expected,
            "query {query:?}: (only Koha, only Elzevir)"
        );
    }
}
