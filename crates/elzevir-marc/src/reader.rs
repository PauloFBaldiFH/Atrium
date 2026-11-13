// SPDX-License-Identifier: GPL-3.0-or-later
//! Streaming ISO 2709 reader over a reused buffer.

use std::io::{BufRead, Read};

use crate::{Limits, RECORD_TERMINATOR, ReadError, RecordError, RecordRef};

/// Reads ISO 2709 records one at a time from any buffered input.
///
/// Memory use is bounded by [`Limits::max_record_len`]: the buffer is reused and never
/// grows past one record. Each [`RecordRef`] borrows that buffer, so it must be dropped
/// before the next call to [`MarcReader::next_record`].
///
/// ```
/// use elzevir_marc::{Limits, MarcReader};
///
/// let input: &[u8] = b"";
/// let mut reader = MarcReader::new(input, Limits::default());
/// while let Some(record) = reader.next_record() {
///     match record {
///         Ok(record) => { let _ = record.field_count(); }
///         Err(error) => eprintln!("skipped: {error}"),
///     }
/// }
/// ```
pub struct MarcReader<R> {
    input: R,
    buffer: Vec<u8>,
    limits: Limits,
    index: u64,
    offset: u64,
}

impl<R: BufRead> MarcReader<R> {
    /// Wraps `input`.
    pub fn new(input: R, limits: Limits) -> Self {
        Self {
            input,
            buffer: Vec::new(),
            limits,
            index: 0,
            offset: 0,
        }
    }

    /// Byte offset where the next record starts (the bytes consumed so far).
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Reads the next record. `None` at the end of the input.
    ///
    /// A rejected record yields `Some(Err(ReadError::Record { .. }))` and the following
    /// call continues after it. An I/O error yields `Some(Err(ReadError::Io(_)))`; callers
    /// should stop there.
    pub fn next_record(&mut self) -> Option<Result<RecordRef<'_>, ReadError>> {
        self.buffer.clear();
        let index = self.index;
        let offset = self.offset;
        let cap = u64::try_from(self.limits.max_record_len)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let read = match (&mut self.input)
            .take(cap)
            .read_until(RECORD_TERMINATOR, &mut self.buffer)
        {
            Ok(read) => read,
            Err(error) => return Some(Err(ReadError::Io(error))),
        };
        if read == 0 {
            return None;
        }
        self.index = self.index.saturating_add(1);
        self.offset = self.offset.saturating_add(len_u64(read));

        let located = |error| ReadError::Record {
            index,
            offset,
            error,
        };
        if self.buffer.last() != Some(&RECORD_TERMINATOR) {
            if read > self.limits.max_record_len {
                return Some(match self.skip_past_terminator() {
                    Ok(()) => Err(located(RecordError::TooLong {
                        limit: self.limits.max_record_len,
                    })),
                    Err(error) => Err(ReadError::Io(error)),
                });
            }
            // End of input without a terminator. Trailing whitespace (a final newline is
            // common in exported files) is not a record.
            if self.buffer.iter().all(u8::is_ascii_whitespace) {
                return None;
            }
            return Some(Err(located(RecordError::Truncated { len: read })));
        }
        Some(RecordRef::parse(&self.buffer, &self.limits).map_err(located))
    }

    /// Discards input up to and including the next record terminator.
    fn skip_past_terminator(&mut self) -> std::io::Result<()> {
        loop {
            let available = self.input.fill_buf()?;
            if available.is_empty() {
                return Ok(());
            }
            let (consumed, found) = match available.iter().position(|b| *b == RECORD_TERMINATOR) {
                Some(at) => (at.saturating_add(1), true),
                None => (available.len(), false),
            };
            self.input.consume(consumed);
            self.offset = self.offset.saturating_add(len_u64(consumed));
            if found {
                return Ok(());
            }
        }
    }
}

fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}
