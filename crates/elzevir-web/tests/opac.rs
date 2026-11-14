// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP-level tests of the OPAC search and record pages over fakes
//! (docs/specs/opac/busca-e-registro.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use elzevir_app::catalog::{InstanceView, ItemStatus, ItemView};
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::search::SearchHit;
use elzevir_app::staff::{StaffAccount, StaffDirectory, StaffDirectoryError};
use elzevir_domain::catalog::{InstanceId, ItemId, MaterialType, NoteKind, WorkId};
use elzevir_web::{AppState, OpacPorts, WebConfig, router};
use elzevir_web_templates::I18n;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::{Catalog, Index};

struct NoStaff;

impl StaffDirectory for NoStaff {
    fn by_login(&self, _: &str) -> Result<Option<StaffAccount>, StaffDirectoryError> {
        Ok(None)
    }
}

struct NoCounters;

impl MemoryProbe for NoCounters {
    fn report(&self) -> MemoryReport {
        MemoryReport {
            allocator: "system",
            counters: None,
        }
    }
}

fn view(id: i64, title: &str) -> InstanceView {
    InstanceView {
        id: InstanceId(id),
        version: 1,
        work: WorkId(id),
        work_version: 1,
        work_title: title.to_owned(),
        hub: None,
        work_material_type: Some(MaterialType::Book),
        material_type: Some(MaterialType::Book),
        relations: Vec::new(),
        title: title.to_owned(),
        publisher: Some("Editora".to_owned()),
        date: Some("1995".to_owned()),
        classification: Some("310.62".to_owned()),
        summary: None,
        audience: Some("Ensino médio.".to_owned()),
        contributors: Vec::new(),
        subjects: Vec::new(),
        identifiers: Vec::new(),
        notes: vec![
            (NoteKind::General, "Inclui índice.".to_owned()),
            (NoteKind::Contents, "Parte 1 -- Parte 2.".to_owned()),
        ],
        languages: vec!["por".to_owned()],
    }
}

fn item(id: i64, status: ItemStatus) -> ItemView {
    ItemView {
        id: ItemId(id),
        barcode: Some(format!("B{id}")),
        inventory_number: None,
        home_branch: "CPL".to_owned(),
        item_type: "BK".to_owned(),
        call_number: Some("310.62 P94".to_owned()),
        location: None,
        status,
    }
}

fn hit(id: i64, title: &str) -> SearchHit {
    SearchHit {
        instance: InstanceId(id),
        work: WorkId(id),
        hub: None,
        score: 1.0,
        title: title.to_owned(),
        main_contributor: Some("Prefeitura".to_owned()),
        date: Some("1995".to_owned()),
        material_type: Some(MaterialType::Book),
        call_number: Some("310.62 P94".to_owned()),
    }
}

const LOST: ItemStatus = ItemStatus {
    not_for_loan: 0,
    damaged: 0,
    lost: 1,
    withdrawn: 0,
    restricted: 0,
};

fn app(catalog: Arc<Catalog>, hits: Vec<SearchHit>) -> Router {
    router(AppState::new(
        Arc::new(NoStaff),
        Arc::new(NoCounters),
        OpacPorts {
            catalog,
            index: Arc::new(Index(hits)),
        },
        I18n::english().unwrap(),
        WebConfig::default(),
    ))
}

fn catalog() -> Catalog {
    let catalog = Catalog::default();
    let mut records = catalog.records.lock().unwrap();
    records.insert(
        1,
        (
            1,
            view(1, "Perfil <Anuário>"),
            vec![item(1, ItemStatus::default()), item(2, LOST)],
        ),
    );
    records.insert(2, (1, view(2, "Sem exemplares"), Vec::new()));
    drop(records);
    catalog
}

async fn get(app: &Router, uri: &str, htmx: bool) -> (StatusCode, String) {
    let mut request = Request::get(uri);
    if htmx {
        request = request.header("HX-Request", "true");
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn results_show_availability_and_escape_titles() {
    let app = app(
        Arc::new(catalog()),
        vec![hit(1, "Perfil <Anuário>"), hit(2, "Sem exemplares")],
    );
    let (status, html) = get(&app, "/search?q=perfil", false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<html"));
    assert!(html.contains("2 results"));
    assert!(html.contains(r#"href="/record/1""#));
    assert!(html.contains("Perfil &#60;Anuário&#62;") || html.contains("Perfil &lt;Anuário&gt;"));
    assert!(html.contains("1 of 2 available"));
    assert!(html.contains("No copies"));
    assert!(html.contains("Book"));
}

#[tokio::test]
async fn an_htmx_request_gets_only_the_result_list() {
    let app = app(Arc::new(catalog()), vec![hit(1, "Perfil")]);
    let (status, html) = get(&app, "/search?q=perfil", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("<html"));
    assert!(html.trim_start().starts_with(r#"<section id="results""#));
    assert!(html.contains("1 result"));
}

#[tokio::test]
async fn pages_link_with_the_query_encoded() {
    let hits = (1..=45).map(|id| hit(id, "Livro")).collect();
    let app = app(Arc::new(Catalog::default()), hits);
    let (_, first) = get(&app, "/search?q=caf%C3%A9+%26+p%C3%A3o", false).await;
    assert!(first.contains("45 results"));
    assert!(first.contains("/search?q=caf%C3%A9%20%26%20p%C3%A3o&amp;page=2"));
    assert!(!first.contains("rel=\"prev\""));
    let (_, last) = get(&app, "/search?q=livro&page=3", false).await;
    assert!(last.contains("rel=\"prev\""));
    assert!(!last.contains("rel=\"next\""));
    // A page that is not a number is the first one.
    let (status, _) = get(&app, "/search?q=livro&page=abc", false).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn an_empty_query_shows_no_list() {
    let app = app(Arc::new(catalog()), vec![hit(1, "Perfil")]);
    let (status, html) = get(&app, "/search?q=+++", false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains(r#"class="results""#));
    assert!(!html.contains("Nothing found"));
}

#[tokio::test]
async fn a_record_shows_its_items_and_is_rendered_once_per_version() {
    let catalog = Arc::new(catalog());
    let app = app(Arc::clone(&catalog), Vec::new());
    let (status, html) = get(&app, "/record/1", false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("<title>Perfil &#60;Anuário&#62;")
            || html.contains("<title>Perfil &lt;Anuário&gt;")
    );
    assert!(html.contains("Editora, 1995"));
    assert!(html.contains("Available"));
    assert!(html.contains("Lost"));
    assert!(html.contains("<dt>Contents</dt>\n    <dd>Parte 1 -- Parte 2.</dd>"));
    assert!(html.contains("<dt>Audience</dt><dd>Ensino médio.</dd>"));
    assert!(html.contains("<dt>Notes</dt>\n    <dd>Inclui índice.</dd>"));
    assert!(!html.contains("<Anuário>"));
    assert_eq!(catalog.reads.load(Ordering::SeqCst), 1);
    let (_, again) = get(&app, "/record/1", false).await;
    assert_eq!(again, html);
    assert_eq!(catalog.reads.load(Ordering::SeqCst), 1);
    let (_, empty) = get(&app, "/record/2", false).await;
    assert!(empty.contains("This record has no copies."));
}

#[tokio::test]
async fn a_new_version_is_rendered_again() {
    let catalog = Arc::new(catalog());
    let app = app(Arc::clone(&catalog), Vec::new());
    let (_, before) = get(&app, "/record/1", false).await;
    {
        let mut records = catalog.records.lock().unwrap();
        let record = records.get_mut(&1).unwrap();
        record.0 = 2;
        record.2.push(item(3, ItemStatus::default()));
    }
    let (_, after) = get(&app, "/record/1", false).await;
    assert_eq!(catalog.reads.load(Ordering::SeqCst), 2);
    assert_ne!(before, after);
    assert_eq!(after.matches(">Available<").count(), 2);
}

#[tokio::test]
async fn unknown_records_are_404() {
    let app = app(Arc::new(catalog()), Vec::new());
    for uri in ["/record/99", "/record/abc", "/record/-1"] {
        let (status, html) = get(&app, uri, false).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(html.contains("Record not found"));
    }
}
