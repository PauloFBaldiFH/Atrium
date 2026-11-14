// SPDX-License-Identifier: GPL-3.0-or-later
//! The focused block as a form, in its profile's order (docs/specs/catalogo/edicao.md,
//! items 25 to 32).

use askama::Template;

use crate::I18n;

/// The form of one Work or one Instance.
#[derive(Template)]
#[template(path = "staff_record_form.html")]
pub struct RecordForm<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Where the edit is sent.
    pub endpoint: String,
    /// The version the form was read at.
    pub version: i64,
    /// Filled fields, in the profile's order.
    pub filled: Vec<FormField>,
    /// The profile's empty fields, in its order.
    pub empty: Vec<FormField>,
}

/// One field of the form: one change of the edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    /// The change's name in the edit (`Title`, `Identifiers`).
    pub change: &'static str,
    /// The field name refusals use (`title`, `identifiers`).
    pub error: &'static str,
    /// Message key of the label.
    pub label_key: &'static str,
    /// The input.
    pub input: FormInput,
}

/// How a field is entered, and how the script turns it into JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormInput {
    /// One line; `required` sends a string, otherwise an empty line clears the field.
    Text {
        /// The stored value.
        value: String,
        /// Whether it cannot be cleared.
        required: bool,
    },
    /// Several lines; empty clears the field.
    LongText {
        /// The stored value.
        value: String,
    },
    /// One choice.
    Select {
        /// The stored choice, empty when none.
        value: String,
        /// The choices.
        options: Vec<FormOption>,
    },
    /// A list of rows, each sent as one object.
    Rows {
        /// The stored rows.
        rows: Vec<Vec<FormCell>>,
        /// A new row.
        blank: Vec<FormCell>,
    },
    /// A list of single values, sent as an array of strings.
    Values {
        /// The stored values.
        values: Vec<String>,
    },
    /// Several values sent together as one object.
    Group {
        /// Its parts, with their stored values.
        cells: Vec<FormCell>,
    },
}

/// One column of a row or part of a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormColumn {
    /// Its name in the JSON object.
    pub name: &'static str,
    /// Message key of its label.
    pub label_key: &'static str,
    /// How it is entered.
    pub kind: ColumnKind,
    /// Whether an empty value is sent as `null`.
    pub optional: bool,
}

/// One column of one row, with its value (`true` or `false` for a check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormCell {
    /// The column.
    pub column: FormColumn,
    /// Its value in this row.
    pub value: String,
}

/// How a column is entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    /// Free text.
    Text,
    /// One of `options`.
    Select(Vec<FormOption>),
    /// Yes or no.
    Check,
}

/// One choice of a select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormOption {
    /// The value sent.
    pub value: &'static str,
    /// The label shown, in the interface language.
    pub label: String,
}
