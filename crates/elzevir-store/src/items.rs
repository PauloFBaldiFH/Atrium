// SPDX-License-Identifier: GPL-3.0-or-later
//! An Instance's Items saved as one batch, all or nothing, and read for the grid
//! (docs/specs/catalogo/exemplares.md).

use std::collections::HashMap;

use elzevir_app::catalog::{
    CatalogOutput, CatalogReadError, CatalogRefusal, CodeLists, CodeName, ItemData, ItemStatus,
};
use elzevir_app::commands::WriteRefusal;
use elzevir_app::items::{
    GridItem, ItemColumn, ItemFields, ItemGridSource, ItemRow, ItemsUpdate, RowProblem, RowRefusal,
    repeated_barcodes,
};
use elzevir_domain::catalog::{InstanceId, ItemId, Money, Vocabulary};
use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::{add_item, lookup, optional_term, record_changed, refused, status_at};
use crate::editing::{check_version, version};
use crate::{CommandFailed, ElzevirStore, StoreError, WriteTx};

type Saved = Result<CatalogOutput, CommandFailed<WriteRefusal>>;

/// `catalog.save_items` (items 3 to 9).
pub(crate) fn save_items(tx: &WriteTx<'_>, update: &ItemsUpdate) -> Saved {
    let connection = tx.connection();
    let instance = update.instance;
    let current = version(
        connection,
        "SELECT row_version FROM instances WHERE id = ?1",
        instance.0,
    )?
    .ok_or_else(|| refused(CatalogRefusal::UnknownInstance(instance)))?;
    check_version(update.row_version, current)?;
    if update.rows.is_empty() {
        return Ok(CatalogOutput::ItemsSaved {
            instance,
            row_version: current,
            items: Vec::new(),
        });
    }
    let mut refusals = refusals(connection, instance, &update.rows)?;
    if !refusals.is_empty() {
        refusals.sort_by_key(|refusal| refusal.row);
        return Err(refused(CatalogRefusal::ItemRows(refusals)));
    }
    // Barcodes may move between rows of the batch (item 6): the existing rows let go of
    // theirs first, so no intermediate state breaks the unique index.
    for id in update.rows.iter().filter_map(|row| row.id) {
        connection
            .execute(
                "UPDATE items SET barcode = NULL WHERE id = ?1",
                params![id.0],
            )
            .map_err(StoreError::from)?;
    }
    let mut items = Vec::with_capacity(update.rows.len());
    for row in &update.rows {
        items.push(match row.id {
            Some(id) => {
                update_item(connection, id, &row.fields)?;
                id
            }
            None => add_item(connection, instance, &new_item(&row.fields))?,
        });
    }
    record_changed(tx, instance)?;
    let row_version = version(
        connection,
        "SELECT row_version FROM instances WHERE id = ?1",
        instance.0,
    )?
    .unwrap_or(current + 1);
    Ok(CatalogOutput::ItemsSaved {
        instance,
        row_version,
        items,
    })
}

/// Every cell of the batch the catalog refuses (item 5), found before anything is
/// written.
fn refusals(
    connection: &Connection,
    instance: InstanceId,
    rows: &[ItemRow],
) -> Result<Vec<RowRefusal>, StoreError> {
    let mut refusals = repeated_barcodes(rows);
    let mut owned = Vec::with_capacity(rows.len());
    for row in rows {
        owned.push(match row.id {
            Some(id) => {
                let instance_of_item: Option<i64> = connection
                    .query_row(
                        "SELECT instance_id FROM items WHERE id = ?1",
                        params![id.0],
                        |row| row.get(0),
                    )
                    .optional()?;
                instance_of_item == Some(instance.0)
            }
            None => true,
        });
    }
    // The barcode each existing Item of the batch ends with.
    let moving: HashMap<i64, Option<&str>> = rows
        .iter()
        .zip(&owned)
        .filter(|(_, owned)| **owned)
        .filter_map(|(row, _)| row.id.map(|id| (id.0, row.fields.barcode.as_deref())))
        .collect();
    for (position, (row, owned)) in rows.iter().zip(&owned).enumerate() {
        let mut refuse = |column, problem| {
            refusals.push(RowRefusal {
                row: position,
                column,
                problem,
            });
        };
        if !owned {
            refuse(ItemColumn::Row, RowProblem::UnknownItem);
        }
        let branch = "SELECT id FROM branches WHERE code = ?1";
        if lookup(connection, branch, &row.fields.home_branch)?.is_none() {
            refuse(ItemColumn::HomeBranch, RowProblem::UnknownBranch);
        }
        if lookup(connection, branch, &row.fields.holding_branch)?.is_none() {
            refuse(ItemColumn::HoldingBranch, RowProblem::UnknownBranch);
        }
        let item_type = "SELECT id FROM item_types WHERE code = ?1";
        if lookup(connection, item_type, &row.fields.item_type)?.is_none() {
            refuse(ItemColumn::ItemType, RowProblem::UnknownItemType);
        }
        if let Some(barcode) = &row.fields.barcode {
            let holder = lookup(
                connection,
                "SELECT id FROM items WHERE barcode = ?1",
                barcode,
            )?;
            // Taken, unless by this row's own Item or by an Item of the batch that lets
            // go of it.
            let taken = holder.is_some_and(|holder| {
                Some(ItemId(holder)) != row.id
                    && moving
                        .get(&holder)
                        .is_none_or(|ends_with| *ends_with == Some(barcode.as_str()))
            });
            if taken {
                refuse(ItemColumn::Barcode, RowProblem::BarcodeTaken);
            }
        }
    }
    Ok(refusals)
}

fn update_item(
    connection: &Connection,
    id: ItemId,
    fields: &ItemFields,
) -> Result<(), CommandFailed<WriteRefusal>> {
    let code = |sql: &str, value: &str| -> Result<i64, CommandFailed<WriteRefusal>> {
        // Checked in `refusals` inside the same savepoint, so it is there.
        lookup(connection, sql, value)?
            .ok_or_else(|| refused(CatalogRefusal::UnknownBranch(value.to_owned())))
    };
    let home = code(
        "SELECT id FROM branches WHERE code = ?1",
        &fields.home_branch,
    )?;
    let holding = code(
        "SELECT id FROM branches WHERE code = ?1",
        &fields.holding_branch,
    )?;
    let item_type = code(
        "SELECT id FROM item_types WHERE code = ?1",
        &fields.item_type,
    )?;
    let location = optional_term(connection, Vocabulary::Location, fields.location.as_deref())?;
    let collection = optional_term(
        connection,
        Vocabulary::Collection,
        fields.collection.as_deref(),
    )?;
    connection
        .execute(
            "UPDATE items SET barcode = ?2, inventory_number = ?3, home_branch = ?4,
                              holding_branch = ?5, item_type = ?6, call_number = ?7,
                              location = ?8, collection = ?9, copy_number = ?10,
                              price_cents = ?11, row_version = row_version + 1
             WHERE id = ?1",
            params![
                id.0,
                fields.barcode,
                fields.inventory_number,
                home,
                holding,
                item_type,
                fields.call_number,
                location,
                collection,
                fields.copy_number,
                fields.price.map(|money| money.0)
            ],
        )
        .map_err(StoreError::from)?;
    Ok(())
}

/// A new Item: the grid's columns, normal status and nothing else (item 2).
fn new_item(fields: &ItemFields) -> ItemData {
    ItemData {
        barcode: fields.barcode.clone(),
        inventory_number: fields.inventory_number.clone(),
        home_branch: fields.home_branch.clone(),
        holding_branch: fields.holding_branch.clone(),
        item_type: fields.item_type.clone(),
        call_number: fields.call_number.clone(),
        location: fields.location.clone(),
        collection: fields.collection.clone(),
        price: fields.price,
        replacement_price: None,
        copy_number: fields.copy_number.clone(),
        status: ItemStatus::default(),
        acquired_on: None,
    }
}

impl ItemGridSource for ElzevirStore {
    fn grid_items(&self, instance: InstanceId) -> Result<Vec<GridItem>, CatalogReadError> {
        self.readers()
            .read(|c| {
                let mut statement = c.prepare(
                    "SELECT items.id, barcode, inventory_number, home.code, holding.code,
                            item_types.code, call_number, location.code, collection.code,
                            copy_number, price_cents,
                            not_for_loan, damaged, lost, withdrawn, restricted
                     FROM items
                     JOIN branches AS home ON home.id = items.home_branch
                     JOIN branches AS holding ON holding.id = items.holding_branch
                     JOIN item_types ON item_types.id = items.item_type
                     LEFT JOIN terms AS location ON location.id = items.location
                     LEFT JOIN terms AS collection ON collection.id = items.collection
                     WHERE instance_id = ?1 ORDER BY items.id",
                )?;
                statement
                    .query_map(params![instance.0], |row| {
                        Ok(GridItem {
                            id: ItemId(row.get(0)?),
                            fields: ItemFields {
                                barcode: row.get(1)?,
                                inventory_number: row.get(2)?,
                                home_branch: row.get(3)?,
                                holding_branch: row.get(4)?,
                                item_type: row.get(5)?,
                                call_number: row.get(6)?,
                                location: row.get(7)?,
                                collection: row.get(8)?,
                                copy_number: row.get(9)?,
                                price: row.get::<_, Option<i64>>(10)?.map(Money),
                            },
                            status: status_at(row, 11)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| CatalogReadError(e.to_string()))
    }
}

/// The code tables with their names, in code order (item 29).
pub(crate) fn code_lists(c: &Connection) -> Result<CodeLists, rusqlite::Error> {
    let table = |sql: &str, vocabulary: Option<Vocabulary>| {
        let mut statement = c.prepare(sql)?;
        let entry = |row: &rusqlite::Row<'_>| {
            Ok(CodeName {
                code: row.get(0)?,
                name: row.get(1)?,
            })
        };
        match vocabulary {
            Some(vocabulary) => statement
                .query_map(params![vocabulary.code()], entry)?
                .collect::<Result<Vec<_>, _>>(),
            None => statement
                .query_map([], entry)?
                .collect::<Result<Vec<_>, _>>(),
        }
    };
    let terms = "SELECT code, label FROM terms WHERE vocabulary = ?1 ORDER BY code";
    Ok(CodeLists {
        branches: table("SELECT code, name FROM branches ORDER BY code", None)?,
        item_types: table(
            "SELECT code, description FROM item_types ORDER BY code",
            None,
        )?,
        locations: table(terms, Some(Vocabulary::Location))?,
        collections: table(terms, Some(Vocabulary::Collection))?,
    })
}
