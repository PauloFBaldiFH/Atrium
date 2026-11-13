// SPDX-License-Identifier: GPL-3.0-or-later
//! The writer queue as the import's record sink (docs/specs/catalogo/importacao.md).

use elzevir_app::catalog::{CatalogCommand, CatalogOutput, NewRecord};
use elzevir_app::commands::{WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::import::{PendingRecord, RecordRejected, RecordSink, SinkClosed};

use crate::{CommandFailed, ElzevirHandler, ElzevirStore, Pending};

/// A record queued by an import, waiting for the writer's answer.
pub struct PendingAdd(Pending<ElzevirHandler>);

impl RecordSink for ElzevirStore {
    type Pending = PendingAdd;

    fn submit(&self, record: NewRecord) -> Result<PendingAdd, SinkClosed> {
        self.submit_waiting(WriteCommand::Catalog(CatalogCommand::AddRecord(Box::new(
            record,
        ))))
        .map(PendingAdd)
        .map_err(|_| SinkClosed)
    }
}

impl PendingRecord for PendingAdd {
    fn wait(self) -> Result<CatalogOutput, RecordRejected> {
        match self.0.wait() {
            Ok(WriteOutput::Catalog(output)) => Ok(output),
            Ok(WriteOutput::StaffCreated(_)) => Err(RecordRejected::Storage(
                "the writer answered a record with a staff result".to_owned(),
            )),
            Err(CommandFailed::Rejected(WriteRefusal::Catalog(refusal))) => {
                Err(RecordRejected::Refused(refusal))
            }
            Err(CommandFailed::Rejected(refusal)) => {
                Err(RecordRejected::Storage(refusal.to_string()))
            }
            Err(CommandFailed::Storage(error)) => Err(RecordRejected::Storage(error.to_string())),
            Err(CommandFailed::WriterGone) => Err(RecordRejected::WriterGone),
        }
    }
}
