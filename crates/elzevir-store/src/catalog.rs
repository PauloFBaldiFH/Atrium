// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog commands and reads on SQLite (docs/specs/catalogo/modelo-bibliografico.md).

use elzevir_app::catalog::{
    CatalogCommand, CatalogOutput, CatalogReadError, CatalogReader, CatalogRefusal, InstanceView,
    ItemData, ItemView, NewContribution, NewInstance, NewRecord, NewSubject, NewWorkRelation,
};
use elzevir_app::commands::{WriteOutput, WriteRefusal};
use elzevir_app::search::REINDEX_INSTANCE;
use elzevir_domain::catalog::{
    AgentKind, BranchId, HubId, IdentifierKind, InstanceId, ItemId, ItemTypeId, MaterialType,
    SourceFormat, SubjectKind, Vocabulary, WorkId, WorkRelationKind,
};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::{CommandFailed, ElzevirStore, StoreError, WriteTx};

type Applied = Result<WriteOutput, CommandFailed<WriteRefusal>>;

/// Applies one catalog command inside the writer's savepoint.
pub(crate) fn apply(tx: &WriteTx<'_>, command: &CatalogCommand) -> Applied {
    let connection = tx.connection();
    let output = match command {
        CatalogCommand::CreateBranch { code, name } => {
            insert_coded(
                connection,
                "INSERT INTO branches (code, name) VALUES (?1, ?2)",
                code,
                name,
            )?;
            CatalogOutput::Branch(BranchId(connection.last_insert_rowid()))
        }
        CatalogCommand::CreateItemType { code, description } => {
            insert_coded(
                connection,
                "INSERT INTO item_types (code, description) VALUES (?1, ?2)",
                code,
                description,
            )?;
            CatalogOutput::ItemType(ItemTypeId(connection.last_insert_rowid()))
        }
        CatalogCommand::CreateHub { title } => {
            connection
                .execute("INSERT INTO hubs (title) VALUES (?1)", params![title])
                .map_err(StoreError::from)?;
            CatalogOutput::Hub(HubId(connection.last_insert_rowid()))
        }
        CatalogCommand::AddRecord(record) => {
            let output = add_record(connection, record)?;
            if let CatalogOutput::Record { instance, .. } = &output {
                reindex(tx, *instance)?;
            }
            output
        }
        CatalogCommand::AddItem(new) => {
            if !exists(
                connection,
                "SELECT 1 FROM instances WHERE id = ?1",
                new.instance.0,
            )? {
                return Err(refused(CatalogRefusal::UnknownInstance(new.instance)));
            }
            let item = add_item(connection, new.instance, &new.item)?;
            reindex(tx, new.instance)?;
            CatalogOutput::Item(item)
        }
    };
    Ok(WriteOutput::Catalog(output))
}

fn refused(refusal: CatalogRefusal) -> CommandFailed<WriteRefusal> {
    CommandFailed::Rejected(WriteRefusal::Catalog(refusal))
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.code == ErrorCode::ConstraintViolation)
}

fn insert_coded(
    connection: &Connection,
    sql: &str,
    code: &str,
    text: &str,
) -> Result<(), CommandFailed<WriteRefusal>> {
    match connection.execute(sql, params![code, text]) {
        Ok(_) => Ok(()),
        Err(e) if is_constraint(&e) => Err(refused(CatalogRefusal::CodeTaken(code.to_owned()))),
        Err(e) => Err(StoreError::from(e).into()),
    }
}

fn add_record(
    connection: &Connection,
    record: &NewRecord,
) -> Result<CatalogOutput, CommandFailed<WriteRefusal>> {
    if let Some(hub) = record.hub
        && !exists(connection, "SELECT 1 FROM hubs WHERE id = ?1", hub.0)?
    {
        return Err(refused(CatalogRefusal::UnknownHub(hub)));
    }
    let scheme = optional_term(
        connection,
        Vocabulary::ClassificationScheme,
        record.classification_scheme.as_deref(),
    )?;
    connection
        .execute(
            "INSERT INTO works (title, material_type, hub_id, classification,
                                classification_scheme, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.work_title,
                record.material_type.code(),
                record.hub.map(|hub| hub.0),
                record.classification,
                scheme,
                record.summary
            ],
        )
        .map_err(StoreError::from)?;
    let work = connection.last_insert_rowid();
    link_relations(connection, work, &record.relations)?;
    link_languages(connection, work, &record.languages)?;
    link_contributions(connection, work, &record.contributions)?;
    link_subjects(connection, work, &record.subjects)?;
    let instance = add_instance(connection, work, record.material_type, &record.instance)?;
    let instance = InstanceId(instance);
    if let Some(source) = &record.source {
        connection
            .execute(
                "INSERT INTO source_records (instance_id, format, record) VALUES (?1, ?2, ?3)",
                params![instance.0, SourceFormat::Marc21Iso2709.code(), source],
            )
            .map_err(StoreError::from)?;
    }
    let items = record
        .items
        .iter()
        .map(|item| add_item(connection, instance, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CatalogOutput::Record {
        work: WorkId(work),
        instance,
        items,
    })
}

/// Queues the Instance for reindexing once this command commits (docs/specs/catalogo/modelo-bibliografico.md, item 24).
fn reindex(tx: &WriteTx<'_>, instance: InstanceId) -> Result<(), StoreError> {
    tx.emit_effect(
        REINDEX_INSTANCE,
        &serde_json::json!({ "instance": instance.0 }),
    )
}

fn exists(connection: &Connection, sql: &str, id: i64) -> Result<bool, StoreError> {
    Ok(connection
        .query_row(sql, params![id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn link_relations(
    connection: &Connection,
    work: i64,
    relations: &[NewWorkRelation],
) -> Result<(), CommandFailed<WriteRefusal>> {
    for (relation, position) in relations.iter().zip(0_i64..) {
        if !exists(
            connection,
            "SELECT 1 FROM works WHERE id = ?1",
            relation.work.0,
        )? {
            return Err(refused(CatalogRefusal::UnknownWork(relation.work)));
        }
        connection
            .execute(
                "INSERT INTO work_relations (work_id, position, kind, related_work_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![work, position, relation.kind.code(), relation.work.0],
            )
            .map_err(StoreError::from)?;
    }
    Ok(())
}

fn link_languages(
    connection: &Connection,
    work: i64,
    languages: &[String],
) -> Result<(), CommandFailed<WriteRefusal>> {
    for (code, position) in languages.iter().zip(0_i64..) {
        let language = term(connection, Vocabulary::Language, code)?;
        connection
            .execute(
                "INSERT INTO work_languages (work_id, position, language) VALUES (?1, ?2, ?3)",
                params![work, position, language],
            )
            .map_err(StoreError::from)?;
    }
    Ok(())
}

fn link_contributions(
    connection: &Connection,
    work: i64,
    contributions: &[NewContribution],
) -> Result<(), CommandFailed<WriteRefusal>> {
    for (contribution, position) in contributions.iter().zip(0_i64..) {
        let agent = upsert(
            connection,
            "INSERT INTO agents (kind, heading) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            "SELECT id FROM agents WHERE kind = ?1 AND heading = ?2",
            contribution.kind.code(),
            &contribution.heading,
        )?;
        let role = optional_term(
            connection,
            Vocabulary::Relator,
            contribution.role.as_deref(),
        )?;
        connection
            .execute(
                "INSERT INTO contributions (work_id, position, agent_id, role, is_primary)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![work, position, agent, role, contribution.primary],
            )
            .map_err(StoreError::from)?;
    }
    Ok(())
}

fn link_subjects(
    connection: &Connection,
    work: i64,
    subjects: &[NewSubject],
) -> Result<(), CommandFailed<WriteRefusal>> {
    for (subject, position) in subjects.iter().zip(0_i64..) {
        let thesaurus = optional_term(
            connection,
            Vocabulary::Thesaurus,
            subject.thesaurus.as_deref(),
        )?;
        connection
            .execute(
                "INSERT INTO subjects (kind, heading, thesaurus) VALUES (?1, ?2, ?3)
                 ON CONFLICT DO NOTHING",
                params![subject.kind.code(), subject.heading, thesaurus],
            )
            .map_err(StoreError::from)?;
        let subject_id: i64 = connection
            .query_row(
                "SELECT id FROM subjects
                 WHERE kind = ?1 AND heading = ?2 AND coalesce(thesaurus, 0) = coalesce(?3, 0)",
                params![subject.kind.code(), subject.heading, thesaurus],
                |row| row.get(0),
            )
            .map_err(StoreError::from)?;
        connection
            .execute(
                "INSERT INTO work_subjects (work_id, position, subject_id) VALUES (?1, ?2, ?3)",
                params![work, position, subject_id],
            )
            .map_err(StoreError::from)?;
    }
    Ok(())
}

fn add_instance(
    connection: &Connection,
    work: i64,
    work_material_type: MaterialType,
    instance: &NewInstance,
) -> Result<i64, CommandFailed<WriteRefusal>> {
    let material_type = instance.material_type.unwrap_or(work_material_type);
    connection
        .execute(
            "INSERT INTO instances (work_id, title, material_type, edition, place, publisher, date,
                                    extent, series)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                work,
                instance.title,
                material_type.code(),
                instance.edition,
                instance.place,
                instance.publisher,
                instance.date,
                instance.extent,
                instance.series
            ],
        )
        .map_err(StoreError::from)?;
    let instance_id = connection.last_insert_rowid();
    for (identifier, position) in instance.identifiers.iter().zip(0_i64..) {
        connection
            .execute(
                "INSERT INTO instance_identifiers (instance_id, position, kind, value, qualifier)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    instance_id,
                    position,
                    identifier.kind.code(),
                    identifier.value,
                    identifier.qualifier
                ],
            )
            .map_err(StoreError::from)?;
    }
    Ok(instance_id)
}

fn add_item(
    connection: &Connection,
    instance: InstanceId,
    item: &ItemData,
) -> Result<ItemId, CommandFailed<WriteRefusal>> {
    let branch = |code: &str| {
        lookup(connection, "SELECT id FROM branches WHERE code = ?1", code)?
            .ok_or_else(|| refused(CatalogRefusal::UnknownBranch(code.to_owned())))
    };
    let home = branch(&item.home_branch)?;
    let holding = branch(&item.holding_branch)?;
    let item_type = lookup(
        connection,
        "SELECT id FROM item_types WHERE code = ?1",
        &item.item_type,
    )?
    .ok_or_else(|| refused(CatalogRefusal::UnknownItemType(item.item_type.clone())))?;
    let location = optional_term(connection, Vocabulary::Location, item.location.as_deref())?;
    let collection = optional_term(
        connection,
        Vocabulary::Collection,
        item.collection.as_deref(),
    )?;
    let inserted = connection.execute(
        "INSERT INTO items (instance_id, barcode, inventory_number, home_branch, holding_branch,
                            item_type, call_number, location, collection, price_cents,
                            replacement_price_cents, copy_number, not_for_loan, damaged, lost,
                            withdrawn, restricted, acquired_on)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            instance.0,
            item.barcode,
            item.inventory_number,
            home,
            holding,
            item_type,
            item.call_number,
            location,
            collection,
            item.price.map(|m| m.0),
            item.replacement_price.map(|m| m.0),
            item.copy_number,
            item.status.not_for_loan,
            item.status.damaged,
            item.status.lost,
            item.status.withdrawn,
            item.status.restricted,
            item.acquired_on
        ],
    );
    match inserted {
        Ok(_) => Ok(ItemId(connection.last_insert_rowid())),
        Err(e) if is_constraint(&e) => Err(refused(CatalogRefusal::BarcodeTaken(
            item.barcode.clone().unwrap_or_default(),
        ))),
        Err(e) => Err(StoreError::from(e).into()),
    }
}

fn lookup(connection: &Connection, sql: &str, code: &str) -> Result<Option<i64>, StoreError> {
    Ok(connection
        .query_row(sql, params![code], |row| row.get(0))
        .optional()?)
}

/// Inserts `(number, text)` if missing and returns the row's id.
fn upsert(
    connection: &Connection,
    insert: &str,
    select: &str,
    number: u8,
    text: &str,
) -> Result<i64, StoreError> {
    connection.execute(insert, params![number, text])?;
    Ok(connection.query_row(select, params![number, text], |row| row.get(0))?)
}

/// The term with `code` in `vocabulary`, created with the code as its label if missing.
fn term(connection: &Connection, vocabulary: Vocabulary, code: &str) -> Result<i64, StoreError> {
    connection.execute(
        "INSERT INTO terms (vocabulary, code, label) VALUES (?1, ?2, ?2) ON CONFLICT DO NOTHING",
        params![vocabulary.code(), code],
    )?;
    Ok(connection.query_row(
        "SELECT id FROM terms WHERE vocabulary = ?1 AND code = ?2",
        params![vocabulary.code(), code],
        |row| row.get(0),
    )?)
}

fn optional_term(
    connection: &Connection,
    vocabulary: Vocabulary,
    code: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    code.map(|code| term(connection, vocabulary, code))
        .transpose()
}

impl CatalogReader for ElzevirStore {
    fn instance(&self, id: InstanceId) -> Result<Option<InstanceView>, CatalogReadError> {
        self.readers()
            .read(|c| read_instance(c, id))
            .map_err(|e| CatalogReadError(e.to_string()))
    }

    fn items(&self, instance: InstanceId) -> Result<Vec<ItemView>, CatalogReadError> {
        self.readers()
            .read(|c| {
                let mut statement = c.prepare(
                    "SELECT items.id, barcode, inventory_number, branches.code, item_types.code,
                            call_number, terms.code
                     FROM items
                     JOIN branches ON branches.id = items.home_branch
                     JOIN item_types ON item_types.id = items.item_type
                     LEFT JOIN terms ON terms.id = items.location
                     WHERE instance_id = ?1 ORDER BY items.id",
                )?;
                statement
                    .query_map(params![instance.0], |row| {
                        Ok(ItemView {
                            id: ItemId(row.get(0)?),
                            barcode: row.get(1)?,
                            inventory_number: row.get(2)?,
                            home_branch: row.get(3)?,
                            item_type: row.get(4)?,
                            call_number: row.get(5)?,
                            location: row.get(6)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e: rusqlite::Error| CatalogReadError(e.to_string()))
    }
}

/// Why reading an Instance failed.
#[derive(Debug, thiserror::Error)]
enum ReadFailure {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error("unknown {what} number {code} in the database (newer schema?)")]
    UnknownCode { what: &'static str, code: u8 },
}

fn read_instance(c: &Connection, id: InstanceId) -> Result<Option<InstanceView>, ReadFailure> {
    let Some(row) = c
        .query_row(
            "SELECT works.id, works.title, instances.title, publisher, date, works.hub_id,
                    works.material_type, instances.material_type, works.classification,
                    works.summary
             FROM instances JOIN works ON works.id = instances.work_id
             WHERE instances.id = ?1",
            params![id.0],
            |row| {
                Ok(InstanceRow {
                    work: row.get(0)?,
                    work_title: row.get(1)?,
                    title: row.get(2)?,
                    publisher: row.get(3)?,
                    date: row.get(4)?,
                    hub: row.get(5)?,
                    work_material_type: row.get(6)?,
                    material_type: row.get(7)?,
                    classification: row.get(8)?,
                    summary: row.get(9)?,
                })
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    let work = row.work;
    let contributors = coded_list(
        c,
        "SELECT agents.kind, agents.heading FROM contributions
         JOIN agents ON agents.id = contributions.agent_id
         WHERE work_id = ?1 ORDER BY position",
        work,
        "agent kind",
        AgentKind::from_code,
    )?;
    let subjects = coded_list(
        c,
        "SELECT subjects.kind, subjects.heading FROM work_subjects
         JOIN subjects ON subjects.id = work_subjects.subject_id
         WHERE work_id = ?1 ORDER BY position",
        work,
        "subject kind",
        SubjectKind::from_code,
    )?;
    let identifiers = coded_list(
        c,
        "SELECT kind, value FROM instance_identifiers WHERE instance_id = ?1 ORDER BY position",
        id.0,
        "identifier kind",
        IdentifierKind::from_code,
    )?;
    let relations = work_relations(c, work)?;
    let mut statement = c.prepare(
        "SELECT terms.code FROM work_languages JOIN terms ON terms.id = work_languages.language
         WHERE work_id = ?1 ORDER BY position",
    )?;
    let languages = statement
        .query_map(params![work], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(Some(InstanceView {
        id,
        work: WorkId(work),
        work_title: row.work_title,
        hub: row.hub.map(HubId),
        work_material_type: material_type(row.work_material_type)?,
        material_type: material_type(row.material_type)?,
        relations,
        title: row.title,
        publisher: row.publisher,
        date: row.date,
        classification: row.classification,
        summary: row.summary,
        contributors,
        subjects,
        identifiers,
        languages,
    }))
}

/// The columns of one Instance and its Work.
struct InstanceRow {
    work: i64,
    work_title: String,
    title: String,
    publisher: Option<String>,
    date: Option<String>,
    hub: Option<i64>,
    work_material_type: Option<u8>,
    material_type: Option<u8>,
    classification: Option<String>,
    summary: Option<String>,
}

fn material_type(code: Option<u8>) -> Result<Option<MaterialType>, ReadFailure> {
    code.map(|code| {
        MaterialType::from_code(code).ok_or(ReadFailure::UnknownCode {
            what: "material type",
            code,
        })
    })
    .transpose()
}

fn work_relations(
    c: &Connection,
    work: i64,
) -> Result<Vec<(WorkRelationKind, WorkId)>, ReadFailure> {
    let mut statement = c.prepare(
        "SELECT kind, related_work_id FROM work_relations WHERE work_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map(params![work], |row| {
            Ok((row.get::<_, u8>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(code, related)| {
            WorkRelationKind::from_code(code)
                .map(|kind| (kind, WorkId(related)))
                .ok_or(ReadFailure::UnknownCode {
                    what: "work relation kind",
                    code,
                })
        })
        .collect()
}

/// Rows of `(stable number, text)` for one parent id, with the number decoded.
fn coded_list<T>(
    c: &Connection,
    sql: &str,
    parent: i64,
    what: &'static str,
    decode: fn(u8) -> Option<T>,
) -> Result<Vec<(T, String)>, ReadFailure> {
    let mut statement = c.prepare(sql)?;
    let rows = statement
        .query_map(params![parent], |row| {
            Ok((row.get::<_, u8>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(code, text)| {
            decode(code)
                .map(|value| (value, text))
                .ok_or(ReadFailure::UnknownCode { what, code })
        })
        .collect()
}
