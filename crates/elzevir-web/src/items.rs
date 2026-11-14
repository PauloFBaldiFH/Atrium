// SPDX-License-Identifier: GPL-3.0-or-later
//! The grid of an Instance's Items: reading it and saving its batch
//! (docs/specs/catalogo/exemplares.md, items 10 to 15).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use elzevir_app::editing::Edit;
use elzevir_app::items::{GridItem, ItemFields, ItemsUpdate, MAX_ROWS, TypedRow, item_rows};
use elzevir_app::staff::Permission;
use elzevir_domain::catalog::{InstanceId, Money};
use elzevir_web_templates::{GridColumn, GridList, GridRow, ItemGrid};
use serde::Deserialize;

use crate::catalog::signal;
use crate::editing::{apply, authorize, invalid_cells};
use crate::staff::current_session;
use crate::{AppState, render};

const fn column(
    name: &'static str,
    label_key: &'static str,
    list: Option<&'static str>,
    inherited: bool,
) -> GridColumn {
    GridColumn {
        name,
        label_key,
        list,
        inherited,
    }
}

/// The grid's columns (item 1). A new row copies the branches, location, collection,
/// item type and call number of the row above (item 19).
const COLUMNS: &[GridColumn] = &[
    column("barcode", "items-column-barcode", None, false),
    column("inventory_number", "items-column-inventory", None, false),
    column("call_number", "items-column-call-number", None, true),
    column(
        "home_branch",
        "items-column-home-branch",
        Some("branches"),
        true,
    ),
    column(
        "holding_branch",
        "items-column-holding-branch",
        Some("branches"),
        true,
    ),
    column("location", "items-column-location", Some("locations"), true),
    column(
        "collection",
        "items-column-collection",
        Some("collections"),
        true,
    ),
    column(
        "item_type",
        "items-column-item-type",
        Some("item-types"),
        true,
    ),
    column("copy_number", "items-column-copy", None, false),
    column("price", "items-column-price", None, false),
];

/// The cells of one Item, in [`COLUMNS`] order.
fn cells(fields: ItemFields) -> Vec<String> {
    let price = fields
        .price
        .map(|Money(cents)| format!("{},{:02}", cents / 100, cents % 100));
    [
        fields.barcode,
        fields.inventory_number,
        fields.call_number,
        Some(fields.home_branch),
        Some(fields.holding_branch),
        fields.location,
        fields.collection,
        Some(fields.item_type),
        fields.copy_number,
        price,
    ]
    .into_iter()
    .map(Option::unwrap_or_default)
    .collect()
}

fn grid_row(item: GridItem) -> GridRow {
    let (signal_class, signal_key) = signal(item.status.signal());
    GridRow {
        id: item.id.0,
        signal_class,
        signal_key,
        cells: cells(item.fields),
    }
}

/// `GET /equipe/acervo/instancia/{id}/exemplares` (item 12).
pub(crate) async fn item_grid(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    let Some(session) = current_session(&state, &headers) else {
        return Redirect::to("/equipe/login").into_response();
    };
    if !session.identity.can(Permission::Cataloguing) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let items = Arc::clone(&state.inner.staff_ports.items);
    let catalog = Arc::clone(&state.inner.opac.catalog);
    let instance = InstanceId(id);
    let read = tokio::task::spawn_blocking(move || {
        let view = catalog.instance(instance).map_err(drop)?;
        let rows = items.grid_items(instance).map_err(drop)?;
        let codes = items.code_lists().map_err(drop)?;
        Ok::<_, ()>((view, rows, codes))
    })
    .await;
    let (view, rows, codes) = match read {
        Ok(Ok((Some(view), rows, codes))) => (view, rows, codes),
        Ok(Ok((None, _, _))) => return (StatusCode::NOT_FOUND, "no such record").into_response(),
        Ok(Err(())) | Err(_) => {
            return (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response();
        }
    };
    render(&ItemGrid {
        t: &state.inner.i18n,
        endpoint: format!("/equipe/acervo/instancia/{id}/exemplares"),
        version: view.version,
        columns: COLUMNS,
        rows: rows.into_iter().map(grid_row).collect(),
        lists: vec![
            GridList {
                name: "branches",
                codes: codes.branches,
            },
            GridList {
                name: "item-types",
                codes: codes.item_types,
            },
            GridList {
                name: "locations",
                codes: codes.locations,
            },
            GridList {
                name: "collections",
                codes: codes.collections,
            },
        ],
    })
}

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
    fn the_grid_columns_are_the_batch_names() {
        // The script sends each cell under its column's name; TypedRow must read it.
        use serde::Deserialize;
        use serde::de::value::{Error, MapDeserializer};
        let cells = super::COLUMNS.iter().map(|column| (column.name, "x"));
        let row =
            elzevir_app::items::TypedRow::deserialize(MapDeserializer::<_, Error>::new(cells))
                .unwrap();
        assert_eq!(row.barcode, "x");
        assert_eq!(row.price, "x");
        assert_eq!(row.holding_branch, "x");
        assert_eq!(row.copy_number, "x");
        let t = I18n::english().unwrap();
        for column in super::COLUMNS {
            assert!(t.has(column.label_key), "{}", column.name);
        }
    }

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
