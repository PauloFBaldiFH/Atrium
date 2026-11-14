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
