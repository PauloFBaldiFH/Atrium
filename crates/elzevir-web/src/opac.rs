// SPDX-License-Identifier: GPL-3.0-or-later
//! The public catalog pages: search and record (docs/specs/opac/busca-e-registro.md).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use elzevir_app::catalog::{CatalogReader, ItemStatus, ItemView};
use elzevir_app::opac::{self, RecordPage, ResultsPage};
use elzevir_app::search::SearchIndex;
use elzevir_domain::catalog::{InstanceId, MaterialType, NoteKind};
use elzevir_web_templates::{
    ItemLine, OpacNotFound, OpacRecord, OpacRecordBody, OpacResults, ResultLine,
};
use serde::Deserialize;

use crate::{AppState, render};

/// Record bodies kept at most; past this the cache starts over. A body is a few kilobytes,
/// so the cache stays in the tens of megabytes.
const RECORD_CACHE_ENTRIES: usize = 10_000;

/// What the OPAC reads: the catalog and the search index. Neither reaches patrons or loans.
#[derive(Clone)]
pub struct OpacPorts {
    /// The catalog.
    pub catalog: Arc<dyn CatalogReader>,
    /// The full-text index.
    pub index: Arc<dyn SearchIndex>,
}

/// A rendered record: its title, for `<title>`, and its body.
pub(crate) struct RenderedRecord {
    title: String,
    body: String,
}

/// Rendered records, by Instance, with the record version they were rendered for. A body
/// is valid until the version changes, so a hit costs one small query instead of reading
/// and rendering the record again.
#[derive(Default)]
pub(crate) struct RecordCache {
    bodies: Mutex<HashMap<i64, (i64, Arc<RenderedRecord>)>>,
}

impl RecordCache {
    fn get(&self, instance: InstanceId, version: i64) -> Option<Arc<RenderedRecord>> {
        let bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        bodies
            .get(&instance.0)
            .filter(|(cached, _)| *cached == version)
            .map(|(_, body)| Arc::clone(body))
    }

    fn put(&self, instance: InstanceId, version: i64, body: Arc<RenderedRecord>) {
        // A poisoned lock only means another request panicked while holding it; the map
        // itself is still whole.
        let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        if bodies.len() >= RECORD_CACHE_ENTRIES && !bodies.contains_key(&instance.0) {
            bodies.clear();
        }
        bodies.insert(instance.0, (version, body));
    }
}

/// Query string of `/search`.
#[derive(Deserialize)]
pub(crate) struct SearchParams {
    #[serde(default)]
    q: String,
    #[serde(default)]
    page: Option<String>,
}

/// `GET /search?q=&page=`: the whole page, or only the result list for an HTMX request.
pub(crate) async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Response {
    // A page that is not a number is read as the first one.
    let page = params
        .page
        .and_then(|page| page.parse::<usize>().ok())
        .unwrap_or(1);
    let ports = state.inner.opac.clone();
    let found = tokio::task::spawn_blocking(move || {
        opac::search(&*ports.index, &*ports.catalog, &params.q, page)
    })
    .await;
    let Ok(Ok(results)) = found else {
        return unavailable();
    };
    let page = results_page(&state, &results);
    if headers.contains_key("hx-request") {
        match page.as_results().render() {
            Ok(html) => Html(html).into_response(),
            Err(_) => unavailable(),
        }
    } else {
        render(&page)
    }
}

fn results_page<'a>(state: &'a AppState, results: &'a ResultsPage) -> OpacResults<'a> {
    let rows = results
        .rows
        .iter()
        .map(|row| ResultLine {
            instance: row.hit.instance.0,
            title: row.hit.title.clone(),
            contributor: row.hit.main_contributor.clone(),
            date: row.hit.date.clone(),
            material: row.hit.material_type.map(material_key),
            call_number: row.hit.call_number.clone(),
            items: row.items,
            available: row.available,
        })
        .collect();
    OpacResults {
        t: &state.inner.i18n,
        query: &results.query,
        total: results.total,
        page: results.page,
        has_next: results.has_next,
        rows,
    }
}

/// `GET /record/{id}`.
pub(crate) async fn record(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Ok(id) = id.parse::<i64>().map(InstanceId) else {
        return not_found(&state);
    };
    let worker_state = state.clone();
    let rendered = tokio::task::spawn_blocking(move || rendered_record(&worker_state, id)).await;
    match rendered {
        Ok(Ok(Some(record))) => render(&OpacRecord {
            t: &state.inner.i18n,
            title: &record.title,
            body: &record.body,
        }),
        Ok(Ok(None)) => not_found(&state),
        _ => unavailable(),
    }
}

/// The record from the cache when its version has not changed, or read and rendered.
/// `Err` when the catalog or the template failed.
fn rendered_record(state: &AppState, id: InstanceId) -> Result<Option<Arc<RenderedRecord>>, ()> {
    let catalog = &*state.inner.opac.catalog;
    let cache = &state.inner.records;
    // The version is read before the record: if the record changes in between, the newer
    // body is kept under the older version and simply rendered again next time.
    let Some(version) = catalog.record_version(id).map_err(drop)? else {
        return Ok(None);
    };
    if let Some(record) = cache.get(id, version) {
        return Ok(Some(record));
    }
    let Some(page) = opac::record(catalog, id).map_err(drop)? else {
        return Ok(None);
    };
    let body = record_body(state, &page).render().map_err(drop)?;
    let record = Arc::new(RenderedRecord {
        title: page.instance.title,
        body,
    });
    cache.put(id, version, Arc::clone(&record));
    Ok(Some(record))
}

fn record_body<'a>(state: &'a AppState, page: &'a RecordPage) -> OpacRecordBody<'a> {
    let instance = &page.instance;
    let publication = match (&instance.publisher, &instance.date) {
        (Some(publisher), Some(date)) => Some(format!("{publisher}, {date}")),
        (Some(only), None) | (None, Some(only)) => Some(only.clone()),
        (None, None) => None,
    };
    OpacRecordBody {
        t: &state.inner.i18n,
        title: &instance.title,
        work_title: (instance.work_title != instance.title).then_some(&*instance.work_title),
        contributors: instance
            .contributors
            .iter()
            .map(|(_, name)| &**name)
            .collect(),
        material: instance
            .material_type
            .or(instance.work_material_type)
            .map(material_key),
        publication,
        classification: instance.classification.as_deref(),
        subjects: instance.subjects.iter().map(|(_, term)| &**term).collect(),
        identifiers: instance
            .identifiers
            .iter()
            .map(|(_, value)| &**value)
            .collect(),
        languages: instance.languages.iter().map(String::as_str).collect(),
        summary: instance.summary.as_deref(),
        notes: notes(&instance.notes, |kind| kind != NoteKind::Contents),
        contents: notes(&instance.notes, |kind| kind == NoteKind::Contents),
        items: page.items.iter().map(item_line).collect(),
    }
}

/// The texts of the notes whose kind passes `keep`, in order.
fn notes(notes: &[(NoteKind, String)], keep: impl Fn(NoteKind) -> bool) -> Vec<&str> {
    notes
        .iter()
        .filter(|(kind, _)| keep(*kind))
        .map(|(_, text)| text.as_str())
        .collect()
}

fn item_line(item: &ItemView) -> ItemLine {
    ItemLine {
        branch: item.home_branch.clone(),
        location: item.location.clone(),
        call_number: item.call_number.clone(),
        available: item.status.is_available(),
        status: status_key(item.status),
    }
}

/// The message key of a category of material.
pub(crate) const fn material_key(material: MaterialType) -> &'static str {
    match material {
        MaterialType::Book => "material-book",
        MaterialType::Periodical => "material-periodical",
        MaterialType::Map => "material-map",
        MaterialType::Score => "material-score",
        MaterialType::SoundRecording => "material-sound-recording",
        MaterialType::Video => "material-video",
        MaterialType::Digital => "material-digital",
        MaterialType::VisualMaterial => "material-visual-material",
        MaterialType::Mixed => "material-mixed",
    }
}

/// The message key of an Item's status: the gravest one set, or available.
pub(crate) const fn status_key(status: ItemStatus) -> &'static str {
    if status.lost != 0 {
        "item-status-lost"
    } else if status.withdrawn != 0 {
        "item-status-withdrawn"
    } else if status.damaged != 0 {
        "item-status-damaged"
    } else if status.restricted != 0 {
        "item-status-restricted"
    } else if status.not_for_loan != 0 {
        "item-status-not-for-loan"
    } else {
        "item-status-available"
    }
}

fn not_found(state: &AppState) -> Response {
    let page = OpacNotFound {
        t: &state.inner.i18n,
    };
    match page.render() {
        Ok(html) => (StatusCode::NOT_FOUND, Html(html)).into_response(),
        Err(_) => unavailable(),
    }
}

/// The catalog or the index failed: answer 503 without details for the visitor.
fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_material_and_status_has_a_message() {
        let t = elzevir_web_templates::I18n::english().unwrap();
        for material in MaterialType::ALL {
            assert!(t.has(material_key(*material)), "{material:?}");
        }
        let statuses = [
            ItemStatus::default(),
            ItemStatus {
                lost: 1,
                ..ItemStatus::default()
            },
            ItemStatus {
                withdrawn: 1,
                ..ItemStatus::default()
            },
            ItemStatus {
                damaged: 1,
                ..ItemStatus::default()
            },
            ItemStatus {
                restricted: 1,
                ..ItemStatus::default()
            },
            ItemStatus {
                not_for_loan: 1,
                ..ItemStatus::default()
            },
        ];
        let keys: std::collections::BTreeSet<_> = statuses.into_iter().map(status_key).collect();
        assert_eq!(keys.len(), statuses.len());
        for key in keys {
            assert!(t.has(key), "{key}");
        }
    }
}
