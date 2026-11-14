// SPDX-License-Identifier: GPL-3.0-or-later
//! Families for the staff listing (docs/specs/catalogo/listagem.md).
//!
//! A page is read in a fixed number of queries whatever its size: one for the heads, then
//! one each for Hub titles, Works, languages, subjects, Instances and Items of the page.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use elzevir_app::catalog::{CatalogReadError, ItemView};
use elzevir_app::listing::{
    CatalogListing, Family, FamilyCursor, FamilyHead, FamilyInstance, FamilyWork,
};
use elzevir_domain::catalog::{HubId, IdentifierKind, InstanceId, ItemId, WorkId};
use rusqlite::{Connection, params, params_from_iter};

use crate::ElzevirStore;
use crate::catalog::{ReadFailure, material_type, status_at};

impl CatalogListing for ElzevirStore {
    fn families_after(
        &self,
        after: Option<&FamilyCursor>,
        limit: NonZeroUsize,
    ) -> Result<Vec<Family>, CatalogReadError> {
        self.readers()
            .read(|c| families_after(c, after, limit))
            .map_err(|e| CatalogReadError(e.to_string()))
    }
}

fn families_after(
    c: &Connection,
    after: Option<&FamilyCursor>,
    limit: NonZeroUsize,
) -> Result<Vec<Family>, ReadFailure> {
    let heads = heads_after(c, after, limit)?;
    let hub_ids: Vec<i64> = heads
        .iter()
        .filter_map(|cursor| match cursor.head {
            FamilyHead::Hub(hub) => Some(hub.0),
            FamilyHead::Work(_) => None,
        })
        .collect();
    let hub_titles = hub_titles(c, &hub_ids)?;
    let mut works = works_of(c, &heads, &hub_ids)?;
    let mut families = Vec::with_capacity(heads.len());
    for cursor in heads {
        let (hub_title, members) = match cursor.head {
            FamilyHead::Hub(hub) => (
                hub_titles.get(&hub.0).cloned(),
                works.remove(&Owner::Hub(hub.0)).unwrap_or_default(),
            ),
            FamilyHead::Work(work) => {
                (None, works.remove(&Owner::Work(work.0)).unwrap_or_default())
            }
        };
        families.push(Family {
            cursor,
            hub_title,
            works: members,
        });
    }
    Ok(families)
}

/// The next `limit` heads: Hubs and Works outside a Hub, merged in listing order. Each side
/// reads at most `limit` rows from its own index.
fn heads_after(
    c: &Connection,
    after: Option<&FamilyCursor>,
    limit: NonZeroUsize,
) -> Result<Vec<FamilyCursor>, ReadFailure> {
    // Hubs come before Works with the same key, so after a Work no Hub of its key is left,
    // and after a Hub every Work of its key still is.
    let (key, hub_after, work_after) = match after {
        None => ("", i64::MIN, i64::MIN),
        Some(cursor) => match cursor.head {
            FamilyHead::Hub(hub) => (cursor.sort_key.as_str(), hub.0, i64::MIN),
            FamilyHead::Work(work) => (cursor.sort_key.as_str(), i64::MAX, work.0),
        },
    };
    let limit = i64::try_from(limit.get()).unwrap_or(i64::MAX);
    let mut statement = c.prepare_cached(
        "SELECT kind, id, sort_key FROM (
             SELECT * FROM (
                 SELECT 0 AS kind, id, sort_key FROM hubs
                 WHERE (sort_key, id) > (?1, ?2) ORDER BY sort_key, id LIMIT ?4)
             UNION ALL
             SELECT * FROM (
                 SELECT 1 AS kind, id, sort_key FROM works
                 WHERE hub_id IS NULL AND (sort_key, id) > (?1, ?3) ORDER BY sort_key, id
                 LIMIT ?4))
         ORDER BY sort_key, kind, id LIMIT ?4",
    )?;
    let heads = statement
        .query_map(params![key, hub_after, work_after, limit], |row| {
            let id: i64 = row.get(1)?;
            let head = if row.get::<_, i64>(0)? == 0 {
                FamilyHead::Hub(HubId(id))
            } else {
                FamilyHead::Work(WorkId(id))
            };
            Ok(FamilyCursor {
                sort_key: row.get(2)?,
                head,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(heads)
}

/// `?, ?, ?` for `n` values; the values themselves are always bound as parameters.
fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

fn hub_titles(c: &Connection, hubs: &[i64]) -> Result<BTreeMap<i64, String>, ReadFailure> {
    if hubs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let sql = format!(
        "SELECT id, title FROM hubs WHERE id IN ({})",
        placeholders(hubs.len())
    );
    let mut statement = c.prepare(&sql)?;
    let titles = statement
        .query_map(params_from_iter(hubs), |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(titles)
}

/// Whose family a Work belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Hub(i64),
    Work(i64),
}

/// The Works of the page, complete with languages, subjects, Instances and Items, by
/// family.
fn works_of(
    c: &Connection,
    heads: &[FamilyCursor],
    hubs: &[i64],
) -> Result<BTreeMap<Owner, Vec<FamilyWork>>, ReadFailure> {
    let lone: Vec<i64> = heads
        .iter()
        .filter_map(|cursor| match cursor.head {
            FamilyHead::Work(work) => Some(work.0),
            FamilyHead::Hub(_) => None,
        })
        .collect();
    let mut rows = Vec::new();
    for (column, ids) in [("id", &lone), ("hub_id", &hubs.to_vec())] {
        if ids.is_empty() {
            continue;
        }
        // `column` is one of two constants above, never input.
        let sql = format!(
            "SELECT id, hub_id, title, classification,
                    (SELECT agents.heading FROM contributions
                     JOIN agents ON agents.id = contributions.agent_id
                     WHERE contributions.work_id = works.id
                     ORDER BY is_primary DESC, position LIMIT 1)
             FROM works WHERE {column} IN ({}) ORDER BY id",
            placeholders(ids.len())
        );
        let mut statement = c.prepare(&sql)?;
        let found = statement
            .query_map(params_from_iter(ids.iter()), |row| {
                let id: i64 = row.get(0)?;
                let hub: Option<i64> = row.get(1)?;
                Ok((
                    hub.map_or(Owner::Work(id), Owner::Hub),
                    FamilyWork {
                        id: WorkId(id),
                        title: row.get(2)?,
                        classification: row.get(3)?,
                        main_contributor: row.get(4)?,
                        languages: Vec::new(),
                        subjects: Vec::new(),
                        instances: Vec::new(),
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.extend(found);
    }
    let ids: Vec<i64> = rows.iter().map(|(_, work)| work.id.0).collect();
    let mut languages = by_owner(
        c,
        "SELECT work_id, terms.code FROM work_languages
         JOIN terms ON terms.id = work_languages.language
         WHERE work_id IN ({}) ORDER BY work_id, position",
        &ids,
    )?;
    let mut subjects = by_owner(
        c,
        "SELECT work_id, subjects.heading FROM work_subjects
         JOIN subjects ON subjects.id = work_subjects.subject_id
         WHERE work_id IN ({}) ORDER BY work_id, position",
        &ids,
    )?;
    let mut instances = instances_of(c, &ids)?;
    let mut families: BTreeMap<Owner, Vec<FamilyWork>> = BTreeMap::new();
    for (owner, mut work) in rows {
        work.languages = languages.remove(&work.id.0).unwrap_or_default();
        work.subjects = subjects.remove(&work.id.0).unwrap_or_default();
        work.instances = instances.remove(&work.id.0).unwrap_or_default();
        families.entry(owner).or_default().push(work);
    }
    Ok(families)
}

/// Text values grouped by their owner id, in query order. `sql` has one `{}` for the id
/// placeholders and selects the owner id, then the value.
fn by_owner(
    c: &Connection,
    sql: &str,
    ids: &[i64],
) -> Result<BTreeMap<i64, Vec<String>>, ReadFailure> {
    let mut grouped: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    if ids.is_empty() {
        return Ok(grouped);
    }
    let sql = sql.replace("{}", &placeholders(ids.len()));
    let mut statement = c.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(ids))?;
    while let Some(row) = rows.next()? {
        grouped.entry(row.get(0)?).or_default().push(row.get(1)?);
    }
    Ok(grouped)
}

/// The Instances of these Works with their Items, by Work, in id order.
fn instances_of(
    c: &Connection,
    works: &[i64],
) -> Result<BTreeMap<i64, Vec<FamilyInstance>>, ReadFailure> {
    let mut grouped: BTreeMap<i64, Vec<FamilyInstance>> = BTreeMap::new();
    if works.is_empty() {
        return Ok(grouped);
    }
    let sql = format!(
        "SELECT work_id, id, row_version, title, edition, publisher, date, material_type,
                (SELECT value FROM instance_identifiers
                 WHERE instance_id = instances.id AND kind = ?
                 ORDER BY position LIMIT 1)
         FROM instances WHERE work_id IN ({}) ORDER BY id",
        placeholders(works.len())
    );
    let isbn = i64::from(IdentifierKind::Isbn.code());
    let parameters = std::iter::once(&isbn).chain(works);
    let mut statement = c.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(parameters))?;
    let mut found = Vec::new();
    while let Some(row) = rows.next()? {
        found.push((
            row.get::<_, i64>(0)?,
            FamilyInstance {
                id: InstanceId(row.get(1)?),
                version: row.get(2)?,
                title: row.get(3)?,
                edition: row.get(4)?,
                publisher: row.get(5)?,
                date: row.get(6)?,
                material_type: material_type(row.get(7)?)?,
                isbn: row.get(8)?,
                items: Vec::new(),
            },
        ));
    }
    let ids: Vec<i64> = found.iter().map(|(_, instance)| instance.id.0).collect();
    let mut items = items_of(c, &ids)?;
    for (work, mut instance) in found {
        instance.items = items.remove(&instance.id.0).unwrap_or_default();
        grouped.entry(work).or_default().push(instance);
    }
    Ok(grouped)
}

fn items_of(
    c: &Connection,
    instances: &[i64],
) -> Result<BTreeMap<i64, Vec<ItemView>>, ReadFailure> {
    let mut grouped: BTreeMap<i64, Vec<ItemView>> = BTreeMap::new();
    if instances.is_empty() {
        return Ok(grouped);
    }
    let sql = format!(
        "SELECT items.instance_id, items.id, barcode, inventory_number, branches.code,
                item_types.code, call_number, terms.code, not_for_loan, damaged, lost,
                withdrawn, restricted
         FROM items
         JOIN branches ON branches.id = items.home_branch
         JOIN item_types ON item_types.id = items.item_type
         LEFT JOIN terms ON terms.id = items.location
         WHERE items.instance_id IN ({}) ORDER BY items.id",
        placeholders(instances.len())
    );
    let mut statement = c.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(instances))?;
    while let Some(row) = rows.next()? {
        grouped.entry(row.get(0)?).or_default().push(ItemView {
            id: ItemId(row.get(1)?),
            barcode: row.get(2)?,
            inventory_number: row.get(3)?,
            home_branch: row.get(4)?,
            item_type: row.get(5)?,
            call_number: row.get(6)?,
            location: row.get(7)?,
            status: status_at(row, 8)?,
        });
    }
    Ok(grouped)
}
