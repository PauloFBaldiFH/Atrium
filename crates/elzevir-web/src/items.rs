// SPDX-License-Identifier: GPL-3.0-or-later
//! Saving an Instance's Items from the grid (docs/specs/catalogo/exemplares.md, items 10
//! to 15).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use elzevir_app::editing::Edit;
use elzevir_app::items::{ItemsUpdate, MAX_ROWS, TypedRow, item_rows};
use elzevir_domain::catalog::InstanceId;
use serde::Deserialize;

use crate::AppState;
use crate::editing::{apply, authorize, invalid_cells};

/// A batch as the grid sends it: the version read and the rows as typed.
#[derive(Deserialize)]
pub(crate) struct ItemsBody {
    version: i64,
    rows: Vec<TypedRow>,
}

/// `POST /equipe/acervo/instancia/{id}/exemplares` (item 13).
pub(crate) async fn save_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ItemsBody>,
) -> Response {
    let session = match authorize(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    if body.rows.len() > MAX_ROWS {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too many rows").into_response();
    }
    let rows = match item_rows(body.rows) {
        Ok(rows) => rows,
        Err(refusals) => return invalid_cells(&state, &refusals),
    };
    let update = ItemsUpdate {
        instance: InstanceId(id),
        row_version: body.version,
        rows,
    };
    apply(&state, &session, Edit::Items(Box::new(update))).await
}

#[cfg(test)]
mod tests {
    use elzevir_app::items::RowProblem;
    use elzevir_web_templates::I18n;

    #[test]
    fn every_refused_cell_has_a_message() {
        let t = I18n::english().unwrap();
        for problem in [
            RowProblem::Required,
            RowProblem::Price,
            RowProblem::UnknownBranch,
            RowProblem::UnknownItemType,
            RowProblem::BarcodeTaken,
            RowProblem::UnknownItem,
        ] {
            assert!(t.has(problem.message_key()), "{problem:?}");
        }
        assert!(t.has("edit-error-material-required"));
    }
}
