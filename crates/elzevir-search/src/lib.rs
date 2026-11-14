// SPDX-License-Identifier: GPL-3.0-or-later
//! Full-text search over the catalog with Tantivy (docs/specs/catalogo/busca.md).
//!
//! [`TantivyIndex`] implements the [`SearchIndex`] port. It owns the only index writer
//! (one owner, like the SQLite writer) and one shared [`IndexReader`], reloaded after each
//! commit and never opened per request (docs/arquitetura/03-estrategia-reconstrucao.md,
//! §2.3). Each commit stores the outbox cursor it reaches as the commit payload, so the
//! documents and the cursor can never disagree after a crash.

mod schema;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{
    IndexBatch, SearchDocument, SearchError, SearchHit, SearchIndex, SearchResults,
    normalized_identifier,
};
use elzevir_domain::catalog::{HubId, IdentifierKind, InstanceId, MaterialType, WorkId};
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, RegexQuery, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{
    Document, Index, IndexReader, IndexSettings, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};

use crate::schema::Fields;

/// Index writer memory per indexing thread. Tantivy needs at least 15 MB per thread.
const MEMORY_PER_THREAD: usize = 32 * 1024 * 1024;

/// Tantivy refuses more indexing threads than this.
const MAX_THREADS: usize = 8;

/// The catalog's search index.
pub struct TantivyIndex {
    index: Index,
    fields: Fields,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
}

impl TantivyIndex {
    /// Opens the index in `directory`, creating it if empty. `parallelism` sets the
    /// writer's indexing threads (`ELZEVIR_SERIAL=index` gives one).
    ///
    /// # Errors
    ///
    /// When the directory cannot be opened or emptied, or is already locked by another
    /// writer.
    pub fn open(directory: &Path, parallelism: Parallelism) -> Result<Self, SearchError> {
        let (schema, fields) = schema::build();
        let opened = MmapDirectory::open(directory).map_err(error)?;
        if Index::exists(&opened).map_err(error)?
            && Index::open(opened.clone()).map_err(error)?.schema() != schema
        {
            // The index is derived data: an index built with another schema is dropped,
            // and with no cursor the indexer replays the whole outbox into the new one.
            drop(opened);
            std::fs::remove_dir_all(directory).map_err(error)?;
            std::fs::create_dir_all(directory).map_err(error)?;
            let fresh = MmapDirectory::open(directory).map_err(error)?;
            return Self::start(
                Index::create(fresh, schema, IndexSettings::default()).map_err(error)?,
                fields,
                parallelism,
            );
        }
        let index = Index::open_or_create(opened, schema).map_err(error)?;
        Self::start(index, fields, parallelism)
    }

    /// A new index in memory, for tests and tools.
    ///
    /// # Errors
    ///
    /// When the writer or the reader cannot start.
    pub fn in_memory(parallelism: Parallelism) -> Result<Self, SearchError> {
        let (schema, fields) = schema::build();
        Self::start(Index::create_in_ram(schema), fields, parallelism)
    }

    fn start(index: Index, fields: Fields, parallelism: Parallelism) -> Result<Self, SearchError> {
        schema::register_tokenizers(&index);
        let threads = match parallelism {
            Parallelism::Serial => 1,
            Parallelism::Threads(threads) => threads.get().min(MAX_THREADS),
        };
        let writer = index
            .writer_with_num_threads(threads, threads.saturating_mul(MEMORY_PER_THREAD))
            .map_err(error)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(error)?;
        Ok(Self {
            index,
            fields,
            writer: Mutex::new(writer),
            reader,
        })
    }

    /// Every stored document as JSON, sorted, to compare two indexes built from the same
    /// effects (docs/arquitetura/03-estrategia-reconstrucao.md, §1.2 item 8).
    ///
    /// # Errors
    ///
    /// When the index cannot be read.
    pub fn stored_documents(&self) -> Result<Vec<String>, SearchError> {
        let searcher = self.reader.searcher();
        let schema = self.index.schema();
        let mut documents = Vec::new();
        for segment in searcher.segment_readers() {
            let store = segment.get_store_reader(1).map_err(error)?;
            for doc in segment.doc_ids_alive() {
                let document: TantivyDocument = store.get(doc).map_err(error)?;
                documents.push(document.to_json(&schema));
            }
        }
        documents.sort();
        Ok(documents)
    }

    fn document(&self, source: &SearchDocument) -> TantivyDocument {
        let f = &self.fields;
        let mut document = TantivyDocument::default();
        document.add_u64(f.instance, id(source.instance.0));
        document.add_u64(f.work, id(source.work.0));
        if let Some(hub) = source.hub {
            document.add_u64(f.hub, id(hub.0));
        }
        document.add_text(f.title, &source.title);
        document.add_text(f.work_title, &source.work_title);
        if let Some(main) = &source.main_contributor {
            document.add_text(f.main_contributor, main);
        }
        let texts = [
            (f.contributor, &source.contributors),
            (f.subject, &source.subjects),
            (f.note, &source.notes),
            (f.identifier, &source.identifiers),
            (f.barcode, &source.barcodes),
            (f.inventory_number, &source.inventory_numbers),
            (f.location, &source.locations),
            (f.language, &source.languages),
        ];
        for (field, values) in texts {
            for value in values {
                document.add_text(field, value);
            }
        }
        for call_number in &source.call_numbers {
            document.add_text(f.call_number, call_number);
            document.add_text(f.call_number_key, call_number.to_lowercase());
        }
        let optional = [
            (f.publisher, &source.publisher),
            (f.summary, &source.summary),
            (f.date, &source.date),
            (f.classification, &source.classification),
        ];
        for (field, value) in optional {
            if let Some(value) = value {
                document.add_text(field, value);
            }
        }
        if let Some(material_type) = source.material_type {
            document.add_u64(f.material_type, u64::from(material_type.code()));
        }
        document
    }

    /// The query for what a person typed: words over the descriptive fields, plus exact
    /// matches on codes (ISBN, barcode, tombo, call number) and a classification prefix
    /// (`82*`).
    fn query(&self, text: &str) -> Box<dyn Query> {
        let f = &self.fields;
        let mut parser = QueryParser::for_index(
            &self.index,
            vec![
                f.title,
                f.work_title,
                f.contributor,
                f.subject,
                f.publisher,
                f.summary,
                f.note,
            ],
        );
        parser.set_conjunction_by_default();
        parser.set_field_boost(f.title, 2.0);
        parser.set_field_boost(f.work_title, 2.0);
        // User input is never an error: whatever the grammar cannot read is dropped.
        let (words, _ignored) = parser.parse_query_lenient(text);
        let typed = text.trim();
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Should, words)];
        let exact = [
            (
                f.identifier,
                normalized_identifier(IdentifierKind::Isbn, typed),
            ),
            (f.identifier, typed.to_owned()),
            (f.barcode, typed.to_owned()),
            (f.inventory_number, typed.to_owned()),
            (f.call_number_key, typed.to_lowercase()),
            (f.classification, typed.to_owned()),
        ];
        for (field, value) in exact {
            if !value.is_empty() {
                let term = TermQuery::new(
                    Term::from_field_text(field, &value),
                    IndexRecordOption::Basic,
                );
                // An exact code beats any word match.
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(term), 10.0)),
                ));
            }
        }
        if let Some(prefix) = classification_prefix(typed)
            && let Ok(regex) = RegexQuery::from_pattern(&format!("{prefix}.*"), f.classification)
        {
            clauses.push((Occur::Should, Box::new(regex)));
        }
        Box::new(BooleanQuery::new(clauses))
    }

    fn hit(&self, score: f32, document: &TantivyDocument) -> Option<SearchHit> {
        let number = |field| {
            document
                .get_first(field)
                .and_then(|value| value.as_u64())
                .and_then(|n| i64::try_from(n).ok())
        };
        let text = |field| {
            document
                .get_first(field)
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        };
        Some(SearchHit {
            instance: InstanceId(number(self.fields.instance)?),
            work: WorkId(number(self.fields.work)?),
            hub: number(self.fields.hub).map(HubId),
            score,
            title: text(self.fields.title).unwrap_or_default(),
            main_contributor: text(self.fields.main_contributor),
            date: text(self.fields.date),
            material_type: number(self.fields.material_type)
                .and_then(|code| u8::try_from(code).ok())
                .and_then(MaterialType::from_code),
            call_number: text(self.fields.call_number),
        })
    }
}

impl SearchIndex for TantivyIndex {
    fn cursor(&self) -> Result<i64, SearchError> {
        let metas = self.index.load_metas().map_err(error)?;
        metas.payload.map_or(Ok(0), |payload| {
            payload
                .parse()
                .map_err(|_| SearchError(format!("index cursor `{payload}` is not a number")))
        })
    }

    fn apply(&self, batch: IndexBatch) -> Result<(), SearchError> {
        // A poisoned lock means an earlier apply panicked; Tantivy discards uncommitted
        // work on the next commit or rollback, so the writer is still usable.
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let replaced = batch
            .upserts
            .iter()
            .map(|document| document.instance)
            .chain(batch.removals.iter().copied());
        for instance in replaced {
            writer.delete_term(Term::from_field_u64(self.fields.instance, id(instance.0)));
        }
        for document in &batch.upserts {
            writer
                .add_document(self.document(document))
                .map_err(error)?;
        }
        let mut commit = writer.prepare_commit().map_err(error)?;
        commit.set_payload(&batch.cursor.to_string());
        commit.commit().map_err(error)?;
        drop(writer);
        self.reader.reload().map_err(error)
    }

    fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, SearchError> {
        let searcher = self.reader.searcher();
        let query = self.query(query);
        if limit == 0 {
            let total = searcher.search(&*query, &Count).map_err(error)?;
            return Ok(SearchResults {
                total,
                hits: Vec::new(),
            });
        }
        let (found, total) = searcher
            .search(
                &*query,
                &(
                    TopDocs::with_limit(limit)
                        .and_offset(offset)
                        .order_by_score(),
                    Count,
                ),
            )
            .map_err(error)?;
        let mut hits = Vec::with_capacity(found.len());
        for (score, address) in found {
            let document: TantivyDocument = searcher.doc(address).map_err(error)?;
            if let Some(hit) = self.hit(score, &document) {
                hits.push(hit);
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.instance.0.cmp(&b.instance.0))
        });
        Ok(SearchResults { total, hits })
    }
}

/// `82*` or `869.3*` (digits and dots before a final `*`) asks for a classification prefix.
fn classification_prefix(typed: &str) -> Option<String> {
    let prefix = typed.strip_suffix('*')?;
    (!prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .then(|| prefix.replace('.', "\\."))
}

/// Ids are positive SQLite rowids; a negative one would be a bug upstream, stored as 0.
fn id(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn error(e: impl std::fmt::Display) -> SearchError {
    SearchError(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_prefixes_are_digits_and_dots() {
        assert_eq!(classification_prefix("82*").as_deref(), Some("82"));
        assert_eq!(classification_prefix("869.3*").as_deref(), Some("869\\.3"));
        assert_eq!(classification_prefix("82"), None);
        assert_eq!(classification_prefix("*"), None);
        assert_eq!(classification_prefix("dom*"), None);
    }

    #[test]
    fn an_index_with_another_schema_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = tantivy::schema::Schema::builder();
        old.add_text_field("title", tantivy::schema::STORED);
        let directory = MmapDirectory::open(dir.path()).unwrap();
        drop(Index::create(directory, old.build(), IndexSettings::default()).unwrap());
        let index = TantivyIndex::open(dir.path(), Parallelism::Serial).unwrap();
        assert_eq!(index.cursor().unwrap(), 0);
        assert_eq!(index.index.schema(), schema::build().0);
    }
}
