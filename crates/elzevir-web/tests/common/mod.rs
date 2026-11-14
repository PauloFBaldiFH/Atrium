// SPDX-License-Identifier: GPL-3.0-or-later
//! Fakes shared by the HTTP tests.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use elzevir_app::catalog::{CatalogReadError, CatalogReader, InstanceView, ItemStatus, ItemView};
use elzevir_app::listing::{CatalogListing, Family, FamilyCursor, FamilyHead};
use elzevir_app::search::{IndexBatch, SearchError, SearchHit, SearchIndex, SearchResults};
use elzevir_domain::catalog::InstanceId;

/// One record: its version, the Instance and its Items.
pub(crate) type Record = (i64, InstanceView, Vec<ItemView>);

/// A catalog held in memory, counting how often records are read.
#[derive(Default)]
pub(crate) struct Catalog {
    /// Records by Instance, with their version and Items.
    pub(crate) records: Mutex<BTreeMap<i64, Record>>,
    /// Calls to [`CatalogReader::instance`].
    pub(crate) reads: AtomicUsize,
}

impl CatalogReader for Catalog {
    fn instance(&self, id: InstanceId) -> Result<Option<InstanceView>, CatalogReadError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .records
            .lock()
            .unwrap()
            .get(&id.0)
            .map(|(_, view, _)| view.clone()))
    }

    fn items(&self, instance: InstanceId) -> Result<Vec<ItemView>, CatalogReadError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .get(&instance.0)
            .map(|(_, _, items)| items.clone())
            .unwrap_or_default())
    }

    fn record_version(&self, id: InstanceId) -> Result<Option<i64>, CatalogReadError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .get(&id.0)
            .map(|(version, _, _)| *version))
    }

    fn item_statuses(
        &self,
        instances: &[InstanceId],
    ) -> Result<Vec<(InstanceId, ItemStatus)>, CatalogReadError> {
        Ok(instances
            .iter()
            .flat_map(|id| {
                self.items(*id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |item| (*id, item.status))
            })
            .collect())
    }
}

/// An index that answers every query with the same hits.
#[derive(Default)]
pub(crate) struct Index(pub(crate) Vec<SearchHit>);

impl SearchIndex for Index {
    fn cursor(&self) -> Result<i64, SearchError> {
        Ok(0)
    }

    fn apply(&self, _: IndexBatch) -> Result<(), SearchError> {
        Ok(())
    }

    fn search(&self, _: &str, offset: usize, limit: usize) -> Result<SearchResults, SearchError> {
        Ok(SearchResults {
            total: self.0.len(),
            hits: self.0.iter().skip(offset).take(limit).cloned().collect(),
        })
    }
}

/// Families held in memory, in listing order.
#[derive(Default)]
pub(crate) struct Families(pub(crate) Vec<Family>);

impl CatalogListing for Families {
    fn families_after(
        &self,
        after: Option<&FamilyCursor>,
        limit: NonZeroUsize,
    ) -> Result<Vec<Family>, CatalogReadError> {
        Ok(self
            .0
            .iter()
            .filter(|family| after.is_none_or(|after| &family.cursor > after))
            .take(limit.get())
            .cloned()
            .collect())
    }

    fn families(&self, heads: &[FamilyHead]) -> Result<Vec<Family>, CatalogReadError> {
        Ok(heads
            .iter()
            .filter_map(|head| self.0.iter().find(|family| family.cursor.head == *head))
            .cloned()
            .collect())
    }
}

/// Stored records held in memory, in Instance order.
pub(crate) struct Records(pub(crate) Vec<elzevir_app::export::StoredRecord>);

impl elzevir_app::export::RecordSource for Records {
    fn records_after(
        &self,
        after: Option<InstanceId>,
        limit: NonZeroUsize,
    ) -> Result<Vec<elzevir_app::export::StoredRecord>, CatalogReadError> {
        Ok(self
            .0
            .iter()
            .filter(|stored| after.is_none_or(|after| stored.instance > after))
            .take(limit.get())
            .cloned()
            .collect())
    }
}

/// Records edits from the screen. An edit read at version 1 is accepted and moves to 2;
/// any other version is stale, as if someone else had edited first.
#[derive(Default)]
pub(crate) struct Editor(
    pub(crate) Mutex<Vec<(elzevir_app::staff::StaffId, elzevir_app::editing::Edit)>>,
);

impl elzevir_app::editing::CatalogEditor for Editor {
    fn apply(
        &self,
        actor: elzevir_app::staff::StaffId,
        edit: elzevir_app::editing::Edit,
    ) -> Result<elzevir_app::catalog::CatalogOutput, elzevir_app::editing::EditFailed> {
        use elzevir_app::catalog::{CatalogOutput, CatalogRefusal};
        use elzevir_app::editing::{Edit, EditFailed};
        let read = match &edit {
            Edit::Work(update) => update.row_version,
            Edit::Instance(update) => update.row_version,
        };
        if read != 1 {
            return Err(EditFailed::Refused(CatalogRefusal::Stale {
                read,
                current: 2,
            }));
        }
        let output = match &edit {
            Edit::Work(update) => CatalogOutput::WorkUpdated {
                work: update.work,
                row_version: 2,
            },
            Edit::Instance(update) => CatalogOutput::InstanceUpdated {
                instance: update.instance,
                row_version: 2,
            },
        };
        self.0.lock().unwrap().push((actor, edit));
        Ok(output)
    }
}

/// Suggests from a fixed list of agents and subjects.
pub(crate) struct Suggestions;

impl elzevir_app::suggestions::AuthoritySuggestions for Suggestions {
    fn agents(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<elzevir_app::suggestions::AgentSuggestion>, CatalogReadError> {
        Ok(["Assis, Machado de, 1839-1908", "Assunção, Maria"]
            .into_iter()
            .filter(|heading| heading.to_lowercase().starts_with(&prefix.to_lowercase()))
            .take(limit)
            .map(|heading| elzevir_app::suggestions::AgentSuggestion {
                heading: heading.to_owned(),
                kind: elzevir_domain::catalog::AgentKind::Person,
            })
            .collect())
    }

    fn subjects(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<elzevir_app::suggestions::SubjectSuggestion>, CatalogReadError> {
        Ok(["Romance brasileiro"]
            .into_iter()
            .filter(|heading| heading.to_lowercase().starts_with(&prefix.to_lowercase()))
            .take(limit)
            .map(|heading| elzevir_app::suggestions::SubjectSuggestion {
                heading: heading.to_owned(),
                kind: elzevir_domain::catalog::SubjectKind::Topic,
                thesaurus: Some("larpcal".to_owned()),
            })
            .collect())
    }
}
