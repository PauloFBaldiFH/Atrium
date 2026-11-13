// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog search: what goes into the index, and the indexer that follows the effect
//! outbox (docs/specs/catalogo/busca.md).
//!
//! The index is built from the model, never from MARC (vision A.1): the indexer reads an
//! Instance through [`CatalogReader`] and turns it into a [`SearchDocument`].

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use elzevir_domain::catalog::{HubId, IdentifierKind, InstanceId, MaterialType, WorkId};

use crate::catalog::{CatalogReadError, CatalogReader, InstanceView, ItemView};
use crate::parallelism::Parallelism;

/// Effect kind asking the index to rebuild one Instance's document after commit.
pub const REINDEX_INSTANCE: &str = "catalog.reindex_instance";

/// One effect from the outbox, as the indexer understands it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Rebuild (or remove, if it no longer exists) this Instance's document.
    ReindexInstance(InstanceId),
    /// An effect for another worker (notices, caches); the indexer skips it.
    Other,
}

/// An effect with its place in the outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoggedEffect {
    /// Its id in the outbox; ids grow with commit order.
    pub id: i64,
    /// What it asks for.
    pub effect: Effect,
}

/// Port: the committed effects, in order.
pub trait EffectLog: Send + Sync {
    /// Up to `limit` effects with an id greater than `cursor`, by id.
    ///
    /// # Errors
    ///
    /// When storage fails or an effect payload cannot be read.
    fn after(&self, cursor: i64, limit: usize) -> Result<Vec<LoggedEffect>, EffectLogError>;
}

/// The outbox could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot read the effect outbox: {0}")]
pub struct EffectLogError(pub String);

/// Everything the index knows about one Instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDocument {
    /// The Instance; one document per Instance.
    pub instance: InstanceId,
    /// Its Work.
    pub work: WorkId,
    /// The Work's Hub, if any.
    pub hub: Option<HubId>,
    /// The Instance's title.
    pub title: String,
    /// The Work's preferred title.
    pub work_title: String,
    /// The first contributor (the main entry when the record has one).
    pub main_contributor: Option<String>,
    /// Every contributor heading, in order.
    pub contributors: Vec<String>,
    /// Every subject heading, in order.
    pub subjects: Vec<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Date of publication, as written.
    pub date: Option<String>,
    /// Category of material: the Instance's, else the Work's.
    pub material_type: Option<MaterialType>,
    /// Classification number of the Work, such as `869.3`.
    pub classification: Option<String>,
    /// Identifiers, normalized for exact match (ISBN and ISSN without hyphens).
    pub identifiers: Vec<String>,
    /// Call numbers of the Items, distinct, in Item order.
    pub call_numbers: Vec<String>,
    /// Shelving locations of the Items, distinct, in Item order.
    pub locations: Vec<String>,
    /// Barcodes of the Items.
    pub barcodes: Vec<String>,
    /// Inventory numbers (tombos) of the Items.
    pub inventory_numbers: Vec<String>,
    /// Language codes of the Work.
    pub languages: Vec<String>,
}

/// Builds the document of one Instance. Pure.
#[must_use]
pub fn document_of(instance: &InstanceView, items: &[ItemView]) -> SearchDocument {
    SearchDocument {
        instance: instance.id,
        work: instance.work,
        hub: instance.hub,
        title: instance.title.clone(),
        work_title: instance.work_title.clone(),
        main_contributor: instance
            .contributors
            .first()
            .map(|(_, heading)| heading.clone()),
        contributors: instance
            .contributors
            .iter()
            .map(|(_, heading)| heading.clone())
            .collect(),
        subjects: instance
            .subjects
            .iter()
            .map(|(_, heading)| heading.clone())
            .collect(),
        publisher: instance.publisher.clone(),
        date: instance.date.clone(),
        material_type: instance.material_type.or(instance.work_material_type),
        classification: instance.classification.clone(),
        identifiers: instance
            .identifiers
            .iter()
            .map(|(kind, value)| normalized_identifier(*kind, value))
            .collect(),
        call_numbers: distinct(items.iter().filter_map(|item| item.call_number.as_deref())),
        locations: distinct(items.iter().filter_map(|item| item.location.as_deref())),
        barcodes: items
            .iter()
            .filter_map(|item| item.barcode.clone())
            .collect(),
        inventory_numbers: distinct(
            items
                .iter()
                .filter_map(|item| item.inventory_number.as_deref()),
        ),
        languages: instance.languages.clone(),
    }
}

/// ISBN and ISSN lose hyphens and spaces and get an upper-case `X`, so `85-7232-040-x` and
/// `857232040X` meet; other identifiers are kept as they are.
#[must_use]
pub fn normalized_identifier(kind: IdentifierKind, value: &str) -> String {
    match kind {
        IdentifierKind::Isbn | IdentifierKind::Issn => value
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .map(|c| c.to_ascii_uppercase())
            .collect(),
        _ => value.to_owned(),
    }
}

fn distinct<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .filter(|value| seen.insert(*value))
        .map(str::to_owned)
        .collect()
}

/// One change to the index, committed together with the outbox position it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexBatch {
    /// Documents to add or replace (by Instance).
    pub upserts: Vec<SearchDocument>,
    /// Instances whose documents go away.
    pub removals: Vec<InstanceId>,
    /// The last outbox id this batch covers; saved with the commit.
    pub cursor: i64,
}

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// The Instance found.
    pub instance: InstanceId,
    /// Its Work.
    pub work: WorkId,
    /// Its Hub, if any.
    pub hub: Option<HubId>,
    /// Relevance; higher is better.
    pub score: f32,
}

/// Port: the full-text index.
pub trait SearchIndex: Send + Sync {
    /// The last outbox id already in the index (0 for a new index).
    ///
    /// # Errors
    ///
    /// When the index cannot be read.
    fn cursor(&self) -> Result<i64, SearchError>;

    /// Applies `batch` and commits it with its cursor, atomically: after a crash the index
    /// holds either the whole batch and its cursor, or neither.
    ///
    /// # Errors
    ///
    /// When the index cannot be written.
    fn apply(&self, batch: IndexBatch) -> Result<(), SearchError>;

    /// Up to `limit` Instances matching `query`, best first; equal scores by Instance id.
    ///
    /// # Errors
    ///
    /// When the index cannot be read.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, SearchError>;
}

/// The index failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("search index: {0}")]
pub struct SearchError(pub String);

/// Settings of one indexing run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOptions {
    /// How documents are built.
    pub parallelism: Parallelism,
    /// Outbox effects read and committed together.
    pub batch: NonZeroUsize,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            parallelism: Parallelism::Serial,
            batch: NonZeroUsize::new(1024).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

/// What an indexing run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexReport {
    /// Outbox effects read.
    pub effects: u64,
    /// Documents written.
    pub indexed: u64,
    /// Documents removed because their Instance no longer exists.
    pub removed: u64,
}

/// Why indexing stopped. What was committed before stays, with its cursor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    /// The outbox could not be read.
    #[error(transparent)]
    Effects(#[from] EffectLogError),
    /// The catalog could not be read.
    #[error(transparent)]
    Catalog(#[from] CatalogReadError),
    /// The index failed.
    #[error(transparent)]
    Index(#[from] SearchError),
}

/// Brings the index up to the end of the outbox.
///
/// Effects are read in outbox order, from the cursor saved in the index. Each batch reads
/// its Instances (in parallel when `options` say so), builds their documents and commits
/// them with the new cursor. Running it twice is harmless: the second run finds nothing.
///
/// # Errors
///
/// [`IndexError`] when the outbox, the catalog or the index fails.
pub fn index_pending<L, C, I>(
    log: &L,
    catalog: &C,
    index: &I,
    options: &IndexOptions,
) -> Result<IndexReport, IndexError>
where
    L: EffectLog + ?Sized,
    C: CatalogReader + ?Sized,
    I: SearchIndex + ?Sized,
{
    let mut report = IndexReport::default();
    loop {
        let cursor = index.cursor()?;
        let effects = log.after(cursor, options.batch.get())?;
        let Some(last) = effects.last().map(|logged| logged.id) else {
            return Ok(report);
        };
        report.effects = report.effects.saturating_add(len(effects.len()));
        let mut seen = BTreeSet::new();
        let instances: Vec<InstanceId> = effects
            .iter()
            .filter_map(|logged| match logged.effect {
                Effect::ReindexInstance(instance) => Some(instance),
                Effect::Other => None,
            })
            .filter(|instance| seen.insert(*instance))
            .collect();
        let built = options
            .parallelism
            .map(instances, |instance| build(catalog, instance));
        let mut batch = IndexBatch {
            upserts: Vec::new(),
            removals: Vec::new(),
            cursor: last,
        };
        for (instance, document) in built {
            match document? {
                Some(document) => batch.upserts.push(document),
                None => batch.removals.push(instance),
            }
        }
        report.indexed = report.indexed.saturating_add(len(batch.upserts.len()));
        report.removed = report.removed.saturating_add(len(batch.removals.len()));
        index.apply(batch)?;
    }
}

type Built = (InstanceId, Result<Option<SearchDocument>, CatalogReadError>);

fn build<C: CatalogReader + ?Sized>(catalog: &C, instance: InstanceId) -> Built {
    let document = catalog.instance(instance).and_then(|view| {
        view.map(|view| Ok(document_of(&view, &catalog.items(instance)?)))
            .transpose()
    });
    (instance, document)
}

fn len(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// An outbox, a catalog holding only `existing`, and an index that records batches.
    struct Fake {
        effects: Vec<LoggedEffect>,
        existing: Vec<InstanceId>,
        batches: Mutex<Vec<IndexBatch>>,
    }

    impl EffectLog for Fake {
        fn after(&self, cursor: i64, limit: usize) -> Result<Vec<LoggedEffect>, EffectLogError> {
            Ok(self
                .effects
                .iter()
                .filter(|logged| logged.id > cursor)
                .take(limit)
                .copied()
                .collect())
        }
    }

    impl CatalogReader for Fake {
        fn instance(&self, id: InstanceId) -> Result<Option<InstanceView>, CatalogReadError> {
            Ok(self.existing.contains(&id).then(|| InstanceView {
                id,
                work: WorkId(id.0),
                work_title: format!("Work {}", id.0),
                hub: None,
                work_material_type: Some(MaterialType::Book),
                material_type: None,
                relations: Vec::new(),
                title: format!("Title {}", id.0),
                publisher: None,
                date: None,
                classification: None,
                contributors: Vec::new(),
                subjects: Vec::new(),
                identifiers: Vec::new(),
                languages: Vec::new(),
            }))
        }

        fn items(&self, _: InstanceId) -> Result<Vec<ItemView>, CatalogReadError> {
            Ok(Vec::new())
        }
    }

    impl SearchIndex for Fake {
        fn cursor(&self) -> Result<i64, SearchError> {
            Ok(self
                .batches
                .lock()
                .map_err(|e| SearchError(e.to_string()))?
                .last()
                .map_or(0, |batch| batch.cursor))
        }

        fn apply(&self, batch: IndexBatch) -> Result<(), SearchError> {
            self.batches
                .lock()
                .map_err(|e| SearchError(e.to_string()))?
                .push(batch);
            Ok(())
        }

        fn search(&self, _: &str, _: usize) -> Result<Vec<SearchHit>, SearchError> {
            Ok(Vec::new())
        }
    }

    fn reindex(id: i64, instance: i64) -> LoggedEffect {
        LoggedEffect {
            id,
            effect: Effect::ReindexInstance(InstanceId(instance)),
        }
    }

    #[test]
    fn batches_follow_the_outbox_once_per_instance() {
        let fake = Fake {
            effects: vec![
                reindex(1, 10),
                LoggedEffect {
                    id: 2,
                    effect: Effect::Other,
                },
                reindex(3, 10),
                reindex(4, 11),
                reindex(7, 12),
            ],
            existing: vec![InstanceId(10), InstanceId(12)],
            batches: Mutex::new(Vec::new()),
        };
        let options = IndexOptions {
            parallelism: Parallelism::Serial,
            batch: NonZeroUsize::new(3).unwrap_or(NonZeroUsize::MIN),
        };
        let report = index_pending(&fake, &fake, &fake, &options).unwrap_or_default();
        assert_eq!(
            report,
            IndexReport {
                effects: 5,
                indexed: 2,
                removed: 1
            }
        );
        let batches = fake.batches.into_inner().unwrap_or_default();
        let summary: Vec<_> = batches
            .iter()
            .map(|batch| {
                (
                    batch.cursor,
                    batch
                        .upserts
                        .iter()
                        .map(|d| d.instance.0)
                        .collect::<Vec<_>>(),
                    batch.removals.iter().map(|i| i.0).collect::<Vec<_>>(),
                )
            })
            .collect();
        // First batch: effects 1-3 (Instance 10 twice, built once); then 4 and 7.
        assert_eq!(summary, [(3, vec![10], vec![]), (7, vec![12], vec![11])]);
    }

    #[test]
    fn isbn_and_issn_are_compared_without_hyphens() {
        assert_eq!(
            normalized_identifier(IdentifierKind::Isbn, "85-7232-040-x"),
            "857232040X"
        );
        assert_eq!(
            normalized_identifier(IdentifierKind::Issn, "0378-5955"),
            "03785955"
        );
        assert_eq!(
            normalized_identifier(IdentifierKind::Lccn, "2001-123"),
            "2001-123"
        );
    }
}
