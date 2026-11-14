// SPDX-License-Identifier: GPL-3.0-or-later
//! The stored records as the export reads them (docs/specs/catalogo/linha-de-comando.md).

use std::num::NonZeroUsize;

use elzevir_app::catalog::{
    CatalogReadError, ItemData, NewContribution, NewIdentifier, NewInstance, NewNote, NewRecord,
    NewSubject, NewWorkRelation,
};
use elzevir_app::export::{RecordSource, StoredRecord};
use elzevir_domain::catalog::{
    AgentKind, HubId, IdentifierKind, InstanceId, MaterialType, Money, NoteKind, SubjectKind,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::ElzevirStore;
use crate::catalog::{ReadFailure, status_at, work_relations};

impl RecordSource for ElzevirStore {
    fn records_after(
        &self,
        after: Option<InstanceId>,
        limit: NonZeroUsize,
    ) -> Result<Vec<StoredRecord>, CatalogReadError> {
        self.readers()
            .read(|c| records_after(c, after, limit))
            .map_err(|e| CatalogReadError(e.to_string()))
    }
}

/// The columns of one Instance and its Work.
struct Head {
    instance: i64,
    work: i64,
    title: String,
    material_type: Option<u8>,
    edition: Option<String>,
    place: Option<String>,
    publisher: Option<String>,
    date: Option<String>,
    extent: Option<String>,
    series: Option<String>,
    work_title: String,
    work_material_type: Option<u8>,
    hub: Option<i64>,
    classification: Option<String>,
    classification_scheme: Option<String>,
    summary: Option<String>,
    audience: Option<String>,
}

impl Head {
    fn from_row(row: &Row<'_>) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            instance: row.get(0)?,
            work: row.get(1)?,
            title: row.get(2)?,
            material_type: row.get(3)?,
            edition: row.get(4)?,
            place: row.get(5)?,
            publisher: row.get(6)?,
            date: row.get(7)?,
            extent: row.get(8)?,
            series: row.get(9)?,
            work_title: row.get(10)?,
            work_material_type: row.get(11)?,
            hub: row.get(12)?,
            classification: row.get(13)?,
            classification_scheme: row.get(14)?,
            summary: row.get(15)?,
            audience: row.get(16)?,
        })
    }
}

fn records_after(
    c: &Connection,
    after: Option<InstanceId>,
    limit: NonZeroUsize,
) -> Result<Vec<StoredRecord>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT instances.id, works.id, instances.title, instances.material_type, edition,
                place, publisher, date, extent, series, works.title, works.material_type,
                works.hub_id, works.classification, terms.code, works.summary, works.audience
         FROM instances
         JOIN works ON works.id = instances.work_id
         LEFT JOIN terms ON terms.id = works.classification_scheme
         WHERE instances.id > ?1
         ORDER BY instances.id
         LIMIT ?2",
    )?;
    let limit = i64::try_from(limit.get()).unwrap_or(i64::MAX);
    let heads = statement
        .query_map(params![after.map_or(0, |id| id.0), limit], Head::from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    heads.into_iter().map(|head| stored(c, head)).collect()
}

fn stored(c: &Connection, head: Head) -> Result<StoredRecord, ReadFailure> {
    // Rows written before categories of material existed read as books, the mapping's
    // own default.
    let work_material = decode(
        head.work_material_type,
        "material type",
        MaterialType::from_code,
    )?
    .unwrap_or(MaterialType::Book);
    let material = decode(head.material_type, "material type", MaterialType::from_code)?;
    let relations = work_relations(c, head.work)?
        .into_iter()
        .map(|(kind, work)| NewWorkRelation { kind, work })
        .collect();
    let record = NewRecord {
        work_title: head.work_title,
        material_type: work_material,
        hub: head.hub.map(HubId),
        relations,
        classification: head.classification,
        classification_scheme: head.classification_scheme,
        summary: head.summary,
        audience: head.audience,
        languages: languages(c, head.work)?,
        contributions: contributions(c, head.work)?,
        subjects: subjects(c, head.work)?,
        instance: NewInstance {
            title: head.title,
            // The model keeps the Instance's category only where it differs from the Work's.
            material_type: material.filter(|material| *material != work_material),
            edition: head.edition,
            place: head.place,
            publisher: head.publisher,
            date: head.date,
            extent: head.extent,
            series: head.series,
            identifiers: identifiers(c, head.instance)?,
            notes: notes(c, head.instance)?,
        },
        items: items(c, head.instance)?,
        source: c
            .query_row(
                "SELECT record FROM source_records WHERE instance_id = ?1",
                params![head.instance],
                |row| row.get(0),
            )
            .optional()?,
    };
    Ok(StoredRecord {
        instance: InstanceId(head.instance),
        record,
    })
}

fn decode<T>(
    code: Option<u8>,
    what: &'static str,
    from_code: fn(u8) -> Option<T>,
) -> Result<Option<T>, ReadFailure> {
    code.map(|code| from_code(code).ok_or(ReadFailure::UnknownCode { what, code }))
        .transpose()
}

fn required<T>(
    code: u8,
    what: &'static str,
    from_code: fn(u8) -> Option<T>,
) -> Result<T, ReadFailure> {
    from_code(code).ok_or(ReadFailure::UnknownCode { what, code })
}

fn languages(c: &Connection, work: i64) -> Result<Vec<String>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT terms.code FROM work_languages JOIN terms ON terms.id = work_languages.language
         WHERE work_id = ?1 ORDER BY position",
    )?;
    Ok(statement
        .query_map(params![work], |row| row.get(0))?
        .collect::<Result<_, _>>()?)
}

fn contributions(c: &Connection, work: i64) -> Result<Vec<NewContribution>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT agents.kind, agents.heading, terms.code, is_primary FROM contributions
         JOIN agents ON agents.id = contributions.agent_id
         LEFT JOIN terms ON terms.id = contributions.role
         WHERE work_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map(params![work], |row| {
            Ok((row.get::<_, u8>(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(kind, heading, role, primary)| {
            Ok(NewContribution {
                kind: required(kind, "agent kind", AgentKind::from_code)?,
                heading,
                role,
                primary,
            })
        })
        .collect()
}

fn subjects(c: &Connection, work: i64) -> Result<Vec<NewSubject>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT subjects.kind, subjects.heading, terms.code FROM work_subjects
         JOIN subjects ON subjects.id = work_subjects.subject_id
         LEFT JOIN terms ON terms.id = subjects.thesaurus
         WHERE work_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map(params![work], |row| {
            Ok((row.get::<_, u8>(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(kind, heading, thesaurus)| {
            Ok(NewSubject {
                kind: required(kind, "subject kind", SubjectKind::from_code)?,
                heading,
                thesaurus,
            })
        })
        .collect()
}

fn notes(c: &Connection, instance: i64) -> Result<Vec<NewNote>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT kind, text FROM instance_notes WHERE instance_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map(params![instance], |row| {
            Ok((row.get::<_, u8>(0)?, row.get(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(kind, text)| {
            Ok(NewNote {
                kind: required(kind, "note kind", NoteKind::from_code)?,
                text,
            })
        })
        .collect()
}

fn identifiers(c: &Connection, instance: i64) -> Result<Vec<NewIdentifier>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT kind, value, qualifier FROM instance_identifiers
         WHERE instance_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map(params![instance], |row| {
            Ok((row.get::<_, u8>(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(kind, value, qualifier)| {
            Ok(NewIdentifier {
                kind: required(kind, "identifier kind", IdentifierKind::from_code)?,
                value,
                qualifier,
            })
        })
        .collect()
}

fn items(c: &Connection, instance: i64) -> Result<Vec<ItemData>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT barcode, inventory_number, home.code, holding.code, item_types.code,
                call_number, location.code, collection.code, price_cents,
                replacement_price_cents, copy_number, acquired_on,
                not_for_loan, damaged, lost, withdrawn, restricted
         FROM items
         JOIN branches AS home ON home.id = items.home_branch
         JOIN branches AS holding ON holding.id = items.holding_branch
         JOIN item_types ON item_types.id = items.item_type
         LEFT JOIN terms AS location ON location.id = items.location
         LEFT JOIN terms AS collection ON collection.id = items.collection
         WHERE instance_id = ?1 ORDER BY items.id",
    )?;
    Ok(statement
        .query_map(params![instance], |row| {
            Ok(ItemData {
                barcode: row.get(0)?,
                inventory_number: row.get(1)?,
                home_branch: row.get(2)?,
                holding_branch: row.get(3)?,
                item_type: row.get(4)?,
                call_number: row.get(5)?,
                location: row.get(6)?,
                collection: row.get(7)?,
                price: row.get::<_, Option<i64>>(8)?.map(Money),
                replacement_price: row.get::<_, Option<i64>>(9)?.map(Money),
                copy_number: row.get(10)?,
                acquired_on: row.get(11)?,
                status: status_at(row, 12)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}
