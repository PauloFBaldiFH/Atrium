// SPDX-License-Identifier: GPL-3.0-or-later
//! The staff listing: the catalog as family rows, a page at a time
//! (docs/specs/catalogo/listagem.md, UI spec §2 and §8).
//!
//! A family is a Hub with all its Works, or a Work outside any Hub. The store reads whole
//! families through [`CatalogListing`]; this module decides what a row shows.

use std::num::NonZeroUsize;

use elzevir_domain::catalog::{HubId, InstanceId, ItemId, MaterialType, WorkId};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::catalog::{CatalogReadError, ItemView, ShelfSignal};

/// Families per page.
pub const PAGE_SIZE: NonZeroUsize = match NonZeroUsize::new(30) {
    Some(size) => size,
    None => NonZeroUsize::MIN,
};

/// Item pills shown per Instance; the rest is counted as "+N".
pub const PILLS_PER_INSTANCE: usize = 4;

/// Subjects shown per Work; the rest is counted as "+N".
pub const SUBJECTS_PER_WORK: usize = 3;

/// The key families are sorted by: the title lower-cased, without accents, punctuation
/// or repeated spaces, so `Ética`, `ética` and `[Etica]` sort together.
#[must_use]
pub fn sort_key(title: &str) -> String {
    let mut key = String::with_capacity(title.len());
    let mut space = false;
    for c in title.nfd().filter(|c| !is_combining_mark(*c)) {
        if c.is_alphanumeric() {
            if space && !key.is_empty() {
                key.push(' ');
            }
            space = false;
            key.extend(c.to_lowercase());
        } else {
            space = true;
        }
    }
    key
}

/// What a family is built around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FamilyHead {
    /// A Hub and all its Works.
    Hub(HubId),
    /// A Work outside any Hub.
    Work(WorkId),
}

/// Where a family sits in the listing: families are ordered by `sort_key`, then by head
/// (Hubs before Works, then by id), so the order is total and a page can start after any
/// family.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FamilyCursor {
    /// [`sort_key`] of the Hub's or the Work's title.
    pub sort_key: String,
    /// The family.
    pub head: FamilyHead,
}

/// One family as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// Its place in the listing.
    pub cursor: FamilyCursor,
    /// The Hub's title, for a Hub family.
    pub hub_title: Option<String>,
    /// Its Works, in id order (one for a Work family).
    pub works: Vec<FamilyWork>,
}

/// One Work of a family as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyWork {
    /// The Work.
    pub id: WorkId,
    /// Preferred title.
    pub title: String,
    /// The main entry, else the first contributor.
    pub main_contributor: Option<String>,
    /// Classification number.
    pub classification: Option<String>,
    /// Language codes, in order.
    pub languages: Vec<String>,
    /// Subject headings, in order.
    pub subjects: Vec<String>,
    /// Its Instances, in id order.
    pub instances: Vec<FamilyInstance>,
}

/// One Instance of a family as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyInstance {
    /// The Instance.
    pub id: InstanceId,
    /// Its record's version, for the fragment cache.
    pub version: i64,
    /// Title as on the publication.
    pub title: String,
    /// Edition statement.
    pub edition: Option<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Date of publication.
    pub date: Option<String>,
    /// The first ISBN, as written.
    pub isbn: Option<String>,
    /// Category of material.
    pub material_type: Option<MaterialType>,
    /// Its Items, in creation order.
    pub items: Vec<ItemView>,
}

/// Port: reading families for the listing. The store implements it.
pub trait CatalogListing: Send + Sync {
    /// Up to `limit` families after `after` (from the first when `None`), in listing
    /// order.
    ///
    /// # Errors
    ///
    /// When storage fails or holds a value this build does not understand.
    fn families_after(
        &self,
        after: Option<&FamilyCursor>,
        limit: NonZeroUsize,
    ) -> Result<Vec<Family>, CatalogReadError>;
}

/// One page of the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingPage {
    /// The rows, in listing order.
    pub rows: Vec<FamilyRow>,
    /// Where the next page starts, when there is one.
    pub next: Option<FamilyCursor>,
}

/// One family row as the listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyRow {
    /// Its place in the listing.
    pub cursor: FamilyCursor,
    /// The Hub's title, for a Hub family.
    pub hub_title: Option<String>,
    /// Its Works.
    pub works: Vec<WorkRow>,
}

/// One Work in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRow {
    /// The Work.
    pub id: WorkId,
    /// Preferred title.
    pub title: String,
    /// The main contributor.
    pub main_contributor: Option<String>,
    /// Classification number.
    pub classification: Option<String>,
    /// Language codes.
    pub languages: Vec<String>,
    /// The first [`SUBJECTS_PER_WORK`] subjects.
    pub subjects: Vec<String>,
    /// How many subjects are not shown.
    pub more_subjects: usize,
    /// Its Instances.
    pub instances: Vec<InstanceRow>,
}

/// One Instance in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRow {
    /// The Instance.
    pub id: InstanceId,
    /// Its record's version.
    pub version: i64,
    /// Title as on the publication.
    pub title: String,
    /// Edition statement.
    pub edition: Option<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Date of publication.
    pub date: Option<String>,
    /// The first ISBN.
    pub isbn: Option<String>,
    /// Category of material.
    pub material_type: Option<MaterialType>,
    /// Items in all.
    pub items: usize,
    /// Items available.
    pub available: usize,
    /// The first [`PILLS_PER_INSTANCE`] Items, available first, then by inventory number.
    pub pills: Vec<ItemPill>,
    /// How many Items are not shown.
    pub more_items: usize,
}

/// One Item as a pill: `branch | location | call number | inventory number`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPill {
    /// The Item.
    pub id: ItemId,
    /// Home branch code.
    pub home_branch: String,
    /// Shelving location code.
    pub location: Option<String>,
    /// Call number.
    pub call_number: Option<String>,
    /// Inventory number (tombo).
    pub inventory_number: Option<String>,
    /// The counter signal.
    pub signal: ShelfSignal,
}

/// One page of the listing, after `after` (from the first when `None`).
///
/// # Errors
///
/// When the catalog fails.
pub fn browse<L: CatalogListing + ?Sized>(
    listing: &L,
    after: Option<&FamilyCursor>,
) -> Result<ListingPage, CatalogReadError> {
    // One more than a page tells whether a next page exists without counting.
    let ask = PAGE_SIZE.saturating_add(1);
    let mut families = listing.families_after(after, ask)?;
    let more = families.len() > PAGE_SIZE.get();
    families.truncate(PAGE_SIZE.get());
    let next = if more {
        families.last().map(|family| family.cursor.clone())
    } else {
        None
    };
    Ok(ListingPage {
        rows: families.into_iter().map(row).collect(),
        next,
    })
}

/// What a row shows of a stored family. Pure.
#[must_use]
pub fn row(family: Family) -> FamilyRow {
    FamilyRow {
        cursor: family.cursor,
        hub_title: family.hub_title,
        works: family.works.into_iter().map(work_row).collect(),
    }
}

fn work_row(work: FamilyWork) -> WorkRow {
    let more_subjects = work.subjects.len().saturating_sub(SUBJECTS_PER_WORK);
    let mut subjects = work.subjects;
    subjects.truncate(SUBJECTS_PER_WORK);
    WorkRow {
        id: work.id,
        title: work.title,
        main_contributor: work.main_contributor,
        classification: work.classification,
        languages: work.languages,
        subjects,
        more_subjects,
        instances: work.instances.into_iter().map(instance_row).collect(),
    }
}

fn instance_row(instance: FamilyInstance) -> InstanceRow {
    let items = instance.items.len();
    let mut pills: Vec<ItemPill> = instance.items.into_iter().map(pill).collect();
    let available = pills
        .iter()
        .filter(|pill| pill.signal == ShelfSignal::Available)
        .count();
    // Stable: Items with the same signal and inventory number keep creation order.
    pills.sort_by(|a, b| {
        (a.signal != ShelfSignal::Available, &a.inventory_number)
            .cmp(&(b.signal != ShelfSignal::Available, &b.inventory_number))
    });
    pills.truncate(PILLS_PER_INSTANCE);
    InstanceRow {
        id: instance.id,
        version: instance.version,
        title: instance.title,
        edition: instance.edition,
        publisher: instance.publisher,
        date: instance.date,
        isbn: instance.isbn,
        material_type: instance.material_type,
        items,
        available,
        more_items: items.saturating_sub(pills.len()),
        pills,
    }
}

fn pill(item: ItemView) -> ItemPill {
    ItemPill {
        id: item.id,
        signal: item.status.signal(),
        home_branch: item.home_branch,
        location: item.location,
        call_number: item.call_number,
        inventory_number: item.inventory_number,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ItemStatus;

    #[test]
    fn sort_keys_ignore_case_accents_and_punctuation() {
        assert_eq!(sort_key("Ética"), "etica");
        assert_eq!(sort_key("[Ética] : a  vida"), "etica a vida");
        assert_eq!(sort_key("  Coração... "), "coracao");
        assert_eq!(sort_key("1984"), "1984");
        assert_eq!(sort_key("¿?"), "");
    }

    #[test]
    fn the_signal_follows_the_item_status() {
        let status = |not_for_loan, lost| ItemStatus {
            not_for_loan,
            lost,
            ..ItemStatus::default()
        };
        assert_eq!(status(0, 0).signal(), ShelfSignal::Available);
        assert_eq!(status(1, 0).signal(), ShelfSignal::NotForLoan);
        assert_eq!(status(1, 2).signal(), ShelfSignal::OutOfCirculation);
        let restricted = ItemStatus {
            restricted: 1,
            ..ItemStatus::default()
        };
        assert_eq!(restricted.signal(), ShelfSignal::NotForLoan);
    }

    fn item(id: i64, inventory: &str, status: ItemStatus) -> ItemView {
        ItemView {
            id: ItemId(id),
            barcode: None,
            inventory_number: Some(inventory.to_owned()),
            home_branch: "MAIN".to_owned(),
            item_type: "BK".to_owned(),
            call_number: None,
            location: None,
            status,
        }
    }

    #[test]
    fn pills_show_available_items_first_then_by_inventory_number() {
        let lost = ItemStatus {
            lost: 1,
            ..ItemStatus::default()
        };
        let ok = ItemStatus::default();
        let instance = FamilyInstance {
            id: InstanceId(1),
            version: 1,
            title: "T".to_owned(),
            edition: None,
            publisher: None,
            date: None,
            isbn: None,
            material_type: None,
            items: vec![
                item(1, "0005", lost),
                item(2, "0004", ok),
                item(3, "0001", lost),
                item(4, "0003", ok),
                item(5, "0002", ok),
                item(6, "0006", ok),
            ],
        };
        let row = instance_row(instance);
        let ids: Vec<i64> = row.pills.iter().map(|pill| pill.id.0).collect();
        assert_eq!(ids, [5, 4, 2, 6]);
        assert_eq!((row.items, row.available, row.more_items), (6, 4, 2));
    }

    struct Fake(Vec<Family>);

    impl CatalogListing for Fake {
        fn families_after(
            &self,
            after: Option<&FamilyCursor>,
            limit: NonZeroUsize,
        ) -> Result<Vec<Family>, CatalogReadError> {
            Ok(self
                .0
                .iter()
                .filter(|family| after.is_none_or(|after| &family.cursor > after))
                .take(limit.get())
                .cloned()
                .collect())
        }
    }

    fn family(n: i64) -> Family {
        Family {
            cursor: FamilyCursor {
                sort_key: format!("{n:03}"),
                head: FamilyHead::Work(WorkId(n)),
            },
            hub_title: None,
            works: Vec::new(),
        }
    }

    #[test]
    fn pages_follow_each_other_without_gaps_or_repeats() {
        let fake = Fake((1..=65).map(family).collect());
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = browse(&fake, after.as_ref()).unwrap();
            assert!(page.rows.len() <= PAGE_SIZE.get());
            seen.extend(page.rows.iter().map(|row| row.cursor.head));
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        let expected: Vec<FamilyHead> = (1..=65).map(|n| FamilyHead::Work(WorkId(n))).collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn a_full_last_page_has_no_next_page() {
        let fake = Fake((1..=30).map(family).collect());
        let page = browse(&fake, None).unwrap();
        assert_eq!(page.rows.len(), 30);
        assert_eq!(page.next, None);
    }
}
