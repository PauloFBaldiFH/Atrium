// SPDX-License-Identifier: GPL-3.0-or-later
//! The Items of an Instance as a grid (docs/specs/catalogo/exemplares.md, items 12 and 17
//! to 28). The server writes the rows; `<elz-item-grid>` in the catalog script edits them.

use askama::Template;

use crate::I18n;

/// The grid of one Instance.
#[derive(Template)]
#[template(path = "staff_item_grid.html")]
pub struct ItemGrid<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Where the batch is sent.
    pub endpoint: String,
    /// The record's version when the grid was read.
    pub version: i64,
    /// The columns, in order.
    pub columns: &'static [GridColumn],
    /// The stored Items, in creation order.
    pub rows: Vec<GridRow>,
    /// The codes the cells with a list choose from.
    pub lists: Vec<GridList>,
}

/// One editable column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridColumn {
    /// The column's name in the batch's JSON.
    pub name: &'static str,
    /// Message key of its header.
    pub label_key: &'static str,
    /// The list its cell suggests from, if any.
    pub list: Option<&'static str>,
    /// Whether a new row copies it from the row above (item 19).
    pub inherited: bool,
}

/// One stored Item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridRow {
    /// The Item's id.
    pub id: i64,
    /// CSS class of its circulation signal.
    pub signal_class: &'static str,
    /// Message key of its circulation signal.
    pub signal_key: &'static str,
    /// The cells, in column order.
    pub cells: Vec<GridCell>,
}

/// One cell of a stored Item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// The value the batch carries: a code in a coded column.
    pub value: String,
    /// In a coded column, the name shown in place of the code.
    pub name: Option<String>,
}

/// The codes of one list, with their names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridList {
    /// Its name, as columns refer to it.
    pub name: &'static str,
    /// Each code and its name, in code order.
    pub options: Vec<(String, String)>,
}
