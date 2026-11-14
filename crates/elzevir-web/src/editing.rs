// SPDX-License-Identifier: GPL-3.0-or-later
//! Editing a Work or an Instance from the staff screen (docs/specs/catalogo/edicao.md,
//! items 16 to 24). The routes turn a JSON edit into a command; the writer applies it.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use elzevir_app::catalog::{
    CatalogOutput, CatalogRefusal, InstanceChange, InstanceUpdate, WorkChange, WorkUpdate,
};
use elzevir_app::editing::{
    Edit, EditFailed, FieldError, validate_instance_changes, validate_work_changes,
};
use elzevir_app::staff::Permission;
use elzevir_domain::catalog::{InstanceId, WorkId};
use serde::{Deserialize, Serialize};

use crate::AppState;
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
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<ActiveSession, StatusCode> {
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

fn invalid(state: &AppState, errors: &[FieldError]) -> Response {
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

async fn apply(state: &AppState, session: &ActiveSession, edit: Edit) -> Response {
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
