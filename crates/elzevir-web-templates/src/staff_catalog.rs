// SPDX-License-Identifier: GPL-3.0-or-later
//! The staff catalog listing: family rows (docs/specs/catalogo/listagem.md).

use askama::Template;

use crate::I18n;

/// The listing page, or only its rows for the next page.
#[derive(Template)]
#[template(path = "staff_catalog.html", blocks = ["rows"])]
pub struct StaffCatalog<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// The query, empty when browsing.
    pub query: &'a str,
    /// Records found, for a search.
    pub total: Option<usize>,
    /// The rows of this page.
    pub rows: Vec<FamilyLine>,
    /// Where the next page is, when there is one.
    pub next_url: Option<String>,
}

impl StaffCatalog<'_> {
    /// How many records matched, as a sentence, for a search.
    #[must_use]
    pub fn count_text(&self) -> Option<String> {
        self.total.map(|total| {
            self.t.get_numbers(
                "staff-catalog-count",
                &[("count", u64::try_from(total).unwrap_or(u64::MAX))],
            )
        })
    }
}

/// One family row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyLine {
    /// The Hub's title, for a Hub family.
    pub hub_title: Option<String>,
    /// Its Works.
    pub works: Vec<WorkLine>,
}

/// One Work in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkLine {
    /// Preferred title.
    pub title: String,
    /// Main contributor.
    pub contributor: Option<String>,
    /// Classification number.
    pub classification: Option<String>,
    /// Language codes.
    pub languages: Vec<String>,
    /// The subjects shown.
    pub subjects: Vec<String>,
    /// Subjects not shown.
    pub more_subjects: usize,
    /// Its Instances.
    pub instances: Vec<InstanceLine>,
}

/// One Instance in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceLine {
    /// Instance id, for the link to the record.
    pub id: i64,
    /// Title.
    pub title: String,
    /// Edition statement.
    pub edition: Option<String>,
    /// Publisher and date.
    pub imprint: Option<String>,
    /// First ISBN.
    pub isbn: Option<String>,
    /// Message key of the category of material.
    pub material: Option<&'static str>,
    /// Whether the search found it.
    pub found: bool,
    /// Items in all.
    pub items: usize,
    /// The pills shown.
    pub pills: Vec<PillLine>,
    /// Items not shown.
    pub more_items: usize,
}

impl InstanceLine {
    /// "+N items", for the Items not shown.
    #[must_use]
    pub fn more_items_text(&self, t: &I18n) -> String {
        t.get_numbers(
            "staff-catalog-more-items",
            &[("count", u64::try_from(self.more_items).unwrap_or(u64::MAX))],
        )
    }
}

/// One Item pill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PillLine {
    /// `branch | location | call number | inventory number`, the parts it has.
    pub text: String,
    /// CSS class of its signal.
    pub signal_class: &'static str,
    /// Message key of its signal.
    pub signal_key: &'static str,
}
