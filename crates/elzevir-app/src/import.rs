// SPDX-License-Identifier: GPL-3.0-or-later
//! Batch import of MARC21 files (docs/specs/catalogo/importacao.md).
//!
//! Reading and mapping are pure, so they run on several threads; the records then go to
//! the writer one by one, in file order. The order is what keeps a parallel import equal
//! to a serial one (docs/arquitetura/03-estrategia-reconstrucao.md, §1.2 items 2 and 8).

use std::collections::VecDeque;
use std::io::BufRead;
use std::num::NonZeroUsize;

use elzevir_marc::{DetectedEncoding, Limits, MarcReader, ReadError, Record, RecordRef};

use crate::catalog::{CatalogOutput, CatalogRefusal, NewRecord};
use crate::marc_mapping::{MappingError, marc_to_bibframe};
pub use crate::parallelism::Parallelism;

/// Settings of one import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    /// Limits applied to every record read.
    pub limits: Limits,
    /// How decoding and mapping run.
    pub parallelism: Parallelism,
    /// Records read and mapped together before they are handed to the writer.
    pub chunk: NonZeroUsize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            parallelism: Parallelism::Serial,
            chunk: NonZeroUsize::new(1024).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

/// Port: where mapped records go. The store implements it with the writer queue.
pub trait RecordSink {
    /// A record that was queued and has not been answered yet.
    type Pending: PendingRecord;

    /// Queues one record, waiting while the queue is full. Records are applied in the
    /// order they are queued.
    ///
    /// # Errors
    ///
    /// [`SinkClosed`] when the writer has stopped.
    fn submit(&self, record: NewRecord) -> Result<Self::Pending, SinkClosed>;
}

/// The answer to one queued record.
pub trait PendingRecord {
    /// Waits until the record is committed or refused.
    ///
    /// # Errors
    ///
    /// Why the record was not stored.
    fn wait(self) -> Result<CatalogOutput, RecordRejected>;
}

/// The writer stopped, so nothing more can be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the writer has stopped")]
pub struct SinkClosed;

/// Why the writer did not store a record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordRejected {
    /// A catalog rule refused it (unknown branch, barcode taken...).
    #[error(transparent)]
    Refused(CatalogRefusal),
    /// Storage failed; the message comes from the store.
    #[error("storage failed: {0}")]
    Storage(String),
    /// The writer stopped before answering.
    #[error("the writer stopped before answering")]
    WriterGone,
}

/// Why one record of the file was not imported. The import goes on with the next one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordFailure {
    /// The bytes are not a valid ISO 2709 record (or break a limit).
    #[error(transparent)]
    Unreadable(elzevir_marc::RecordError),
    /// The record lacks what the model requires.
    #[error(transparent)]
    Mapping(MappingError),
    /// The converted record could not be written back as ISO 2709 for `source_records`.
    #[error(transparent)]
    Source(elzevir_marc::WriteError),
    /// The writer refused or failed to store it.
    #[error(transparent)]
    Rejected(RecordRejected),
}

/// One record that was not imported, and where it is in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedRecord {
    /// Position of the record in the file (0-based).
    pub index: u64,
    /// Byte offset of the record's first byte.
    pub offset: u64,
    /// What went wrong.
    pub failure: RecordFailure,
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Records found in the file, imported or not.
    pub read: u64,
    /// Records stored.
    pub imported: u64,
    /// Items stored with them.
    pub items: u64,
    /// Records not stored, in file order.
    pub failures: Vec<FailedRecord>,
}

/// Why an import stopped before the end of the file. Records already stored stay.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The input could not be read.
    #[error("cannot read the input: {0}")]
    Io(#[from] std::io::Error),
    /// The writer stopped.
    #[error(transparent)]
    SinkClosed(#[from] SinkClosed),
}

/// A record read from the file, with its place in it.
struct RawRecord {
    index: u64,
    offset: u64,
    bytes: Vec<u8>,
}

/// Imports every MARC21 record of an ISO 2709 file.
///
/// Records are decoded to UTF-8 and mapped (in parallel when `options` say so), then
/// queued in file order. A broken, unmappable or refused record is reported and skipped.
///
/// # Errors
///
/// [`ImportError`] when reading the input fails or the writer stops; what was already
/// committed stays.
pub fn import_marc21<R: BufRead, S: RecordSink>(
    input: R,
    sink: &S,
    options: &ImportOptions,
) -> Result<ImportReport, ImportError> {
    let mut reader = MarcReader::new(input, options.limits);
    let mut report = ImportReport::default();
    // The previous chunk's answers, collected after the next chunk is mapped so that the
    // writer commits one chunk while the threads map the next.
    let mut waiting: VecDeque<(RawPlace, S::Pending)> = VecDeque::new();
    loop {
        let chunk = read_chunk(&mut reader, options.chunk.get(), &mut report)?;
        if chunk.is_empty() {
            break;
        }
        let mapped = map_chunk(chunk, options);
        collect(&mut waiting, &mut report);
        for (place, result) in mapped {
            match result {
                Ok(record) => waiting.push_back((place, sink.submit(record)?)),
                Err(failure) => report.failures.push(place.failed(failure)),
            }
        }
    }
    collect(&mut waiting, &mut report);
    report.failures.sort_by_key(|failed| failed.index);
    Ok(report)
}

/// Index and offset of a record, kept after its bytes are gone.
#[derive(Debug, Clone, Copy)]
struct RawPlace {
    index: u64,
    offset: u64,
}

impl RawPlace {
    fn failed(self, failure: RecordFailure) -> FailedRecord {
        FailedRecord {
            index: self.index,
            offset: self.offset,
            failure,
        }
    }
}

fn read_chunk<R: BufRead>(
    reader: &mut MarcReader<R>,
    size: usize,
    report: &mut ImportReport,
) -> Result<Vec<RawRecord>, ImportError> {
    let mut chunk = Vec::with_capacity(size);
    while chunk.len() < size {
        let index = report.read;
        let offset = reader.offset();
        match reader.next_record() {
            None => break,
            Some(Ok(record)) => {
                report.read = report.read.saturating_add(1);
                chunk.push(RawRecord {
                    index,
                    offset,
                    bytes: record.as_bytes().to_vec(),
                });
            }
            Some(Err(ReadError::Record {
                index,
                offset,
                error,
            })) => {
                report.read = report.read.saturating_add(1);
                report.failures.push(FailedRecord {
                    index,
                    offset,
                    failure: RecordFailure::Unreadable(error),
                });
            }
            Some(Err(ReadError::Io(error))) => return Err(ImportError::Io(error)),
        }
    }
    Ok(chunk)
}

fn map_chunk(
    chunk: Vec<RawRecord>,
    options: &ImportOptions,
) -> Vec<(RawPlace, Result<NewRecord, RecordFailure>)> {
    options
        .parallelism
        .map(chunk, |raw| prepare(raw, &options.limits))
}

/// Decodes and maps one record. Pure: the same bytes give the same result on any thread.
fn prepare(raw: RawRecord, limits: &Limits) -> (RawPlace, Result<NewRecord, RecordFailure>) {
    let place = RawPlace {
        index: raw.index,
        offset: raw.offset,
    };
    (place, to_model(raw.bytes, limits))
}

fn to_model(bytes: Vec<u8>, limits: &Limits) -> Result<NewRecord, RecordFailure> {
    let parsed = RecordRef::parse(&bytes, limits).map_err(RecordFailure::Unreadable)?;
    let already_utf8 = parsed.leader().get(9) == Some(&b'a');
    let mut record = Record::from_ref(&parsed);
    let detected = record.marc21_to_utf8();
    // Keep the bytes as they arrived when they already were UTF-8 MARC21; otherwise keep
    // the converted record, since `source_records` holds UTF-8 (spec §2).
    let source = if already_utf8 && detected == DetectedEncoding::Utf8 {
        bytes
    } else {
        let mut converted = Vec::with_capacity(bytes.len());
        record
            .write_iso2709(&mut converted)
            .map_err(RecordFailure::Source)?;
        converted
    };
    marc_to_bibframe(&record, Some(source)).map_err(RecordFailure::Mapping)
}

fn collect<P: PendingRecord>(waiting: &mut VecDeque<(RawPlace, P)>, report: &mut ImportReport) {
    while let Some((place, pending)) = waiting.pop_front() {
        match pending.wait() {
            Ok(CatalogOutput::Record { items, .. }) => {
                report.imported = report.imported.saturating_add(1);
                report.items = report
                    .items
                    .saturating_add(u64::try_from(items.len()).unwrap_or(u64::MAX));
            }
            Ok(_) => report.imported = report.imported.saturating_add(1),
            Err(rejected) => report
                .failures
                .push(place.failed(RecordFailure::Rejected(rejected))),
        }
    }
}
