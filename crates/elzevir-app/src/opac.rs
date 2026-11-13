// SPDX-License-Identifier: GPL-3.0-or-later
//! The public catalog: a page of search results and the page of one record
//! (docs/specs/opac/busca-e-registro.md).
//!
//! What the OPAC reads goes through [`SearchIndex`] and [`CatalogReader`]; neither reaches
//! patrons or loans (vision B.5, LGPD).

use std::collections::BTreeMap;

use elzevir_domain::catalog::InstanceId;

use crate::catalog::{CatalogReadError, CatalogReader, InstanceView, ItemView};
use crate::search::{SearchError, SearchHit, SearchIndex};

/// Results per page.
pub const PAGE_SIZE: usize = 20;

/// Longest query read, in characters; the rest is ignored (an explicit input limit).
pub const MAX_QUERY_CHARS: usize = 256;

/// Last page that can be asked for, so a crafted URL cannot make the index skip millions
/// of documents.
pub const MAX_PAGE: usize = 500;

/// One result with its availability.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    /// What the index stores about the Instance.
    pub hit: SearchHit,
    /// Items of the Instance.
    pub items: u32,
    /// Items available ([`crate::catalog::ItemStatus::is_available`]).
    pub available: u32,
}

/// One page of search results.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultsPage {
    /// The query as used (trimmed and cut to [`MAX_QUERY_CHARS`]).
    pub query: String,
    /// Matching Instances in all.
    pub total: usize,
    /// This page, from 1.
    pub page: usize,
    /// Whether a next page exists.
    pub has_next: bool,
    /// The results of this page, best first.
    pub rows: Vec<ResultRow>,
}

/// One record: the Instance with its Work, and its Items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPage {
    /// The Instance and what it shows of its Work.
    pub instance: InstanceView,
    /// Its Items, in creation order.
    pub items: Vec<ItemView>,
}

/// The OPAC could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpacError {
    /// The index failed.
    #[error(transparent)]
    Search(#[from] SearchError),
    /// The catalog failed.
    #[error(transparent)]
    Catalog(#[from] CatalogReadError),
}

/// A page of results for `query`, with availability read for the whole page at once.
///
/// An empty query gives an empty page; a page past [`MAX_PAGE`] is read as the last one.
///
/// # Errors
///
/// When the index or the catalog fails.
pub fn search<I, C>(
    index: &I,
    catalog: &C,
    query: &str,
    page: usize,
) -> Result<ResultsPage, OpacError>
where
    I: SearchIndex + ?Sized,
    C: CatalogReader + ?Sized,
{
    let query: String = query.trim().chars().take(MAX_QUERY_CHARS).collect();
    let page = page.clamp(1, MAX_PAGE);
    if query.is_empty() {
        return Ok(ResultsPage {
            query,
            total: 0,
            page,
            has_next: false,
            rows: Vec::new(),
        });
    }
    let offset = (page - 1).saturating_mul(PAGE_SIZE);
    let results = index.search(&query, offset, PAGE_SIZE)?;
    let instances: Vec<InstanceId> = results.hits.iter().map(|hit| hit.instance).collect();
    let mut counts: BTreeMap<InstanceId, (u32, u32)> = BTreeMap::new();
    for (instance, status) in catalog.item_statuses(&instances)? {
        let (items, available) = counts.entry(instance).or_default();
        *items = items.saturating_add(1);
        if status.is_available() {
            *available = available.saturating_add(1);
        }
    }
    let rows = results
        .hits
        .into_iter()
        .map(|hit| {
            let (items, available) = counts.get(&hit.instance).copied().unwrap_or_default();
            ResultRow {
                hit,
                items,
                available,
            }
        })
        .collect();
    Ok(ResultsPage {
        query,
        total: results.total,
        page,
        has_next: offset.saturating_add(PAGE_SIZE) < results.total && page < MAX_PAGE,
        rows,
    })
}

/// The record of one Instance, or `None` if it does not exist.
///
/// # Errors
///
/// When the catalog fails.
pub fn record<C: CatalogReader + ?Sized>(
    catalog: &C,
    id: InstanceId,
) -> Result<Option<RecordPage>, OpacError> {
    let Some(instance) = catalog.instance(id)? else {
        return Ok(None);
    };
    let items = catalog.items(id)?;
    Ok(Some(RecordPage { instance, items }))
}
