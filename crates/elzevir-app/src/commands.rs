// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed write commands (docs/arquitetura/05-definicoes-de-implementacao.md, §1).
//!
//! Every change to an institution's data is one of these values, sent to the writer
//! thread. Each command is serialized into the audit log, so it must carry only data that
//! may be kept there: never a plain password, only its hash.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::catalog::{CatalogCommand, CatalogOutput, CatalogRefusal};
use crate::staff::{Permission, StaffId};

/// Every write the application can make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteCommand {
    /// Staff account management.
    Staff(StaffCommand),
    /// Catalog: records, items and their vocabularies.
    Catalog(CatalogCommand),
}

/// Staff account commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaffCommand {
    /// Creates an account.
    Create {
        /// Unique login.
        login: String,
        /// Argon2id hash (PHC string).
        password_hash: String,
        /// Granted permissions.
        permissions: BTreeSet<Permission>,
    },
}

/// What a successful command returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutput {
    /// A staff account was created.
    StaffCreated(StaffId),
    /// A catalog command succeeded.
    Catalog(CatalogOutput),
}

/// Why a command was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteRefusal {
    /// The login is already taken.
    #[error("login `{0}` is already in use")]
    LoginTaken(String),
    /// A catalog command was refused.
    #[error(transparent)]
    Catalog(#[from] CatalogRefusal),
}

impl WriteCommand {
    /// Short, stable name for the audit log.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Staff(StaffCommand::Create { .. }) => "staff.create",
            Self::Catalog(CatalogCommand::CreateBranch { .. }) => "catalog.create_branch",
            Self::Catalog(CatalogCommand::CreateItemType { .. }) => "catalog.create_item_type",
            Self::Catalog(CatalogCommand::CreateHub { .. }) => "catalog.create_hub",
            Self::Catalog(CatalogCommand::AddRecord(_)) => "catalog.add_record",
            Self::Catalog(CatalogCommand::AddItem(_)) => "catalog.add_item",
            Self::Catalog(CatalogCommand::ImportRecord(_)) => "catalog.import_record",
            Self::Catalog(CatalogCommand::ReplaceRecord { .. }) => "catalog.replace_record",
            Self::Catalog(CatalogCommand::UpdateWork(_)) => "catalog.update_work",
            Self::Catalog(CatalogCommand::UpdateInstance(_)) => "catalog.update_instance",
        }
    }

    /// The permission a staff member needs to submit this command. Checked where a person's
    /// request becomes a command; the writer trusts what it receives.
    #[must_use]
    pub const fn permission(&self) -> Permission {
        match self {
            Self::Staff(_) => Permission::Administration,
            Self::Catalog(_) => Permission::Cataloguing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::WorkUpdate;
    use elzevir_domain::catalog::WorkId;

    #[test]
    fn catalog_edits_need_cataloguing_and_staff_accounts_need_administration() {
        let edit = WriteCommand::Catalog(CatalogCommand::UpdateWork(Box::new(WorkUpdate {
            work: WorkId(1),
            row_version: 1,
            changes: Vec::new(),
        })));
        assert_eq!(edit.permission(), Permission::Cataloguing);
        assert_eq!(edit.kind(), "catalog.update_work");
        let account = WriteCommand::Staff(StaffCommand::Create {
            login: "ana".to_owned(),
            password_hash: String::new(),
            permissions: BTreeSet::new(),
        });
        assert_eq!(account.permission(), Permission::Administration);
    }

    #[test]
    fn permission_names_round_trip() {
        for permission in [
            Permission::StaffInterface,
            Permission::Administration,
            Permission::Cataloguing,
        ] {
            assert_eq!(Permission::from_name(permission.as_str()), Some(permission));
        }
    }
}
