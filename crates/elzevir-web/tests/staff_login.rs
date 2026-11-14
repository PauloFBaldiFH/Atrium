// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP-level tests of the OPAC home and the staff sign-in flow.
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::staff::{
    Permission, StaffAccount, StaffDirectory, StaffDirectoryError, StaffId, hash_password,
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

fn app() -> Router {
    let accounts = Accounts(vec![
        StaffAccount {
            id: StaffId(1),
            login: "ana".to_owned(),
            password_hash: hash_password("s3cret").unwrap(),
            permissions: [Permission::StaffInterface, Permission::Administration].into(),
        },
        StaffAccount {
            id: StaffId(2),
            login: "leo".to_owned(),
            password_hash: hash_password("s3cret").unwrap(),
            permissions: [Permission::StaffInterface].into(),
        },
    ]);
    let config = WebConfig {
        secure_cookies: false,
        ..WebConfig::default()
    };
    router(AppState::new(
        Arc::new(accounts),
        Arc::new(NoCounters),
        OpacPorts {
            catalog: Arc::new(common::Catalog::default()),
            index: Arc::new(common::Index::default()),
        },
        StaffPorts {
            listing: Arc::new(common::Families::default()),
            records: Arc::new(common::Records(Vec::new())),
            editor: Arc::new(common::Editor::default()),
            suggestions: Arc::new(common::Suggestions),
            items: Arc::new(common::ItemGrid),
        },
        I18n::english().unwrap(),
        config,
    ))
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

async fn body(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn post(uri: &str, cookie: &str, form: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_owned()))
        .unwrap()
}

/// The `name=value` part of the response cookie called `name`.
fn set_cookie(response: &Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{name}=")))
        .and_then(|v| v.split(';').next())
        .map(str::to_owned)
}

fn hidden_csrf(html: &str) -> String {
    html.split(r#"name="csrf" value=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned()
}

fn location(response: &Response<Body>) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

#[tokio::test]
async fn opac_home_renders_in_english_with_security_headers() {
    let response = send(&app(), get("/", None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert!(
        headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("default-src 'self'")
    );
    assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
    let html = body(response).await;
    assert!(html.contains(r#"<html lang="en">"#));
    assert!(html.contains("Library catalog"));
}

#[tokio::test]
async fn static_assets_are_served() {
    let app = app();
    let css = send(&app, get("/static/elzevir.css", None)).await;
    assert_eq!(css.status(), StatusCode::OK);
    assert!(body(css).await.contains("--elzevir-color-primary"));
    let js = send(&app, get("/static/htmx.min.js", None)).await;
    assert_eq!(js.status(), StatusCode::OK);
}

#[tokio::test]
async fn staff_pages_require_a_session() {
    let response = send(&app(), get("/equipe", None)).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/equipe/login");
}

#[tokio::test]
async fn full_sign_in_and_sign_out() {
    let app = app();

    let form = send(&app, get("/equipe/login", None)).await;
    let login_cookie = set_cookie(&form, "elzevir_login_csrf").unwrap();
    let csrf = hidden_csrf(&body(form).await);

    let wrong = send(
        &app,
        post(
            "/equipe/login",
            &login_cookie,
            &format!("login=ana&password=nope&csrf={csrf}"),
        ),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let retry_cookie = set_cookie(&wrong, "elzevir_login_csrf").unwrap();
    let wrong_html = body(wrong).await;
    assert!(wrong_html.contains("Invalid username or password."));
    assert!(wrong_html.contains(r#"value="ana""#));
    let retry_csrf = hidden_csrf(&wrong_html);

    let ok = send(
        &app,
        post(
            "/equipe/login",
            &retry_cookie,
            &format!("login=ana&password=s3cret&csrf={retry_csrf}"),
        ),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&ok), "/equipe");
    let session = set_cookie(&ok, "elzevir_session").unwrap();

    let home = send(&app, get("/equipe", Some(&session))).await;
    assert_eq!(home.status(), StatusCode::OK);
    let home_html = body(home).await;
    assert!(home_html.contains(r#"<strong id="signed-in-as">ana</strong>"#));
    assert!(home_html.contains("Administration"));
    let logout_csrf = hidden_csrf(&home_html);

    let memory = send(&app, get("/equipe/admin/memory", Some(&session))).await;
    assert_eq!(memory.status(), StatusCode::OK);
    assert_eq!(
        body(memory).await,
        r#"{"allocator":"system","counters":null}"#
    );

    let forged = send(&app, post("/equipe/logout", &session, "csrf=forged")).await;
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);

    let out = send(
        &app,
        post("/equipe/logout", &session, &format!("csrf={logout_csrf}")),
    )
    .await;
    assert_eq!(out.status(), StatusCode::SEE_OTHER);

    let after = send(&app, get("/equipe", Some(&session))).await;
    assert_eq!(after.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn sign_in_without_the_form_token_is_forbidden() {
    let response = send(
        &app(),
        post("/equipe/login", "", "login=ana&password=s3cret&csrf=x"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_memory_report_is_for_administrators_only() {
    let app = app();
    let form = send(&app, get("/equipe/login", None)).await;
    let login_cookie = set_cookie(&form, "elzevir_login_csrf").unwrap();
    let csrf = hidden_csrf(&body(form).await);
    let ok = send(
        &app,
        post(
            "/equipe/login",
            &login_cookie,
            &format!("login=leo&password=s3cret&csrf={csrf}"),
        ),
    )
    .await;
    let session = set_cookie(&ok, "elzevir_session").unwrap();

    let memory = send(&app, get("/equipe/admin/memory", Some(&session))).await;
    assert_eq!(memory.status(), StatusCode::FORBIDDEN);
    let anonymous = send(&app, get("/equipe/admin/memory", None)).await;
    assert_eq!(anonymous.status(), StatusCode::SEE_OTHER);
}
