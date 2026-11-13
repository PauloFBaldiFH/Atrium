// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP layer: axum routes for the OPAC and the staff interface, HTMX fragments.
//!
//! OPAC at `/`, staff interface at `/equipe` on the same listener by default
//! (docs/arquitetura/04-decisoes.md, §5). The web layer reaches data only through
//! `elzevir-app` ports; it never opens the database.

mod assets;
mod security;
mod session;
mod staff;

use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use elzevir_app::memory::MemoryProbe;
use elzevir_app::staff::StaffDirectory;
use elzevir_web_templates::{I18n, OpacHome};

pub use session::Sessions;

/// Web settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebConfig {
    /// Mark cookies `Secure`. Off only for plain-HTTP development and tests.
    pub secure_cookies: bool,
    /// How long a staff session lasts without use.
    pub session_idle: Duration,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            secure_cookies: true,
            session_idle: Duration::from_hours(8),
        }
    }
}

/// Shared state of the web layer.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    staff: Arc<dyn StaffDirectory>,
    memory: Arc<dyn MemoryProbe>,
    sessions: Sessions,
    i18n: I18n,
    config: WebConfig,
}

impl AppState {
    /// Builds the state from its ports.
    #[must_use]
    pub fn new(
        staff: Arc<dyn StaffDirectory>,
        memory: Arc<dyn MemoryProbe>,
        i18n: I18n,
        config: WebConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                staff,
                memory,
                sessions: Sessions::new(config.session_idle),
                i18n,
                config,
            }),
        }
    }
}

/// All routes, with security headers on every response.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(opac_home))
        .route("/static/elzevir.css", get(assets::stylesheet))
        .route("/static/htmx.min.js", get(assets::htmx))
        .route("/equipe", get(staff::home))
        .route("/equipe/login", get(staff::login_form).post(staff::log_in))
        .route("/equipe/logout", post(staff::log_out))
        .route("/equipe/admin/memory", get(staff::memory))
        .layer(axum::middleware::map_response(security::headers))
        .with_state(state)
}

async fn opac_home(State(state): State<AppState>) -> Response {
    render(&OpacHome {
        t: &state.inner.i18n,
    })
}

/// Renders a template, or answers 500 without leaking the error to the visitor.
fn render(page: &impl Template) -> Response {
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
    }
}
