// SPDX-License-Identifier: GPL-3.0-or-later
//! The application's command handler and port implementations on SQLite.

use std::collections::BTreeSet;

use elzevir_app::commands::{StaffCommand, WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::staff::{Permission, StaffAccount, StaffDirectory, StaffDirectoryError, StaffId};
use rusqlite::{ErrorCode, OptionalExtension, params};

use crate::{CommandFailed, CommandHandler, Store, StoreError, WriteTx};

/// Applies [`WriteCommand`]s with SQL. Holds no state: the database is the state.
#[derive(Debug, Default)]
pub struct ElzevirHandler;

/// A store running the application's commands.
pub type ElzevirStore = Store<ElzevirHandler>;

impl CommandHandler for ElzevirHandler {
    type Command = WriteCommand;
    type Output = WriteOutput;
    type Rejection = WriteRefusal;

    fn kind(command: &WriteCommand) -> &'static str {
        command.kind()
    }

    fn apply(
        &mut self,
        tx: &WriteTx<'_>,
        command: &WriteCommand,
    ) -> Result<WriteOutput, CommandFailed<WriteRefusal>> {
        match command {
            WriteCommand::Staff(StaffCommand::Create {
                login,
                password_hash,
                permissions,
            }) => create_staff(tx, login, password_hash, permissions),
            WriteCommand::Catalog(command) => crate::catalog::apply(tx, command),
        }
    }
}

fn create_staff(
    tx: &WriteTx<'_>,
    login: &str,
    password_hash: &str,
    permissions: &BTreeSet<Permission>,
) -> Result<WriteOutput, CommandFailed<WriteRefusal>> {
    let connection = tx.connection();
    let inserted = connection.execute(
        "INSERT INTO staff_accounts (login, password_hash) VALUES (?1, ?2)",
        params![login, password_hash],
    );
    match inserted {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => {
            return Err(CommandFailed::Rejected(WriteRefusal::LoginTaken(
                login.to_owned(),
            )));
        }
        Err(e) => return Err(StoreError::from(e).into()),
    }
    let id = connection.last_insert_rowid();
    for permission in permissions {
        connection
            .execute(
                "INSERT INTO staff_permissions (staff_id, permission) VALUES (?1, ?2)",
                params![id, permission.as_str()],
            )
            .map_err(StoreError::from)?;
    }
    Ok(WriteOutput::StaffCreated(StaffId(id)))
}

impl StaffDirectory for ElzevirStore {
    fn by_login(&self, login: &str) -> Result<Option<StaffAccount>, StaffDirectoryError> {
        self.readers()
            .read(|c| {
                let Some((id, password_hash)) = c
                    .query_row(
                        "SELECT id, password_hash FROM staff_accounts WHERE login = ?1",
                        params![login],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()?
                else {
                    return Ok(None);
                };
                let mut statement =
                    c.prepare("SELECT permission FROM staff_permissions WHERE staff_id = ?1")?;
                let names = statement
                    .query_map(params![id], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some((id, password_hash, names)))
            })
            .map_err(|e: rusqlite::Error| StaffDirectoryError(e.to_string()))?
            .map(|(id, password_hash, names)| {
                let permissions = names
                    .iter()
                    .map(|name| {
                        Permission::from_name(name).ok_or_else(|| {
                            StaffDirectoryError(format!("unknown permission `{name}` in database"))
                        })
                    })
                    .collect::<Result<_, _>>()?;
                Ok(StaffAccount {
                    id: StaffId(id),
                    login: login.to_owned(),
                    password_hash,
                    permissions,
                })
            })
            .transpose()
    }
}
