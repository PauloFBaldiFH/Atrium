// SPDX-License-Identifier: GPL-3.0-or-later
//! Export of the catalog as MARC21 (docs/specs/catalogo/linha-de-comando.md).
//!
//! Records are read in Instance order, a batch at a time, and written back with
//! `bibframe_to_marc` starting from the stored source record: a record nobody edited comes
//! out as it came in.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::num::NonZeroUsize;

use elzevir_domain::catalog::InstanceId;
use elzevir_marc::{Limits, MarcReader, Record, RecordRef};

use crate::catalog::{CatalogReadError, NewRecord};
use crate::marc_mapping::{bibframe_to_marc, marc_to_bibframe};

/// One stored record, as the model holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRecord {
    /// The Instance.
    pub instance: InstanceId,
    /// Its Work, Instance and Items; `source` is the stored source record.
    pub record: NewRecord,
}

/// Port: reads the stored records in Instance order. The store implements it.
pub trait RecordSource {
    /// Up to `limit` records whose Instance comes after `after` (from the first when
    /// `None`), in Instance order.
    ///
    /// # Errors
    ///
    /// When storage fails or holds a value this build does not understand.
    fn records_after(
        &self,
        after: Option<InstanceId>,
        limit: NonZeroUsize,
    ) -> Result<Vec<StoredRecord>, CatalogReadError>;
}

/// Settings of one export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    /// Limits applied when reading back the stored source records.
    pub limits: Limits,
    /// Records read from storage at a time.
    pub batch: NonZeroUsize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            batch: NonZeroUsize::new(1024).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

/// Why one record was not exported. The export goes on with the next one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportFailure {
    /// The stored source record is not valid ISO 2709 any more.
    #[error("the stored source record is unreadable: {0}")]
    Source(elzevir_marc::RecordError),
    /// The record does not fit in ISO 2709 (too long, too many fields).
    #[error(transparent)]
    Write(elzevir_marc::WriteError),
}

/// One record left out of the export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedExport {
    /// The Instance.
    pub instance: InstanceId,
    /// What went wrong.
    pub failure: ExportFailure,
}

/// What an export did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Records written.
    pub written: u64,
    /// Records left out, in Instance order.
    pub failures: Vec<FailedExport>,
}

/// Why an export stopped. What was written stays written.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The catalog could not be read.
    #[error(transparent)]
    Read(#[from] CatalogReadError),
    /// The output could not be written.
    #[error("cannot write the output: {0}")]
    Io(#[from] std::io::Error),
}

/// Writes every stored record as ISO 2709 MARC21 (UTF-8), in Instance order.
///
/// # Errors
///
/// [`ExportError`] when reading the catalog or writing the output fails.
pub fn export_marc21<W: Write, S: RecordSource>(
    source: &S,
    out: &mut W,
    options: &ExportOptions,
) -> Result<ExportReport, ExportError> {
    let mut report = ExportReport::default();
    let mut after = None;
    let mut bytes = Vec::new();
    loop {
        let batch = source.records_after(after, options.batch)?;
        let Some(last) = batch.last() else {
            break;
        };
        after = Some(last.instance);
        for stored in batch {
            bytes.clear();
            match to_marc(&stored.record, &options.limits, &mut bytes) {
                Ok(()) => {
                    out.write_all(&bytes)?;
                    report.written = report.written.saturating_add(1);
                }
                Err(failure) => report.failures.push(FailedExport {
                    instance: stored.instance,
                    failure,
                }),
            }
        }
    }
    out.flush()?;
    Ok(report)
}

fn to_marc(record: &NewRecord, limits: &Limits, out: &mut Vec<u8>) -> Result<(), ExportFailure> {
    let Some(source) = record.source.as_deref() else {
        return bibframe_to_marc(record, None)
            .write_iso2709(out)
            .map_err(ExportFailure::Write);
    };
    let original =
        Record::from_ref(&RecordRef::parse(source, limits).map_err(ExportFailure::Source)?);
    // A record nobody changed goes out as the stored bytes: rewriting it would give the
    // same bytes, except for records the writer would refuse but readers accept (an
    // ISO 2709 delimiter inside a control field, as some LC records have).
    if unchanged(record, &original) {
        out.extend_from_slice(source);
        return Ok(());
    }
    bibframe_to_marc(record, Some(&original))
        .write_iso2709(out)
        .map_err(ExportFailure::Write)
}

/// Whether the model still says what `original` maps to.
fn unchanged(record: &NewRecord, original: &Record) -> bool {
    marc_to_bibframe(original, None).is_ok_and(|mapped| same_description(&mapped, record))
}

/// Every part of two records but their source. Destructured whole, so that a new part
/// cannot be forgotten here.
fn same_description(a: &NewRecord, b: &NewRecord) -> bool {
    let NewRecord {
        work_title,
        material_type,
        hub,
        relations,
        classification,
        classification_scheme,
        summary,
        audience,
        languages,
        contributions,
        subjects,
        instance,
        items,
        source: _,
    } = a;
    *work_title == b.work_title
        && *material_type == b.material_type
        && *hub == b.hub
        && *relations == b.relations
        && *classification == b.classification
        && *classification_scheme == b.classification_scheme
        && *summary == b.summary
        && *audience == b.audience
        && *languages == b.languages
        && *contributions == b.contributions
        && *subjects == b.subjects
        && *instance == b.instance
        && *items == b.items
}

/// The branch and item type codes the Items of a MARC21 file refer to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemCodes {
    /// Home and holding branches.
    pub branches: BTreeSet<String>,
    /// Item types.
    pub item_types: BTreeSet<String>,
}

/// Reads a MARC21 file and collects the codes its Items refer to, so that they can be
/// created before importing it. Records that cannot be read or mapped are passed over:
/// the import reports them.
///
/// # Errors
///
/// When the input cannot be read.
pub fn item_codes<R: BufRead>(input: R, limits: Limits) -> Result<ItemCodes, std::io::Error> {
    let mut codes = ItemCodes::default();
    let mut reader = MarcReader::new(input, limits);
    while let Some(next) = reader.next_record() {
        let parsed = match next {
            Ok(parsed) => parsed,
            Err(elzevir_marc::ReadError::Io(error)) => return Err(error),
            Err(elzevir_marc::ReadError::Record { .. }) => continue,
        };
        let mut record = Record::from_ref(&parsed);
        record.marc21_to_utf8();
        let Ok(mapped) = marc_to_bibframe(&record, None) else {
            continue;
        };
        for item in mapped.items {
            codes.branches.insert(item.home_branch);
            codes.branches.insert(item.holding_branch);
            codes.item_types.insert(item.item_type);
        }
    }
    Ok(codes)
}
