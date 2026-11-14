// SPDX-License-Identifier: GPL-3.0-or-later
//! Creating a family or an Instance from the staff screen (docs/specs/catalogo/criacao.md,
//! items 5 to 12). The screen sends the same changes as an edit; the app turns them into
//! a new record and the writer stores it.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use elzevir_app::catalog::{InstanceChange, NewInstanceOfWork, WorkChange};
use elzevir_app::creating::{new_instance, new_record};
use elzevir_app::editing::Edit;
use elzevir_domain::catalog::WorkId;
use serde::Deserialize;

use crate::AppState;
use crate::editing::{apply, authorize, invalid};

/// A new family as the screen sends it.
#[derive(Deserialize)]
pub(crate) struct NewFamilyBody {
    #[serde(default)]
    work: Vec<WorkChange>,
    #[serde(default)]
    instance: Vec<InstanceChange>,
}

/// A new Instance as the screen sends it.
#[derive(Deserialize)]
pub(crate) struct NewInstanceBody {
    changes: Vec<InstanceChange>,
}

/// `POST /equipe/acervo/nova` (item 5).
pub(crate) async fn create_family(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NewFamilyBody>,
) -> Response {
    let session = match authorize(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    match new_record(body.work, body.instance) {
        Ok(record) => apply(&state, &session, Edit::NewRecord(Box::new(record))).await,
        Err(errors) => invalid(&state, &errors),
    }
}

/// `POST /equipe/acervo/obra/{id}/instancias` (item 6).
pub(crate) async fn create_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NewInstanceBody>,
) -> Response {
    let session = match authorize(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    match new_instance(body.changes) {
        Ok(instance) => {
            let new = NewInstanceOfWork {
                work: WorkId(id),
                instance,
            };
            apply(&state, &session, Edit::NewInstance(Box::new(new))).await
        }
        Err(errors) => invalid(&state, &errors),
    }
}
