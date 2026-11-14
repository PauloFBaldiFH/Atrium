// SPDX-License-Identifier: GPL-3.0-or-later
//! The staff catalog listing: browse and search on one screen
//! (docs/specs/catalogo/listagem.md).

use std::sync::Arc;

use askama::Template;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use elzevir_app::catalog::ShelfSignal;
use elzevir_app::listing::{
    self, CatalogListing, FamilyCursor, FamilyHead, FamilyRow, InstanceRow, ItemPill, Match,
    WorkRow,
};
use elzevir_web_templates::{
    FamilyLine, HubLine, InstanceLine, PillLine, StaffCatalog, WorkLine, url_encode,
};
use serde::Deserialize;

use crate::opac::material_key;
use crate::staff::current_session;
use crate::{AppState, render};

/// What the staff listing reads.
#[derive(Clone)]
pub struct StaffPorts {
    /// Families of the catalog.
    pub listing: Arc<dyn CatalogListing>,
}

/// Query string of `/equipe/acervo`.
#[derive(Deserialize)]
pub(crate) struct CatalogParams {
    #[serde(default)]
    q: String,
    /// Browsing: the family the page starts after.
    after: Option<String>,
    /// Searching: the hit the page starts from.
    from: Option<String>,
}

/// One page, as read.
enum Page {
    Browse(listing::ListingPage),
    Search(listing::SearchPage),
}

/// `GET /equipe/acervo?q=&after=&from=`: the listing, or a search when `q` is given. An
/// HTMX request for the next page gets only the rows.
pub(crate) async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CatalogParams>,
) -> Response {
    if current_session(&state, &headers).is_none() {
        return Redirect::to("/equipe/login").into_response();
    }
    let listing = Arc::clone(&state.inner.staff_ports.listing);
    let index = Arc::clone(&state.inner.opac.index);
    let read = tokio::task::spawn_blocking(move || {
        if params.q.trim().is_empty() {
            // A malformed cursor is read as the first page.
            let after = params.after.as_deref().and_then(FamilyCursor::from_token);
            listing::browse(&*listing, after.as_ref())
                .map(Page::Browse)
                .map_err(drop)
        } else {
            let from = params
                .from
                .and_then(|from| from.parse::<usize>().ok())
                .unwrap_or(0);
            listing::search(&*index, &*listing, &params.q, from)
                .map(Page::Search)
                .map_err(drop)
        }
    })
    .await;
    let Ok(Ok(page)) = read else {
        return (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response();
    };
    let page = match &page {
        Page::Browse(page) => StaffCatalog {
            t: &state.inner.i18n,
            query: "",
            total: None,
            rows: page.rows.iter().map(family_line).collect(),
            next_url: page
                .next
                .as_ref()
                .map(|next| format!("/equipe/acervo?after={}", url_encode(&next.token()))),
        },
        Page::Search(page) => StaffCatalog {
            t: &state.inner.i18n,
            query: &page.query,
            total: Some(page.total),
            rows: page.rows.iter().map(family_line).collect(),
            next_url: page
                .next
                .map(|from| format!("/equipe/acervo?q={}&from={from}", url_encode(&page.query))),
        },
    };
    // The search form swaps the whole list and picks it from a full page; the next-page
    // loader asks for the rows alone.
    let whole_list = headers
        .get("hx-target")
        .is_some_and(|target| target == "families");
    if headers.contains_key("hx-request") && !whole_list {
        match page.as_rows().render() {
            Ok(html) => Html(html).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
        }
    } else {
        render(&page)
    }
}

fn family_line(row: &FamilyRow) -> FamilyLine {
    let hub = match (row.cursor.head, &row.hub_title) {
        (FamilyHead::Hub(id), Some(title)) => Some(HubLine {
            id: id.0,
            title: title.clone(),
        }),
        _ => None,
    };
    FamilyLine {
        hub,
        works: row.works.iter().map(work_line).collect(),
    }
}

fn work_line(work: &WorkRow) -> WorkLine {
    WorkLine {
        id: work.id.0,
        title: work.title.clone(),
        contributor: work.main_contributor.clone(),
        classification: work.classification.clone(),
        languages: work.languages.clone(),
        subjects: work.subjects.clone(),
        more_subjects: work.more_subjects,
        instances: work.instances.iter().map(instance_line).collect(),
    }
}

fn instance_line(instance: &InstanceRow) -> InstanceLine {
    let imprint = match (&instance.publisher, &instance.date) {
        (Some(publisher), Some(date)) => Some(format!("{publisher}, {date}")),
        (Some(only), None) | (None, Some(only)) => Some(only.clone()),
        (None, None) => None,
    };
    InstanceLine {
        id: instance.id.0,
        title: instance.title.clone(),
        edition: instance.edition.clone(),
        imprint,
        isbn: instance.isbn.clone(),
        material: instance.material_type.map(material_key),
        found: instance.matched == Match::Found,
        items: instance.items,
        pills: instance.pills.iter().map(pill_line).collect(),
        more_items: instance.more_items,
    }
}

fn pill_line(pill: &ItemPill) -> PillLine {
    let parts = [
        Some(pill.home_branch.as_str()),
        pill.location.as_deref(),
        pill.call_number.as_deref(),
        pill.inventory_number.as_deref(),
    ];
    let (signal_class, signal_key) = signal(pill.signal);
    PillLine {
        text: parts.into_iter().flatten().collect::<Vec<_>>().join(" | "),
        signal_class,
        signal_key,
    }
}

/// The CSS class and message key of a signal.
pub(crate) const fn signal(signal: ShelfSignal) -> (&'static str, &'static str) {
    match signal {
        ShelfSignal::Available => ("signal-available", "shelf-signal-available"),
        ShelfSignal::NotForLoan => ("signal-not-for-loan", "shelf-signal-not-for-loan"),
        ShelfSignal::OutOfCirculation => (
            "signal-out-of-circulation",
            "shelf-signal-out-of-circulation",
        ),
    }
}
