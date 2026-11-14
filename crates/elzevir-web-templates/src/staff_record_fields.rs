// SPDX-License-Identifier: GPL-3.0-or-later
//! Every filled field of a focused block (docs/specs/catalogo/listagem.md, items 42 to 45).

use askama::Template;

use crate::I18n;

/// The fields of one Work or one Instance, loaded into its block when it takes the focus.
#[derive(Template)]
#[template(path = "staff_record_fields.html")]
pub struct RecordFields<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// The filled fields, by group.
    pub groups: Vec<FieldGroup>,
    /// The Items, for an Instance.
    pub items: Vec<ItemFields>,
}

/// A group of fields under one heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldGroup {
    /// Message key of the heading.
    pub heading_key: &'static str,
    /// Its filled fields, in order.
    pub fields: Vec<FieldEntry>,
}

/// One field and its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEntry {
    /// Message key of the label.
    pub label_key: &'static str,
    /// The values, in order.
    pub values: Vec<FieldValue>,
}

/// One value, with what qualifies it (a role, a scheme, a kind), already in the interface
/// language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldValue {
    /// The value as stored.
    pub text: String,
    /// What qualifies it.
    pub qualifier: Option<String>,
}

/// One Item of the Instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFields {
    /// Barcode.
    pub barcode: Option<String>,
    /// Inventory number (tombo).
    pub inventory_number: Option<String>,
    /// Home branch code.
    pub home_branch: String,
    /// Holding branch code, when it differs from the home branch.
    pub holding_branch: Option<String>,
    /// Item type code.
    pub item_type: String,
    /// Call number.
    pub call_number: Option<String>,
    /// Shelving location code.
    pub location: Option<String>,
    /// Collection code.
    pub collection: Option<String>,
    /// Copy number.
    pub copy_number: Option<String>,
    /// Date acquired, as stored.
    pub acquired_on: Option<String>,
    /// CSS class of its signal.
    pub signal_class: &'static str,
    /// Message key of its signal.
    pub signal_key: &'static str,
}
