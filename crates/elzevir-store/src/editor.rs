// SPDX-License-Identifier: GPL-3.0-or-later
//! The writer queue as the staff screen's editor (docs/specs/catalogo/edicao.md, items 16
//! to 24).

use elzevir_app::catalog::{CatalogCommand, CatalogOutput};
use elzevir_app::commands::{WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::editing::{CatalogEditor, Edit, EditFailed};
use elzevir_app::staff::StaffId;

use crate::{CommandFailed, ElzevirStore, SubmitError};

impl CatalogEditor for ElzevirStore {
    fn apply(&self, actor: StaffId, edit: Edit) -> Result<CatalogOutput, EditFailed> {
        let command = match edit {
            Edit::Work(update) => CatalogCommand::UpdateWork(Box::new(update)),
            Edit::Instance(update) => CatalogCommand::UpdateInstance(Box::new(update)),
            Edit::NewRecord(record) => CatalogCommand::AddRecord(record),
            Edit::NewInstance(new) => CatalogCommand::AddInstance(new),
        };
        let pending = self
            .submit_as(actor.0, WriteCommand::Catalog(command))
            .map_err(|error| match error {
                SubmitError::Busy => EditFailed::Busy,
                SubmitError::Closed => EditFailed::Unavailable(error.to_string()),
            })?;
        match pending.wait() {
            Ok(WriteOutput::Catalog(output)) => Ok(output),
            Ok(WriteOutput::StaffCreated(_)) => Err(EditFailed::Unavailable(
                "the writer answered an edit with a staff result".to_owned(),
            )),
            Err(CommandFailed::Rejected(WriteRefusal::Catalog(refusal))) => {
                Err(EditFailed::Refused(refusal))
            }
            Err(failure) => Err(EditFailed::Unavailable(failure.to_string())),
        }
    }
}
