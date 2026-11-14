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
    /// The session's CSRF token, which the page's script sends with an edit.
    pub csrf: &'a str,
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

/// One family: a compact row that opens into its blocks
/// (docs/specs/catalogo/listagem.md, items 27 to 32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyLine {
    /// Its head, `h<id>` or `w<id>`: the element id and the URL fragment of its focus.
    pub head: String,
    /// The Hub's title, or the Work's.
    pub title: String,
    /// Icon of the material of its first Instance.
    pub icon: &'static str,
    /// Classification of its first Work.
    pub classification: Option<String>,
    /// Instances in all its Works.
    pub instances: usize,
    /// Items in all its Instances.
    pub items: usize,
    /// Of those, the ones available.
    pub available: usize,
    /// Whether it opens focused: the first family a search found an Instance in.
    pub open: bool,
    /// The agents shown in the authorities column, each once.
    pub authorities: Vec<AuthorityLine>,
    /// The Hub, for a Hub family.
    pub hub: Option<HubLine>,
    /// Its Works.
    pub works: Vec<WorkLine>,
}

impl FamilyLine {
    /// "2 works, 3 instances".
    #[must_use]
    pub fn shape_text(&self, t: &I18n) -> String {
        t.get_numbers(
            "staff-row-shape",
            &[
                ("works", u64::try_from(self.works.len()).unwrap_or(u64::MAX)),
                (
                    "instances",
                    u64::try_from(self.instances).unwrap_or(u64::MAX),
                ),
            ],
        )
    }

    /// "3 of 5 available".
    #[must_use]
    pub fn availability_text(&self, t: &I18n) -> String {
        t.get_numbers(
            "staff-row-available",
            &[
                (
                    "available",
                    u64::try_from(self.available).unwrap_or(u64::MAX),
                ),
                ("items", u64::try_from(self.items).unwrap_or(u64::MAX)),
            ],
        )
    }
}

/// An agent in the authorities column, linked to the Works it contributes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityLine {
    /// Position in the row, to link the cords.
    pub key: usize,
    /// Heading.
    pub name: String,
}

/// The Hub block of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubLine {
    /// Hub id, to link the cords.
    pub id: i64,
    /// Title.
    pub title: String,
}

/// One Work in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkLine {
    /// Work id, to link the cords.
    pub id: i64,
    /// Preferred title.
    pub title: String,
    /// Main contributor.
    pub contributor: Option<String>,
    /// The [`AuthorityLine::key`] of the main contributor.
    pub authority: Option<usize>,
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
    /// Icon of its material type in `/static/elzevir-icons.svg`, shown on each item.
    pub icon: &'static str,
    /// Whether the search found it.
    pub found: bool,
    /// Which of the cover colours it gets.
    pub cover: u8,
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
