// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP-level tests of the staff catalog listing (docs/specs/catalogo/listagem.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use elzevir_app::catalog::{ItemStatus, ItemView};
use elzevir_app::listing::{Family, FamilyCursor, FamilyHead, FamilyInstance, FamilyWork};
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::search::SearchHit;
use elzevir_app::staff::{
    Permission, StaffAccount, StaffDirectory, StaffDirectoryError, StaffId, hash_password,
};
use elzevir_domain::catalog::{InstanceId, ItemId, MaterialType, WorkId};
use elzevir_web::{AppState, OpacPorts, StaffPorts, WebConfig, router};
use elzevir_web_templates::I18n;
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Accounts(Vec<StaffAccount>);

impl StaffDirectory for Accounts {
    fn by_login(&self, login: &str) -> Result<Option<StaffAccount>, StaffDirectoryError> {
        Ok(self.0.iter().find(|a| a.login == login).cloned())
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

fn family(n: i64, title: &str, lost: bool) -> Family {
    let status = ItemStatus {
        lost: i64::from(lost),
        ..ItemStatus::default()
    };
    Family {
        cursor: FamilyCursor {
            sort_key: format!("{n:03}"),
            head: FamilyHead::Work(WorkId(n)),
        },
        hub_title: None,
        works: vec![FamilyWork {
            id: WorkId(n),
            title: title.to_owned(),
            main_contributor: Some("Assis, Machado de".to_owned()),
            classification: Some("869.3".to_owned()),
            languages: vec!["por".to_owned()],
            subjects: vec!["Ficção".to_owned()],
            instances: vec![FamilyInstance {
                id: InstanceId(n * 10),
                version: 1,
                title: title.to_owned(),
                edition: None,
                publisher: Some("Garnier".to_owned()),
                date: Some("1899".to_owned()),
                isbn: Some("9788535910667".to_owned()),
                material_type: Some(MaterialType::Book),
                items: vec![ItemView {
                    id: ItemId(n),
                    barcode: None,
                    inventory_number: Some(format!("T{n}")),
                    home_branch: "MAIN".to_owned(),
                    item_type: "BK".to_owned(),
                    call_number: Some("869.3 A848d".to_owned()),
                    location: None,
                    status,
                }],
            }],
        }],
    }
}

fn hit(n: i64) -> SearchHit {
    SearchHit {
        instance: InstanceId(n * 10),
        work: WorkId(n),
        hub: None,
        score: 1.0,
        title: String::new(),
        main_contributor: None,
        date: None,
        material_type: None,
        call_number: None,
    }
}

fn app() -> Router {
    let accounts = Accounts(vec![StaffAccount {
        id: StaffId(1),
        login: "ana".to_owned(),
        password_hash: hash_password("s3cret").unwrap(),
        permissions: [Permission::StaffInterface].into(),
    }]);
    let families: Vec<Family> = (1..=35)
        .map(|n| match n {
            1 => family(1, "Dom Casmurro", true),
            _ => family(n, &format!("Livro {n:03}"), false),
        })
        .collect();
    router(AppState::new(
        Arc::new(accounts),
        Arc::new(NoCounters),
        OpacPorts {
            catalog: Arc::new(common::Catalog::default()),
            index: Arc::new(common::Index(vec![hit(2)])),
        },
        StaffPorts {
            listing: Arc::new(common::Families(families)),
        },
        I18n::english().unwrap(),
        WebConfig {
            secure_cookies: false,
            ..WebConfig::default()
        },
    ))
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

async fn body(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn get(uri: &str, cookie: Option<&str>, htmx_target: Option<&str>) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(target) = htmx_target {
        builder = builder.header("hx-request", "true");
        if !target.is_empty() {
            builder = builder.header("hx-target", target);
        }
    }
    builder.body(Body::empty()).unwrap()
}

fn set_cookie(response: &Response<Body>, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{name}=")))
        .and_then(|v| v.split(';').next())
        .unwrap()
        .to_owned()
}

async fn signed_in(app: &Router) -> String {
    let form = send(app, get("/equipe/login", None, None)).await;
    let login_cookie = set_cookie(&form, "elzevir_login_csrf");
    let html = body(form).await;
    let csrf = html
        .split(r#"name="csrf" value=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    let request = Request::post("/equipe/login")
        .header(header::COOKIE, login_cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("login=ana&password=s3cret&csrf={csrf}")))
        .unwrap();
    let ok = send(app, request).await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    set_cookie(&ok, "elzevir_session")
}

#[tokio::test]
async fn the_listing_requires_a_session() {
    let response = send(&app(), get("/equipe/acervo", None, None)).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn the_first_page_shows_families_with_pills_and_a_loader_for_the_next() {
    let app = app();
    let session = signed_in(&app).await;
    let response = send(&app, get("/equipe/acervo", Some(&session), None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body(response).await;
    assert_eq!(html.matches(r#"<li class="family no-hub">"#).count(), 30);
    assert!(html.contains("Dom Casmurro"));
    assert!(html.contains("Assis, Machado de"));
    assert!(html.contains("Garnier, 1899"));
    assert!(html.contains(
        r#"<span class="visually-hidden">Out of circulation:</span> <span class="item-pill-text">MAIN | 869.3 A848d | T1</span></li>"#
    ));
    assert!(html.contains(r#"hx-get="/equipe/acervo?after=w30.030""#));
    // Blocks name their parent, for the cords drawn by the page's script.
    assert!(html.contains(r#"<script type="module" src="/static/elzevir-catalog.js"></script>"#));
    assert!(html.contains(r#"data-node="i10""#));
    assert!(html.contains(r#"data-parent="w1" data-cord-label="edition""#));
    assert!(html.contains(r#"data-parent="i10" data-cord-label="items""#));
    // The main contributor is an authority block, parent of its Work with a dashed cord.
    assert!(html.contains(r#"<article class="card card-auth" data-node="a0">"#));
    assert!(html.contains(r#"data-parent="a0""#));
    assert!(html.contains(r#"data-cord-label="written by""#));
    let script = send(&app, get("/static/elzevir-catalog.js", None, None)).await;
    assert_eq!(script.status(), StatusCode::OK);
    assert!(body(script).await.contains("ResizeObserver"));

    // The loader gets the rows alone: the last five families, and no further loader.
    let next = send(
        &app,
        get("/equipe/acervo?after=w30.030", Some(&session), Some("")),
    )
    .await;
    let rows = body(next).await;
    assert!(!rows.contains("<html"));
    assert_eq!(rows.matches(r#"<li class="family no-hub">"#).count(), 5);
    assert!(!rows.contains("hx-trigger"));
}

#[tokio::test]
async fn a_search_shows_the_families_found_on_the_same_screen() {
    let app = app();
    let session = signed_in(&app).await;
    // The search form swaps the whole list, so it gets the full page to pick it from.
    let response = send(
        &app,
        get("/equipe/acervo?q=livro", Some(&session), Some("families")),
    )
    .await;
    let html = body(response).await;
    assert!(html.contains("<html"));
    assert!(html.contains("One record found"));
    assert_eq!(html.matches(r#"<li class="family no-hub">"#).count(), 1);
    assert!(html.contains(r#"<article class="card card-instance found""#));
    assert!(html.contains("Livro 002"));
}

#[tokio::test]
async fn a_malformed_cursor_starts_from_the_first_page() {
    let app = app();
    let session = signed_in(&app).await;
    let response = send(
        &app,
        get("/equipe/acervo?after=garbage", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body(response).await.contains("Dom Casmurro"));
}
