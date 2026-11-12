// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading errors.

use crate::Tag;

/// Why one record was rejected. The reader continues with the next record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    /// Fewer bytes than a leader plus a record terminator.
    #[error("record is {len} bytes, shorter than a leader")]
    TooShort {
        /// Bytes received.
        len: usize,
    },
    /// The record exceeds [`crate::Limits::max_record_len`].
    #[error("record exceeds {limit} bytes")]
    TooLong {
        /// The limit in force.
        limit: usize,
    },
    /// The input ended in the middle of a record.
    #[error("input ended inside a record ({len} bytes without a record terminator)")]
    Truncated {
        /// Bytes received before the end of input.
        len: usize,
    },
    /// A numeric leader position does not hold decimal digits.
    #[error("leader positions {start}-{end} must be digits")]
    LeaderNotNumeric {
        /// First position (0-based).
        start: usize,
        /// Last position (0-based, inclusive).
        end: usize,
    },
    /// The record length in the leader differs from the bytes actually read.
    #[error("leader declares {declared} bytes but the record has {actual}")]
    LengthMismatch {
        /// Length in leader positions 0-4.
        declared: usize,
        /// Bytes up to and including the record terminator.
        actual: usize,
    },
    /// The base address of data is outside the record or does not follow the directory.
    #[error("base address {base} is invalid for a {len}-byte record")]
    BadBaseAddress {
        /// Base address from leader positions 12-16.
        base: usize,
        /// Record length.
        len: usize,
    },
    /// The directory is not a whole number of 12-byte entries ended by a field terminator.
    #[error("directory is malformed")]
    BadDirectory,
    /// More fields than [`crate::Limits::max_fields`].
    #[error("record has {count} fields, more than the limit of {limit}")]
    TooManyFields {
        /// Directory entries.
        count: usize,
        /// The limit in force.
        limit: usize,
    },
    /// A directory entry has non-numeric length or start.
    #[error("directory entry {index} ({tag}) has a non-numeric length or start")]
    EntryNotNumeric {
        /// Entry position in the directory (0-based).
        index: usize,
        /// The entry's tag.
        tag: Tag,
    },
    /// A field points outside the data area or does not end with a field terminator.
    #[error("field {index} ({tag}) is out of bounds or not terminated")]
    BadField {
        /// Entry position in the directory (0-based).
        index: usize,
        /// The entry's tag.
        tag: Tag,
    },
}

/// A reading failure, located in the input.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The underlying reader failed. Reading should stop.
    #[error("cannot read MARC input: {0}")]
    Io(#[from] std::io::Error),
    /// One record was rejected; the next call continues after it.
    #[error("record {index} at byte {offset}: {error}")]
    Record {
        /// Record position in the input (0-based).
        index: u64,
        /// Byte offset of the record's first byte.
        offset: u64,
        /// What was wrong.
        error: RecordError,
    },
}
