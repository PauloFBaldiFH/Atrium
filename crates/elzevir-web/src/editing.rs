// SPDX-License-Identifier: GPL-3.0-or-later
//! Editing a Work or an Instance from the staff screen (docs/specs/catalogo/edicao.md,
//! items 16 to 24). The routes turn a JSON edit into a command; the writer applies it.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use elzevir_app::catalog::CatalogReadError;
use elzevir_app::catalog::{
    CatalogOutput, CatalogRefusal, InstanceChange, InstanceUpdate, WorkChange, WorkUpdate,
};
use elzevir_app::editing::{
    Edit, EditFailed, FieldError, validate_instance_changes, validate_work_changes,
};
use elzevir_app::staff::Permission;
use elzevir_app::suggestions::{AuthoritySuggestions, SUGGESTION_LIMIT, lookup_prefix};
use elzevir_domain::catalog::{InstanceId, WorkId};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::record_form::{agent_name, subject_name};
use crate::security::tokens_match;
use crate::session::ActiveSession;
use crate::staff::current_session;

/// Header that carries the session's CSRF token on a script's request.
const CSRF_HEADER: &str = "x-elzevir-csrf";

/// An edit as the screen sends it: the version read and the changes, in the command's
/// own JSON form.
#[derive(Deserialize)]
pub(crate) struct EditBody<C> {
    version: i64,
    changes: Vec<C>,
}

/// One refused field, with its message in the interface language.
#[derive(Serialize)]
struct FieldMessage {
    field: &'static str,
    key: &'static str,
    message: String,
}

/// The answer to an accepted edit: the new version of what was edited.
#[derive(Serialize)]
struct Accepted {
    version: i64,
}

/// The answer to a creation: the new Instance, for the screen to focus its family.
#[derive(Serialize)]
struct Created {
    instance: i64,
}

/// The answer to a stale edit: the version stored now.
#[derive(Serialize)]
struct Conflict {
    current: i64,
}

/// The answer to an invalid edit.
#[derive(Serialize)]
struct Invalid {
    errors: Vec<FieldMessage>,
}

/// `POST /equipe/acervo/obra/{id}`.
pub(crate) async fn edit_work(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<EditBody<WorkChange>>,
) -> Response {
    let session = match authorize(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    let mut changes = body.changes;
    if let Err(errors) = validate_work_changes(&mut changes) {
        return invalid(&state, &errors);
    }
    let edit = Edit::Work(WorkUpdate {
        work: WorkId(id),
        row_version: body.version,
        changes,
    });
    apply(&state, &session, edit).await
}

/// `POST /equipe/acervo/instancia/{id}`.
pub(crate) async fn edit_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<EditBody<InstanceChange>>,
) -> Response {
    let session = match authorize(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    let mut changes = body.changes;
    if let Err(errors) = validate_instance_changes(&mut changes) {
        return invalid(&state, &errors);
    }
    let edit = Edit::Instance(InstanceUpdate {
        instance: InstanceId(id),
        row_version: body.version,
        changes,
    });
    apply(&state, &session, edit).await
}

/// Item 17: a session, its CSRF token, and the cataloguing permission.
pub(crate) fn authorize(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ActiveSession, StatusCode> {
    let Some(session) = current_session(state, headers) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !tokens_match(&session.csrf, token) || !session.identity.can(Permission::Cataloguing) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(session)
}

pub(crate) fn invalid(state: &AppState, errors: &[FieldError]) -> Response {
    let t = &state.inner.i18n;
    let errors: Vec<FieldMessage> = errors
        .iter()
        .map(|error| FieldMessage {
            field: error.field,
            key: error.message_key,
            message: t.get(error.message_key),
        })
        .collect();
    (StatusCode::UNPROCESSABLE_ENTITY, Json(Invalid { errors })).into_response()
}

pub(crate) async fn apply(state: &AppState, session: &ActiveSession, edit: Edit) -> Response {
    let editor = Arc::clone(&state.inner.staff_ports.editor);
    let actor = session.identity.id;
    let outcome = tokio::task::spawn_blocking(move || editor.apply(actor, edit)).await;
    match outcome {
        Ok(Ok(
            CatalogOutput::WorkUpdated { row_version, .. }
            | CatalogOutput::InstanceUpdated { row_version, .. },
        )) => Json(Accepted {
            version: row_version,
        })
        .into_response(),
        Ok(Ok(
            CatalogOutput::Record { instance, .. } | CatalogOutput::InstanceAdded { instance, .. },
        )) => (
            StatusCode::CREATED,
            Json(Created {
                instance: instance.0,
            }),
        )
            .into_response(),
        Ok(Ok(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "unexpected answer").into_response(),
        Ok(Err(EditFailed::Refused(CatalogRefusal::Stale { current, .. }))) => {
            (StatusCode::CONFLICT, Json(Conflict { current })).into_response()
        }
        Ok(Err(EditFailed::Refused(
            CatalogRefusal::UnknownWork(_) | CatalogRefusal::UnknownInstance(_),
        ))) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(EditFailed::Refused(refusal))) => {
            (StatusCode::UNPROCESSABLE_ENTITY, refusal.to_string()).into_response()
        }
        Ok(Err(EditFailed::Busy)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::RETRY_AFTER, "1")],
        )
            .into_response(),
        Ok(Err(EditFailed::Unavailable(_))) | Err(_) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// Query string of a suggestion lookup.
#[derive(Deserialize)]
pub(crate) struct SuggestParams {
    #[serde(default)]
    q: String,
}

/// One suggested agent or subject, as the form's row wants it.
#[derive(Serialize)]
struct Suggestion {
    heading: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    thesaurus: Option<String>,
}

/// `GET /equipe/acervo/sugestoes/agentes?q=` (docs/specs/catalogo/edicao.md, item 33).
pub(crate) async fn suggest_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SuggestParams>,
) -> Response {
    suggest(&state, &headers, params.q, |port, prefix| {
        port.agents(prefix, SUGGESTION_LIMIT).map(|agents| {
            agents
                .into_iter()
                .map(|agent| Suggestion {
                    heading: agent.heading,
                    kind: agent_name(agent.kind),
                    thesaurus: None,
                })
                .collect()
        })
    })
    .await
}

/// `GET /equipe/acervo/sugestoes/assuntos?q=`.
pub(crate) async fn suggest_subjects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SuggestParams>,
) -> Response {
    suggest(&state, &headers, params.q, |port, prefix| {
        port.subjects(prefix, SUGGESTION_LIMIT).map(|subjects| {
            subjects
                .into_iter()
                .map(|subject| Suggestion {
                    heading: subject.heading,
                    kind: subject_name(subject.kind),
                    thesaurus: subject.thesaurus,
                })
                .collect()
        })
    })
    .await
}

type Lookup = fn(&dyn AuthoritySuggestions, &str) -> Result<Vec<Suggestion>, CatalogReadError>;

async fn suggest(state: &AppState, headers: &HeaderMap, typed: String, lookup: Lookup) -> Response {
    if current_session(state, headers).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(prefix) = lookup_prefix(&typed).map(str::to_owned) else {
        return Json(Vec::<Suggestion>::new()).into_response();
    };
    let port = Arc::clone(&state.inner.staff_ports.suggestions);
    match tokio::task::spawn_blocking(move || lookup(&*port, &prefix)).await {
        Ok(Ok(found)) => Json(found).into_response(),
        Ok(Err(_)) | Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
