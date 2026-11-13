// SPDX-License-Identifier: GPL-3.0-or-later
//! ISO 2709 serialization.

use crate::{
    FIELD_TERMINATOR, Field, LEADER_LEN, RECORD_TERMINATOR, Record, SUBFIELD_DELIMITER, Tag,
    WriteError,
};

/// Length of one directory entry.
const ENTRY_LEN: usize = 12;
/// Largest field length a directory entry can state (four digits).
const MAX_FIELD_LEN: usize = 9_999;
/// Largest record length the leader can state (five digits).
const MAX_RECORD_LEN: usize = 99_999;

impl Record {
    /// Appends the record to `out` as ISO 2709, setting the leader's record length, base
    /// address and MARC21 constants (`22` at 10–11, `4500` at 20–23). On error `out` is
    /// left as it was.
    ///
    /// Koha: `MARC::File::USMARC::encode` (`as_usmarc`).
    ///
    /// # Errors
    ///
    /// [`WriteError::FieldTooLong`], [`WriteError::RecordTooLong`] or
    /// [`WriteError::DelimiterInData`].
    pub fn write_iso2709(&self, out: &mut Vec<u8>) -> Result<(), WriteError> {
        let mut directory = Vec::with_capacity(self.fields.len().saturating_mul(ENTRY_LEN));
        let mut data = Vec::new();
        for field in &self.fields {
            let start = data.len();
            write_field(field, &mut data)?;
            let len = data.len().saturating_sub(start);
            if len > MAX_FIELD_LEN {
                return Err(WriteError::FieldTooLong {
                    tag: field.tag(),
                    len,
                });
            }
            directory.extend_from_slice(field.tag().as_bytes());
            directory.extend_from_slice(format!("{len:04}{start:05}").as_bytes());
        }
        let base = LEADER_LEN + directory.len() + 1;
        let len = base + data.len() + 1;
        // The start of the last field is below the record length, so checking the length
        // also keeps every start within five digits.
        if len > MAX_RECORD_LEN {
            return Err(WriteError::RecordTooLong { len });
        }
        let mut leader = self.leader;
        let numbers = format!("{len:05}{base:05}");
        let (length_digits, base_digits) = numbers.as_bytes().split_at(5);
        splice(&mut leader, 0, length_digits);
        splice(&mut leader, 10, b"22");
        splice(&mut leader, 12, base_digits);
        splice(&mut leader, 20, b"4500");

        out.reserve(len);
        out.extend_from_slice(&leader);
        out.extend_from_slice(&directory);
        out.push(FIELD_TERMINATOR);
        out.extend_from_slice(&data);
        out.push(RECORD_TERMINATOR);
        Ok(())
    }
}

/// Koha: `MARC::Field::as_usmarc`.
fn write_field(field: &Field, out: &mut Vec<u8>) -> Result<(), WriteError> {
    match field {
        Field::Control { tag, value } => {
            check_bytes(*tag, value)?;
            out.extend_from_slice(value);
        }
        Field::Data {
            tag,
            indicators,
            subfields,
        } => {
            check_bytes(*tag, indicators)?;
            out.extend_from_slice(indicators);
            for subfield in subfields {
                check_bytes(*tag, &[subfield.code])?;
                check_bytes(*tag, &subfield.value)?;
                out.push(SUBFIELD_DELIMITER);
                out.push(subfield.code);
                out.extend_from_slice(&subfield.value);
            }
        }
    }
    out.push(FIELD_TERMINATOR);
    Ok(())
}

fn check_bytes(tag: Tag, bytes: &[u8]) -> Result<(), WriteError> {
    if bytes.iter().any(|b| {
        matches!(
            *b,
            RECORD_TERMINATOR | FIELD_TERMINATOR | SUBFIELD_DELIMITER
        )
    }) {
        Err(WriteError::DelimiterInData { tag })
    } else {
        Ok(())
    }
}

/// Copies `bytes` into the leader at `at`. Every call site writes inside the 24 bytes.
fn splice(leader: &mut [u8; LEADER_LEN], at: usize, bytes: &[u8]) {
    if let Some(target) = leader.get_mut(at..at.saturating_add(bytes.len())) {
        target.copy_from_slice(bytes);
    }
}
