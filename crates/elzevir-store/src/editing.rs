// SPDX-License-Identifier: GPL-3.0-or-later
//! Editing a stored Work or Instance in place (docs/specs/catalogo/edicao.md).
//!
//! An edit names the version it was read at; if the row moved since, it is refused and
//! nothing is written. The source record is kept: the export starts from it and rewrites
//! only the fields the model maps, so fields nobody edited go out as they came in.

use elzevir_app::catalog::{
    CatalogOutput, CatalogRefusal, InstanceChange, InstanceUpdate, WorkChange, WorkUpdate,
};
use elzevir_app::commands::WriteRefusal;
use elzevir_app::listing::sort_key;
use elzevir_app::matching::identifier_key;
use elzevir_domain::catalog::{InstanceId, MatchRule, Vocabulary, WorkId};
use rusqlite::{Connection, OptionalExtension, ToSql, params};

use crate::catalog::{
    insert_identifiers, insert_notes, link_contributions, link_languages, link_subjects,
    optional_term, record_changed, refused,
};
use crate::{CommandFailed, StoreError, WriteTx};

type Edited = Result<CatalogOutput, CommandFailed<WriteRefusal>>;

/// Applies a Work edit read at `update.row_version`. Every Instance of the Work gets a
/// new version and is reindexed, since their pages and exports show the Work.
pub(crate) fn update_work(tx: &WriteTx<'_>, update: &WorkUpdate) -> Edited {
    let connection = tx.connection();
    let work = update.work;
    let current = version(
        connection,
        "SELECT row_version FROM works WHERE id = ?1",
        work.0,
    )?
    .ok_or_else(|| refused(CatalogRefusal::UnknownWork(work)))?;
    check_version(update.row_version, current)?;
    if update.changes.is_empty() {
        return Ok(CatalogOutput::WorkUpdated {
            work,
            row_version: current,
        });
    }
    for change in &update.changes {
        apply_work_change(connection, work, change)?;
    }
    let row_version = bump(
        connection,
        "UPDATE works SET row_version = row_version + 1 WHERE id = ?1 RETURNING row_version",
        work.0,
    )?;
    for instance in instances_of(connection, work)? {
        record_changed(tx, instance)?;
    }
    Ok(CatalogOutput::WorkUpdated { work, row_version })
}

/// Applies an Instance edit read at `update.row_version`, the version of its record.
pub(crate) fn update_instance(tx: &WriteTx<'_>, update: &InstanceUpdate) -> Edited {
    let connection = tx.connection();
    let instance = update.instance;
    let current = version(
        connection,
        "SELECT row_version FROM instances WHERE id = ?1",
        instance.0,
    )?
    .ok_or_else(|| refused(CatalogRefusal::UnknownInstance(instance)))?;
    check_version(update.row_version, current)?;
    if update.changes.is_empty() {
        return Ok(CatalogOutput::InstanceUpdated {
            instance,
            row_version: current,
        });
    }
    for change in &update.changes {
        apply_instance_change(connection, instance, change)?;
    }
    record_changed(tx, instance)?;
    let row_version = version(
        connection,
        "SELECT row_version FROM instances WHERE id = ?1",
        instance.0,
    )?
    .unwrap_or(current);
    Ok(CatalogOutput::InstanceUpdated {
        instance,
        row_version,
    })
}

fn check_version(read: i64, current: i64) -> Result<(), CommandFailed<WriteRefusal>> {
    if read == current {
        Ok(())
    } else {
        Err(refused(CatalogRefusal::Stale { read, current }))
    }
}

fn version(connection: &Connection, sql: &str, id: i64) -> Result<Option<i64>, StoreError> {
    Ok(connection
        .query_row(sql, params![id], |row| row.get(0))
        .optional()?)
}

fn bump(connection: &Connection, sql: &str, id: i64) -> Result<i64, StoreError> {
    Ok(connection.query_row(sql, params![id], |row| row.get(0))?)
}

/// Runs one `UPDATE ... SET <column> = ?2 WHERE id = ?1`; the statement text is a constant
/// chosen by the caller, never built from the edit.
fn set(connection: &Connection, sql: &str, id: i64, value: &dyn ToSql) -> Result<(), StoreError> {
    connection.execute(sql, params![id, value])?;
    Ok(())
}

fn apply_work_change(
    connection: &Connection,
    work: WorkId,
    change: &WorkChange,
) -> Result<(), CommandFailed<WriteRefusal>> {
    let id = work.0;
    match change {
        WorkChange::Title(title) => {
            connection
                .execute(
                    "UPDATE works SET title = ?2, sort_key = ?3 WHERE id = ?1",
                    params![id, title, sort_key(title)],
                )
                .map_err(StoreError::from)?;
        }
        WorkChange::MaterialType(kind) => set(
            connection,
            "UPDATE works SET material_type = ?2 WHERE id = ?1",
            id,
            &kind.code(),
        )?,
        WorkChange::Classification { number, scheme } => {
            let scheme = optional_term(
                connection,
                Vocabulary::ClassificationScheme,
                scheme.as_deref(),
            )?;
            connection
                .execute(
                    "UPDATE works SET classification = ?2, classification_scheme = ?3
                     WHERE id = ?1",
                    params![id, number, scheme],
                )
                .map_err(StoreError::from)?;
        }
        WorkChange::Summary(summary) => set(
            connection,
            "UPDATE works SET summary = ?2 WHERE id = ?1",
            id,
            summary,
        )?,
        WorkChange::Audience(audience) => set(
            connection,
            "UPDATE works SET audience = ?2 WHERE id = ?1",
            id,
            audience,
        )?,
        WorkChange::Languages(languages) => {
            clear(
                connection,
                "DELETE FROM work_languages WHERE work_id = ?1",
                id,
            )?;
            link_languages(connection, id, languages)?;
        }
        WorkChange::Contributions(contributions) => {
            clear(
                connection,
                "DELETE FROM contributions WHERE work_id = ?1",
                id,
            )?;
            link_contributions(connection, id, contributions)?;
        }
        WorkChange::Subjects(subjects) => {
            clear(
                connection,
                "DELETE FROM work_subjects WHERE work_id = ?1",
                id,
            )?;
            link_subjects(connection, id, subjects)?;
        }
    }
    Ok(())
}

fn apply_instance_change(
    connection: &Connection,
    instance: InstanceId,
    change: &InstanceChange,
) -> Result<(), StoreError> {
    let id = instance.0;
    let (sql, value): (&str, &dyn ToSql) = match change {
        InstanceChange::Title(title) => ("UPDATE instances SET title = ?2 WHERE id = ?1", title),
        InstanceChange::MaterialType(kind) => {
            return set(
                connection,
                "UPDATE instances SET material_type = ?2 WHERE id = ?1",
                id,
                &kind.code(),
            );
        }
        InstanceChange::Edition(edition) => {
            ("UPDATE instances SET edition = ?2 WHERE id = ?1", edition)
        }
        InstanceChange::Place(place) => ("UPDATE instances SET place = ?2 WHERE id = ?1", place),
        InstanceChange::Publisher(publisher) => (
            "UPDATE instances SET publisher = ?2 WHERE id = ?1",
            publisher,
        ),
        InstanceChange::Date(date) => ("UPDATE instances SET date = ?2 WHERE id = ?1", date),
        InstanceChange::Extent(extent) => {
            ("UPDATE instances SET extent = ?2 WHERE id = ?1", extent)
        }
        InstanceChange::Series(series) => {
            ("UPDATE instances SET series = ?2 WHERE id = ?1", series)
        }
        InstanceChange::Identifiers(identifiers) => {
            clear(
                connection,
                "DELETE FROM instance_identifiers WHERE instance_id = ?1",
                id,
            )?;
            insert_identifiers(connection, id, identifiers)?;
            return refresh_identifier_keys(connection, instance, identifiers);
        }
        InstanceChange::Notes(notes) => {
            clear(
                connection,
                "DELETE FROM instance_notes WHERE instance_id = ?1",
                id,
            )?;
            return insert_notes(connection, id, notes);
        }
    };
    set(connection, sql, id, value)
}

/// The ISBN and ISSN match keys follow the identifiers; the Koha `biblionumber` key comes
/// from the source record, which an edit keeps, so it stays.
fn refresh_identifier_keys(
    connection: &Connection,
    instance: InstanceId,
    identifiers: &[elzevir_app::catalog::NewIdentifier],
) -> Result<(), StoreError> {
    connection.execute(
        "DELETE FROM match_keys WHERE instance_id = ?1 AND rule <> ?2",
        params![instance.0, MatchRule::KohaBiblionumber.code()],
    )?;
    for key in identifiers
        .iter()
        .filter_map(|identifier| identifier_key(identifier.kind, &identifier.value))
    {
        connection.execute(
            "INSERT OR IGNORE INTO match_keys (rule, key, instance_id) VALUES (?1, ?2, ?3)",
            params![key.rule.code(), key.key, instance.0],
        )?;
    }
    Ok(())
}

fn clear(connection: &Connection, sql: &str, id: i64) -> Result<(), StoreError> {
    connection.execute(sql, params![id])?;
    Ok(())
}

fn instances_of(connection: &Connection, work: WorkId) -> Result<Vec<InstanceId>, StoreError> {
    let mut statement =
        connection.prepare_cached("SELECT id FROM instances WHERE work_id = ?1 ORDER BY id")?;
    let ids = statement
        .query_map(params![work.0], |row| row.get(0).map(InstanceId))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
