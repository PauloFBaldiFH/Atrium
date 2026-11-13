// SPDX-License-Identifier: GPL-3.0-or-later
//! An owned, editable MARC record: what the writers serialize.

use crate::{LEADER_LEN, RecordRef, Tag};

/// A MARC record that owns its data, built by hand or from a [`RecordRef`].
///
/// Values are bytes: UTF-8 when leader position 9 is `a`, a legacy character set otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The leader. Writing ISO 2709 fills in the lengths and the MARC21 constants.
    pub leader: [u8; LEADER_LEN],
    /// The fields, in order.
    pub fields: Vec<Field>,
}

/// One field of a [`Record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A control field (`001` to `009`): one value, no indicators.
    Control {
        /// The tag.
        tag: Tag,
        /// The value.
        value: Vec<u8>,
    },
    /// A data field: two indicators and ordered subfields.
    Data {
        /// The tag.
        tag: Tag,
        /// The two indicators.
        indicators: [u8; 2],
        /// The subfields, in order.
        subfields: Vec<Subfield>,
    },
}

/// One subfield of a data field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subfield {
    /// The one-byte code, such as `a`.
    pub code: u8,
    /// The value.
    pub value: Vec<u8>,
}

impl Field {
    /// The field's tag.
    #[must_use]
    pub fn tag(&self) -> Tag {
        match self {
            Self::Control { tag, .. } | Self::Data { tag, .. } => *tag,
        }
    }
}

impl Record {
    /// Copies a record that was read, keeping what `MARC::Record` keeps: bytes between the
    /// indicators and the first subfield, and empty codeless subfields, are dropped; a data
    /// field shorter than two bytes gets blank indicators.
    #[must_use]
    pub fn from_ref(record: &RecordRef<'_>) -> Self {
        let fields = record
            .fields()
            .map(|field| {
                let tag = field.tag();
                if tag.is_control() {
                    return Field::Control {
                        tag,
                        value: field.raw().to_vec(),
                    };
                }
                let raw = field.raw();
                let indicators = [
                    raw.first().copied().unwrap_or(b' '),
                    raw.get(1).copied().unwrap_or(b' '),
                ];
                let subfields = field
                    .subfields()
                    .map(|(code, value)| Subfield {
                        code,
                        value: value.to_vec(),
                    })
                    .collect();
                Field::Data {
                    tag,
                    indicators,
                    subfields,
                }
            })
            .collect();
        Self {
            leader: *record.leader(),
            fields,
        }
    }
}
