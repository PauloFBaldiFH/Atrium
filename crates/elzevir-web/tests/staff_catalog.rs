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
use elzevir_app::catalog::{
    InstanceView, ItemData, ItemStatus, ItemView, NewContribution, NewIdentifier, NewInstance,
    NewNote, NewRecord,
};
use elzevir_app::export::StoredRecord;
use elzevir_app::listing::{Family, FamilyCursor, FamilyHead, FamilyInstance, FamilyWork};
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::search::SearchHit;
use elzevir_app::staff::{
    Permission, StaffAccount, StaffDirectory, StaffDirectoryError, StaffId, hash_password,
};
use elzevir_domain::catalog::{
    AgentKind, IdentifierKind, InstanceId, ItemId, MaterialType, NoteKind, WorkId,
};
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

/// A record nobody imported: it is written from the model.
fn stored(instance: i64) -> StoredRecord {
    StoredRecord {
        instance: InstanceId(instance),
        record: NewRecord {
            work_title: "Dom Casmurro".to_owned(),
            material_type: MaterialType::Book,
            hub: None,
            relations: Vec::new(),
            classification: Some("869.3".to_owned()),
            classification_scheme: Some("ddc".to_owned()),
            summary: Some("Bentinho recorda Capitu.".to_owned()),
            audience: None,
            languages: vec!["por".to_owned()],
            contributions: vec![NewContribution {
                kind: AgentKind::Person,
                heading: "Assis, Machado de, 1839-1908".to_owned(),
                role: Some("author".to_owned()),
                primary: true,
            }],
            subjects: Vec::new(),
            instance: NewInstance {
                title: "Dom Casmurro".to_owned(),
                extent: Some("256 p.".to_owned()),
                identifiers: vec![NewIdentifier {
                    kind: IdentifierKind::Isbn,
                    value: "9788535910667".to_owned(),
                    qualifier: None,
                }],
                notes: vec![NewNote {
                    kind: NoteKind::Local,
                    text: "Doação da família Silva.".to_owned(),
                }],
                ..NewInstance::default()
            },
            items: vec![ItemData {
                barcode: Some("000123".to_owned()),
                inventory_number: Some("T-77".to_owned()),
                home_branch: "MAIN".to_owned(),
                holding_branch: "MAIN".to_owned(),
                item_type: "BK".to_owned(),
                call_number: Some("869.3 A848d".to_owned()),
                location: None,
                collection: None,
                price: None,
                replacement_price: None,
                copy_number: None,
                status: ItemStatus::default(),
                acquired_on: None,
            }],
            source: None,
        },
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

/// The reader the form asks for versions: Instance 10 of Work 1, both at version 1.
fn catalog_with_record() -> common::Catalog {
    let catalog = common::Catalog::default();
    let view = InstanceView {
        id: InstanceId(10),
        version: 1,
        work: WorkId(1),
        work_version: 1,
        work_title: "Dom Casmurro".to_owned(),
        hub: None,
        work_material_type: Some(MaterialType::Book),
        material_type: Some(MaterialType::Book),
        relations: Vec::new(),
        title: "Dom Casmurro".to_owned(),
        publisher: None,
        date: None,
        classification: None,
        summary: None,
        audience: None,
        contributors: Vec::new(),
        subjects: Vec::new(),
        identifiers: Vec::new(),
        notes: Vec::new(),
        languages: Vec::new(),
    };
    catalog
        .records
        .lock()
        .unwrap()
        .insert(10, (1, view, Vec::new()));
    catalog
}

fn app() -> Router {
    let accounts = Accounts(vec![
        StaffAccount {
            id: StaffId(1),
            login: "ana".to_owned(),
            password_hash: hash_password("s3cret").unwrap(),
            permissions: [Permission::StaffInterface, Permission::Cataloguing].into(),
        },
        StaffAccount {
            id: StaffId(2),
            login: "bia".to_owned(),
            password_hash: hash_password("s3cret").unwrap(),
            permissions: [Permission::StaffInterface].into(),
        },
    ]);
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
            catalog: Arc::new(catalog_with_record()),
            index: Arc::new(common::Index(vec![hit(2)])),
        },
        StaffPorts {
            listing: Arc::new(common::Families(families)),
            records: Arc::new(common::Records(vec![stored(10)])),
            editor: Arc::new(common::Editor::default()),
            suggestions: Arc::new(common::Suggestions),
            items: Arc::new(common::ItemGrid),
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
    signed_in_as(app, "ana").await
}

async fn signed_in_as(app: &Router, login: &str) -> String {
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
        .body(Body::from(format!(
            "login={login}&password=s3cret&csrf={csrf}"
        )))
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
    assert_eq!(html.matches(r#"<li class="family" id="f-w"#).count(), 30);
    assert!(html.contains("Dom Casmurro"));
    assert!(html.contains("Assis, Machado de"));
    assert!(html.contains("Garnier, 1899"));
    // Each family is a compact row that opens into its blocks; none is open while browsing.
    assert!(html.contains(r#"<li class="family" id="f-w1" data-head="w1">"#));
    assert!(html.contains("One work, one instance"));
    assert!(html.contains("0 of 1 available"));
    assert!(html.contains("1 of 1 available"));
    assert!(html.contains(r#"<span class="chip chip-agent">Assis, Machado de</span>"#));
    assert!(!html.contains("<details class=\"family-focus\" open>"));
    assert!(html.contains(
        r#"<use href="/static/elzevir-icons.svg#book"></use></svg><span class="item-signal">Out of circulation</span> <span class="item-pill-text">Main Library | 869.3 A848d | T1</span></li>"#
    ));
    assert!(html.contains(r#"hx-get="/equipe/acervo?after=w30.030""#));
    // Blocks name their parent, for the cords drawn by the page's script.
    assert!(html.contains(r#"<script type="module" src="/static/elzevir-catalog.js"></script>"#));
    assert!(html.contains(r#"data-level="instance" tabindex="0" data-node="i10""#));
    assert!(html.contains(r#"data-parent="w1" data-cord-label="edition""#));
    assert!(html.contains(r#"data-parent="i10" data-cord-label="items""#));
    // The main contributor is an authority block, parent of its Work with a dashed cord.
    assert!(html.contains(
        r#"<article class="card card-auth" data-level="auth" tabindex="0" data-node="a0">"#
    ));
    assert!(html.contains(r#"data-parent="a0""#));
    assert!(html.contains(r#"data-cord-label="written by""#));
    // Toggles for the Hub and Authorities columns stay hidden until the script runs.
    assert!(html.contains(r#"data-toggle="auth" aria-pressed="true" hidden>Authorities</button>"#));
    assert!(html.contains(r#"data-toggle="hub" aria-pressed="true" hidden>Hub</button>"#));
    let icons = send(&app, get("/static/elzevir-icons.svg", None, None)).await;
    assert_eq!(icons.status(), StatusCode::OK);
    assert!(body(icons).await.contains(r#"<symbol id="periodical""#));
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
    assert_eq!(rows.matches(r#"<li class="family" id="f-w"#).count(), 5);
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
    assert_eq!(html.matches(r#"<li class="family" id="f-w"#).count(), 1);
    // The family with the Instance found opens by itself.
    assert!(html.contains(r#"<details class="family-focus" open>"#));
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

#[tokio::test]
async fn one_record_is_exported_as_marc21_or_marcxml() {
    let app = app();
    let refused = send(&app, get("/equipe/acervo/instancia/10/marc", None, None)).await;
    assert_eq!(refused.status(), StatusCode::SEE_OTHER);

    let session = signed_in(&app).await;
    let page = body(send(&app, get("/equipe/acervo", Some(&session), None)).await).await;
    assert!(page.contains(r#"href="/equipe/acervo/instancia/10/marc?format=marcxml""#));

    let marc21 = send(
        &app,
        get("/equipe/acervo/instancia/10/marc", Some(&session), None),
    )
    .await;
    assert_eq!(marc21.status(), StatusCode::OK);
    assert_eq!(marc21.headers()[header::CONTENT_TYPE], "application/marc");
    assert_eq!(
        marc21.headers()[header::CONTENT_DISPOSITION],
        r#"attachment; filename="elzevir-10.mrc""#
    );
    let bytes = body(marc21).await;
    assert!(bytes.contains("Dom Casmurro"));
    assert!(
        bytes.ends_with('\u{1d}'),
        "an ISO 2709 record ends with its terminator"
    );

    let xml = send(
        &app,
        get(
            "/equipe/acervo/instancia/10/marc?format=marcxml",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(
        xml.headers()[header::CONTENT_DISPOSITION],
        r#"attachment; filename="elzevir-10.xml""#
    );
    let xml = body(xml).await;
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("Dom Casmurro"));

    let missing = send(
        &app,
        get("/equipe/acervo/instancia/20/marc", Some(&session), None),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let unknown = send(
        &app,
        get(
            "/equipe/acervo/instancia/10/marc?format=pdf",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_focused_block_loads_every_filled_field() {
    let app = app();
    let refused = send(&app, get("/equipe/acervo/instancia/10/campos", None, None)).await;
    assert_eq!(refused.status(), StatusCode::SEE_OTHER);

    // Bia reads the catalog but does not catalogue: she gets the fields to read.
    let session = signed_in_as(&app, "bia").await;
    let page = body(send(&app, get("/equipe/acervo", Some(&session), None)).await).await;
    assert!(page.contains(r#"data-fields="/equipe/acervo/instancia/10/campos?level=work""#));
    assert!(page.contains(r#"data-fields="/equipe/acervo/instancia/10/campos?level=instance""#));

    let work = body(
        send(
            &app,
            get(
                "/equipe/acervo/instancia/10/campos?level=work",
                Some(&session),
                None,
            ),
        )
        .await,
    )
    .await;
    assert!(work.contains("Assis, Machado de, 1839-1908"));
    assert!(work.contains("person · author · main entry"));
    assert!(work.contains("Bentinho recorda Capitu."));
    assert!(work.contains("869.3"));
    // Only what is filled: the record has no audience.
    assert!(!work.contains("Intended audience"));
    assert!(!work.contains("<table>"), "items belong to the Instance");

    let instance = body(
        send(
            &app,
            get(
                "/equipe/acervo/instancia/10/campos?level=instance",
                Some(&session),
                None,
            ),
        )
        .await,
    )
    .await;
    assert!(instance.contains("256 p."));
    assert!(instance.contains("9788535910667"));
    assert!(instance.contains("Local note"));
    assert!(instance.contains("Doação da família Silva."));
    assert!(instance.contains("000123"));
    assert!(instance.contains("T-77"));
    assert!(instance.contains("869.3 A848d"));
    // Names in place of codes (exemplares.md, item 29).
    assert!(instance.contains("<td>Main Library</td>"));
    assert!(instance.contains("<td>Book</td>"));
    assert!(!instance.contains("Edition"));

    let missing = send(
        &app,
        get("/equipe/acervo/instancia/20/campos", Some(&session), None),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let unknown = send(
        &app,
        get(
            "/equipe/acervo/instancia/10/campos?level=hub",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
}

/// The CSRF token the catalog page hands to its script.
async fn page_token(app: &Router, session: &str) -> String {
    let page = body(send(app, get("/equipe/acervo", Some(session), None)).await).await;
    page.split(r#"data-csrf=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned()
}

fn post_json(uri: &str, session: Option<&str>, token: Option<&str>, json: &str) -> Request<Body> {
    let mut builder = Request::post(uri).header(header::CONTENT_TYPE, "application/json");
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, session);
    }
    if let Some(token) = token {
        builder = builder.header("x-elzevir-csrf", token);
    }
    builder.body(Body::from(json.to_owned())).unwrap()
}

#[tokio::test]
async fn an_edit_needs_a_session_its_token_and_the_cataloguing_permission() {
    let app = app();
    let edit = r#"{"version": 1, "changes": [{"Summary": "Novo."}]}"#;
    let anonymous = send(&app, post_json("/equipe/acervo/obra/1", None, None, edit)).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let ana = signed_in(&app).await;
    let token = page_token(&app, &ana).await;
    let no_token = send(
        &app,
        post_json("/equipe/acervo/obra/1", Some(&ana), None, edit),
    )
    .await;
    assert_eq!(no_token.status(), StatusCode::FORBIDDEN);
    let wrong = send(
        &app,
        post_json("/equipe/acervo/obra/1", Some(&ana), Some("x"), edit),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);

    let bia = signed_in_as(&app, "bia").await;
    let bia_token = page_token(&app, &bia).await;
    let reader = send(
        &app,
        post_json("/equipe/acervo/obra/1", Some(&bia), Some(&bia_token), edit),
    )
    .await;
    assert_eq!(reader.status(), StatusCode::FORBIDDEN);

    let accepted = send(
        &app,
        post_json("/equipe/acervo/obra/1", Some(&ana), Some(&token), edit),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(body(accepted).await, r#"{"version":2}"#);
}

#[tokio::test]
async fn an_invalid_or_stale_edit_is_refused_field_by_field() {
    let app = app();
    let ana = signed_in(&app).await;
    let token = page_token(&app, &ana).await;
    let invalid = send(
        &app,
        post_json(
            "/equipe/acervo/instancia/10",
            Some(&ana),
            Some(&token),
            r#"{"version": 1, "changes": [{"Title": "  "}, {"Identifiers": [{"kind": "isbn", "value": "9788535910668", "qualifier": null}]}]}"#,
        ),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let invalid = body(invalid).await;
    assert!(invalid.contains(r#""field":"title","key":"edit-error-title-empty""#));
    assert!(invalid.contains("This ISBN is not valid"));

    let stale = send(
        &app,
        post_json(
            "/equipe/acervo/instancia/10",
            Some(&ana),
            Some(&token),
            r#"{"version": 5, "changes": [{"Edition": "2. ed."}]}"#,
        ),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(body(stale).await, r#"{"current":2}"#);
}

#[tokio::test]
async fn a_cataloguer_gets_the_focused_block_as_a_form_in_profile_order() {
    let app = app();
    let ana = signed_in(&app).await;
    let work = body(
        send(
            &app,
            get(
                "/equipe/acervo/instancia/10/campos?level=work",
                Some(&ana),
                None,
            ),
        )
        .await,
    )
    .await;
    assert!(work.contains(r#"data-endpoint="/equipe/acervo/obra/1" data-version="1""#));
    assert!(work.contains(r#"data-change="Contributions""#));
    assert!(work.contains(r#"value="Assis, Machado de, 1839-1908""#));
    // Filled fields first, then the empty ones of the profile.
    let (filled, empty) = work.split_once("form-empty").unwrap();
    assert!(filled.contains(r#"data-change="Summary""#));
    assert!(empty.contains(r#"data-change="Audience""#));
    assert!(empty.contains(r#"data-change="Subjects""#));

    let instance = body(
        send(
            &app,
            get(
                "/equipe/acervo/instancia/10/campos?level=instance",
                Some(&ana),
                None,
            ),
        )
        .await,
    )
    .await;
    assert!(instance.contains(r#"data-endpoint="/equipe/acervo/instancia/10" data-version="1""#));
    assert!(instance.contains(r#"value="9788535910667""#));
    // A book's new identifier starts as an ISBN.
    let identifiers = instance
        .split(r#"data-change="Identifiers""#)
        .nth(1)
        .unwrap();
    let blank = identifiers.split("<template>").nth(1).unwrap();
    let blank = blank.split("</template>").next().unwrap();
    assert!(blank.contains(r#"<option value="isbn" selected>"#));
}

#[tokio::test]
async fn suggestions_list_existing_agents_and_subjects_by_prefix() {
    let app = app();
    let anonymous = send(
        &app,
        get("/equipe/acervo/sugestoes/agentes?q=as", None, None),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let session = signed_in(&app).await;
    let short = body(
        send(
            &app,
            get("/equipe/acervo/sugestoes/agentes?q=a", Some(&session), None),
        )
        .await,
    )
    .await;
    assert_eq!(short, "[]");
    let agents = body(
        send(
            &app,
            get(
                "/equipe/acervo/sugestoes/agentes?q=%20ass",
                Some(&session),
                None,
            ),
        )
        .await,
    )
    .await;
    assert_eq!(
        agents,
        r#"[{"heading":"Assis, Machado de, 1839-1908","kind":"person"},{"heading":"Assunção, Maria","kind":"person"}]"#
    );
    let subjects = body(
        send(
            &app,
            get(
                "/equipe/acervo/sugestoes/assuntos?q=rom",
                Some(&session),
                None,
            ),
        )
        .await,
    )
    .await;
    assert_eq!(
        subjects,
        r#"[{"heading":"Romance brasileiro","kind":"topic","thesaurus":"larpcal"}]"#
    );
}

#[tokio::test]
async fn a_cataloguer_creates_a_family_or_an_instance_from_the_screen() {
    let app = app();
    let family =
        r#"{"work": [{"Title": "Quincas Borba"}, {"MaterialType": "book"}], "instance": []}"#;
    let anonymous = send(&app, post_json("/equipe/acervo/nova", None, None, family)).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let bia = signed_in_as(&app, "bia").await;
    let bia_token = page_token(&app, &bia).await;
    let reader = send(
        &app,
        post_json("/equipe/acervo/nova", Some(&bia), Some(&bia_token), family),
    )
    .await;
    assert_eq!(reader.status(), StatusCode::FORBIDDEN);

    let ana = signed_in(&app).await;
    let token = page_token(&app, &ana).await;
    let created = send(
        &app,
        post_json("/equipe/acervo/nova", Some(&ana), Some(&token), family),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    // The page that starts at the new family: same sort key, the Work id before.
    assert_eq!(
        body(created).await,
        r#"{"instance":901,"family":"/equipe/acervo?after=w899.quincas%20borba#f-w900"}"#
    );

    let empty = send(
        &app,
        post_json("/equipe/acervo/nova", Some(&ana), Some(&token), "{}"),
    )
    .await;
    assert_eq!(empty.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let empty = body(empty).await;
    assert!(empty.contains(r#""field":"title","key":"edit-error-title-empty""#));
    assert!(empty.contains(r#""field":"material_type","key":"edit-error-material-required""#));

    let instance = r#"{"changes": [{"Title": "Quincas Borba"}, {"Edition": "2. ed."}]}"#;
    let added = send(
        &app,
        post_json(
            "/equipe/acervo/obra/1/instancias",
            Some(&ana),
            Some(&token),
            instance,
        ),
    )
    .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    assert_eq!(body(added).await, r#"{"instance":902}"#);
    let unknown = send(
        &app,
        post_json(
            "/equipe/acervo/obra/404/instancias",
            Some(&ana),
            Some(&token),
            instance,
        ),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_new_record_forms_start_blank_or_carry_over_the_previous_record() {
    let app = app();
    let bia = signed_in_as(&app, "bia").await;
    let listing = body(send(&app, get("/equipe/acervo", Some(&bia), None)).await).await;
    assert!(!listing.contains(r#"data-action="new-family""#));
    let refused = send(&app, get("/equipe/acervo/nova", Some(&bia), None)).await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    let ana = signed_in(&app).await;
    let listing = body(send(&app, get("/equipe/acervo", Some(&ana), None)).await).await;
    assert!(listing.contains(r#"data-action="new-family""#));
    assert!(listing.contains(r#"data-action="new-instance" data-from="10""#));

    let blank = body(
        send(
            &app,
            get("/equipe/acervo/nova?material=periodical", Some(&ana), None),
        )
        .await,
    )
    .await;
    assert!(blank.contains(r#"data-endpoint="/equipe/acervo/nova" data-create="family""#));
    assert!(blank.contains(r#"data-part="work""#) && blank.contains(r#"data-part="instance""#));
    assert!(blank.contains(r#"<option value="periodical" selected>"#));
    assert!(!blank.contains("data-inherited"));

    let next = body(send(&app, get("/equipe/acervo/nova?from=10", Some(&ana), None)).await).await;
    // Language and category carry over, marked; title and contributors do not.
    assert!(next.contains(r#"data-change="Languages" data-error="languages" data-inherited"#));
    assert!(
        next.contains(r#"data-change="MaterialType" data-error="material_type" data-inherited"#)
    );
    assert!(next.contains("from the previous record"));
    assert!(!next.contains("Dom Casmurro") && !next.contains("Assis, Machado de"));

    let instance = body(
        send(
            &app,
            get("/equipe/acervo/instancia/10/nova", Some(&ana), None),
        )
        .await,
    )
    .await;
    assert!(
        instance
            .contains(r#"data-endpoint="/equipe/acervo/obra/1/instancias" data-create="instance""#)
    );
    assert!(instance.contains(r#"value="Dom Casmurro""#));
    assert!(!instance.contains("9788535910667"));
}

#[tokio::test]
async fn a_batch_of_items_is_refused_cell_by_cell_or_saved_whole() {
    let app = app();
    let uri = "/equipe/acervo/instancia/10/exemplares";
    let batch = r#"{"version": 1, "rows": [{"id": 3, "barcode": "0001", "home_branch": "MAIN", "item_type": "BK"}, {"barcode": "0002", "home_branch": "MAIN", "item_type": "BK", "price": "12,50"}]}"#;
    let anonymous = send(&app, post_json(uri, None, None, batch)).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let bia = signed_in_as(&app, "bia").await;
    let bia_token = page_token(&app, &bia).await;
    let reader = send(&app, post_json(uri, Some(&bia), Some(&bia_token), batch)).await;
    assert_eq!(reader.status(), StatusCode::FORBIDDEN);

    let ana = signed_in(&app).await;
    let token = page_token(&app, &ana).await;
    let saved = send(&app, post_json(uri, Some(&ana), Some(&token), batch)).await;
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(body(saved).await, r#"{"version":2,"ids":[3,501]}"#);

    let bad = r#"{"version": 1, "rows": [{"barcode": "0001", "home_branch": "MAIN", "item_type": "BK"}, {"barcode": "0001", "home_branch": "", "item_type": "BK", "price": "doação"}]}"#;
    let refused = send(&app, post_json(uri, Some(&ana), Some(&token), bad)).await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let refused = body(refused).await;
    assert!(refused.contains(r#"{"row":0,"field":"barcode","key":"items-error-barcode-taken""#));
    assert!(refused.contains(r#"{"row":1,"field":"home_branch","key":"items-error-required""#));
    assert!(refused.contains(r#"{"row":1,"field":"price","key":"items-error-price""#));
    assert!(refused.contains("Another item already has this barcode."));

    let stale = send(
        &app,
        post_json(
            uri,
            Some(&ana),
            Some(&token),
            &batch.replace(r#""version": 1"#, r#""version": 4"#),
        ),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_cataloguer_gets_the_item_grid_with_its_rows_and_lists() {
    let app = app();
    let bia = signed_in_as(&app, "bia").await;
    let refused = send(
        &app,
        get("/equipe/acervo/instancia/10/exemplares", Some(&bia), None),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let listing = body(send(&app, get("/equipe/acervo", Some(&bia), None)).await).await;
    assert!(!listing.contains(r#"data-action="edit-items""#));

    let ana = signed_in(&app).await;
    let listing = body(send(&app, get("/equipe/acervo", Some(&ana), None)).await).await;
    assert!(listing.contains(r#"data-action="edit-items" data-from="10""#));
    let grid = body(
        send(
            &app,
            get("/equipe/acervo/instancia/10/exemplares", Some(&ana), None),
        )
        .await,
    )
    .await;
    assert!(
        grid.contains(r#"data-endpoint="/equipe/acervo/instancia/10/exemplares" data-version="1""#)
    );
    assert!(grid.contains(r#"<tr data-id="3">"#));
    for cell in ["000123", "T-77", "869.3 A848d", "45,90"] {
        assert!(
            grid.contains(&format!(r#"<td role="gridcell" tabindex="-1">{cell}</td>"#)),
            "{cell}"
        );
    }
    assert!(grid.contains(r#"<i data-col="home_branch" data-list="branches" data-inherited></i>"#));
    assert!(grid.contains(r#"<i data-col="barcode"></i>"#));
    // A coded cell shows the name and keeps the code (item 29).
    assert!(
        grid.contains(r#"<td role="gridcell" tabindex="-1" data-value="MAIN">Main Library</td>"#)
    );
    assert!(grid.contains(r#"<td role="gridcell" tabindex="-1" data-value="BK">Book</td>"#));
    assert!(grid.contains(r#"<datalist id="grid-list-branches"><option value="Main Library" data-code="MAIN"></option><option value="PAL" data-code="PAL"></option></datalist>"#));
}
