// SPDX-License-Identifier: GPL-3.0-or-later
//! A validated ISO 2709 record that borrows its bytes.

use crate::{
    FIELD_TERMINATOR, LEADER_LEN, Limits, RECORD_TERMINATOR, RecordError, SUBFIELD_DELIMITER, Tag,
};
/// Length of one directory entry: tag (3), field length (4), start position (5).
const ENTRY_LEN: usize = 12;

/// Character coding declared in leader position 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// `a`: UCS/Unicode, stored as UTF-8.
    Utf8,
    /// Blank or anything else: MARC-8 in MARC21, ISO 5426 and friends in UNIMARC. Decoding
    /// these is task 1.4.
    Legacy,
}

/// One ISO 2709 record, validated, borrowing the bytes it was parsed from.
#[derive(Clone, Copy)]
pub struct RecordRef<'a> {
    raw: &'a [u8],
    leader: &'a [u8; LEADER_LEN],
    directory: &'a [u8],
    data: &'a [u8],
}

impl<'a> RecordRef<'a> {
    /// Validates `raw` (one record, record terminator included) and borrows it.
    ///
    /// Validation covers the leader's numeric positions, the declared length, the base
    /// address, every directory entry and every field's bounds and terminator, so that
    /// iterating fields afterwards cannot fail.
    ///
    /// # Errors
    ///
    /// The first [`RecordError`] found.
    pub fn parse(raw: &'a [u8], limits: &Limits) -> Result<Self, RecordError> {
        let len = raw.len();
        if len > limits.max_record_len {
            return Err(RecordError::TooLong {
                limit: limits.max_record_len,
            });
        }
        let leader: &[u8; LEADER_LEN] = raw
            .first_chunk()
            .filter(|_| len > LEADER_LEN)
            .ok_or(RecordError::TooShort { len })?;
        if raw.last() != Some(&RECORD_TERMINATOR) {
            return Err(RecordError::Truncated { len });
        }

        let declared = leader_number(leader, 0, 4)?;
        if declared != len {
            return Err(RecordError::LengthMismatch {
                declared,
                actual: len,
            });
        }
        let base = leader_number(leader, 12, 16)?;
        // The directory sits between the leader and the base address, and ends with a field
        // terminator just before the base address.
        let bad_base = RecordError::BadBaseAddress { base, len };
        let directory_with_terminator = raw.get(LEADER_LEN..base).ok_or(bad_base.clone())?;
        let (&terminator, directory) = directory_with_terminator
            .split_last()
            .ok_or(bad_base.clone())?;
        if terminator != FIELD_TERMINATOR || directory.len() % ENTRY_LEN != 0 {
            return Err(RecordError::BadDirectory);
        }
        // Data runs from the base address up to, not including, the record terminator.
        let data = raw.get(base..len - 1).ok_or(bad_base)?;

        let count = directory.len() / ENTRY_LEN;
        if count > limits.max_fields {
            return Err(RecordError::TooManyFields {
                count,
                limit: limits.max_fields,
            });
        }
        for (index, entry) in directory.chunks_exact(ENTRY_LEN).enumerate() {
            check_entry(index, entry, data)?;
        }

        Ok(Self {
            raw,
            leader,
            directory,
            data,
        })
    }

    /// The whole record, as read.
    #[must_use]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.raw
    }

    /// The 24-byte leader.
    #[must_use]
    pub fn leader(&self) -> &'a [u8; LEADER_LEN] {
        self.leader
    }

    /// Character coding declared in leader position 9.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        match self.leader.get(9) {
            Some(b'a') => Encoding::Utf8,
            _ => Encoding::Legacy,
        }
    }

    /// The fields, in directory order.
    pub fn fields(&self) -> impl Iterator<Item = FieldRef<'a>> + 'a {
        let data = self.data;
        self.directory
            .chunks_exact(ENTRY_LEN)
            .filter_map(move |entry| entry_field(entry, data))
    }

    /// The fields with `tag`, in directory order.
    pub fn fields_with_tag(&self, tag: Tag) -> impl Iterator<Item = FieldRef<'a>> + 'a {
        self.fields().filter(move |field| field.tag() == tag)
    }

    /// Number of fields.
    #[must_use]
    pub fn field_count(&self) -> usize {
        self.directory.len() / ENTRY_LEN
    }
}

impl std::fmt::Debug for RecordRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordRef")
            .field("len", &self.raw.len())
            .field("fields", &self.field_count())
            .finish_non_exhaustive()
    }
}

/// One field, borrowing the record's bytes. Its data excludes the field terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRef<'a> {
    tag: Tag,
    data: &'a [u8],
}

impl<'a> FieldRef<'a> {
    /// The field's tag.
    #[must_use]
    pub fn tag(&self) -> Tag {
        self.tag
    }

    /// Raw field bytes, without the field terminator. For data fields this starts with the
    /// two indicators.
    #[must_use]
    pub fn raw(&self) -> &'a [u8] {
        self.data
    }

    /// The two indicators of a data field; `None` for control fields or a field shorter
    /// than two bytes.
    #[must_use]
    pub fn indicators(&self) -> Option<[u8; 2]> {
        if self.tag.is_control() {
            return None;
        }
        self.data.first_chunk::<2>().copied()
    }

    /// The subfields of a data field as `(code, value)` pairs, in order. Empty for control
    /// fields. Bytes between the indicators and the first delimiter are ignored.
    #[must_use]
    pub fn subfields(&self) -> Subfields<'a> {
        let body = if self.tag.is_control() {
            &[][..]
        } else {
            self.data.get(2..).unwrap_or_default()
        };
        Subfields {
            parts: body.split(is_subfield_delimiter),
            started: false,
        }
    }

    /// Values of the subfields with `code`, in order.
    pub fn subfield_values(&self, code: u8) -> impl Iterator<Item = &'a [u8]> + 'a {
        self.subfields()
            .filter(move |&(c, _)| c == code)
            .map(|(_, value)| value)
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "`slice::split` requires a `fn(&u8) -> bool` predicate"
)]
fn is_subfield_delimiter(byte: &u8) -> bool {
    *byte == SUBFIELD_DELIMITER
}

/// Iterator over `(code, value)` pairs of a data field.
#[derive(Debug, Clone)]
pub struct Subfields<'a> {
    parts: std::slice::Split<'a, u8, fn(&u8) -> bool>,
    started: bool,
}

impl<'a> Iterator for Subfields<'a> {
    type Item = (u8, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.started {
            // Whatever precedes the first delimiter is not a subfield.
            self.started = true;
            self.parts.next()?;
        }
        loop {
            let part = self.parts.next()?;
            if let Some((&code, value)) = part.split_first() {
                return Some((code, value));
            }
            // Two delimiters in a row: an empty, codeless subfield; skip it.
        }
    }
}

/// Parses leader positions `start..=end` as a decimal number.
fn leader_number(
    leader: &[u8; LEADER_LEN],
    start: usize,
    end: usize,
) -> Result<usize, RecordError> {
    leader
        .get(start..=end)
        .and_then(ascii_number)
        .ok_or(RecordError::LeaderNotNumeric { start, end })
}

/// Parses ASCII decimal digits; `None` for any other byte or on overflow.
fn ascii_number(digits: &[u8]) -> Option<usize> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0_usize, |acc, &byte| {
        let digit = usize::from(byte.checked_sub(b'0').filter(|d| *d <= 9)?);
        acc.checked_mul(10)?.checked_add(digit)
    })
}

/// Splits a 12-byte directory entry into tag, field length and start.
fn split_entry(entry: &[u8]) -> Option<(Tag, Option<usize>, Option<usize>)> {
    let (tag, rest) = entry.split_first_chunk::<3>()?;
    let (length, start) = rest.split_first_chunk::<4>()?;
    Some((Tag::new(*tag), ascii_number(length), ascii_number(start)))
}

/// Validates one directory entry against the data area.
fn check_entry(index: usize, entry: &[u8], data: &[u8]) -> Result<(), RecordError> {
    let (tag, length, start) = split_entry(entry).ok_or(RecordError::BadDirectory)?;
    let (Some(length), Some(start)) = (length, start) else {
        return Err(RecordError::EntryNotNumeric { index, tag });
    };
    let field = start
        .checked_add(length)
        .and_then(|end| data.get(start..end))
        .ok_or(RecordError::BadField { index, tag })?;
    if field.last() == Some(&FIELD_TERMINATOR) {
        Ok(())
    } else {
        Err(RecordError::BadField { index, tag })
    }
}

/// Resolves an entry already accepted by [`check_entry`] to its field.
fn entry_field<'a>(entry: &[u8], data: &'a [u8]) -> Option<FieldRef<'a>> {
    let (tag, Some(length), Some(start)) = split_entry(entry)? else {
        return None;
    };
    let with_terminator = data.get(start..start.checked_add(length)?)?;
    let (_, field) = with_terminator.split_last()?;
    Some(FieldRef { tag, data: field })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_number_accepts_only_digits() {
        assert_eq!(ascii_number(b"00042"), Some(42));
        assert_eq!(ascii_number(b"0042 "), None);
        assert_eq!(ascii_number(b""), None);
        assert_eq!(ascii_number(b"99999999999999999999999999"), None);
    }
}
