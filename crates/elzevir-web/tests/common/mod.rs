// SPDX-License-Identifier: GPL-3.0-or-later
//! Fakes shared by the HTTP tests.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use elzevir_app::catalog::{CatalogReadError, CatalogReader, InstanceView, ItemStatus, ItemView};
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
