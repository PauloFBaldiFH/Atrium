// SPDX-License-Identifier: GPL-3.0-or-later
//! Legacy character sets to UTF-8 (task 1.4, docs/specs/marc/conversao-de-caracteres.md).

mod iso5426;
mod marc8;

pub use iso5426::iso5426_to_utf8;
pub use marc8::{Marc8Text, marc8_to_utf8};

use crate::{Field, Record};

/// The character set a record turned out to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedEncoding {
    /// Already UTF-8; nothing was converted.
    Utf8,
    /// Converted from MARC-8.
    Marc8,
    /// MARC-8 failed somewhere in the record, so every value was read as ISO-8859-1.
    Latin1,
}

impl Record {
    /// Brings a MARC21 record of unknown character set to UTF-8 and sets leader position 9
    /// to `a`. Only subfield values change: control fields and indicators are left as read.
    ///
    /// Koha: `C4::Charset::MarcToUTF8Record` without a source encoding, for MARC21
    /// (`_default_marc21_charconv_to_utf8`).
    pub fn marc21_to_utf8(&mut self) -> DetectedEncoding {
        let detected = if self
            .values()
            .all(|value| std::str::from_utf8(value).is_ok())
        {
            DetectedEncoding::Utf8
        } else if self.convert_values(|value| {
            let decoded = marc8_to_utf8(value);
            decoded
                .unmapped
                .is_empty()
                .then(|| decoded.text.into_owned().into_bytes())
        }) {
            DetectedEncoding::Marc8
        } else {
            self.convert_values(|value| {
                Some(
                    value
                        .iter()
                        .map(|&b| char::from(b))
                        .collect::<String>()
                        .into_bytes(),
                )
            });
            DetectedEncoding::Latin1
        };
        if let Some(position) = self.leader.get_mut(9) {
            *position = b'a';
        }
        detected
    }

    /// Every value in the record, control fields included (Koha checks the whole blob).
    fn values(&self) -> impl Iterator<Item = &[u8]> {
        self.fields
            .iter()
            .flat_map(|field| -> Box<dyn Iterator<Item = &[u8]>> {
                match field {
                    Field::Control { value, .. } => Box::new(std::iter::once(value.as_slice())),
                    Field::Data { subfields, .. } => {
                        Box::new(subfields.iter().map(|subfield| subfield.value.as_slice()))
                    }
                }
            })
    }

    /// Replaces every subfield value with `convert(value)`, all or nothing: returns
    /// `false` and leaves the record untouched if any value fails.
    fn convert_values(&mut self, convert: impl Fn(&[u8]) -> Option<Vec<u8>>) -> bool {
        let mut converted = Vec::new();
        for field in &self.fields {
            if let Field::Data { subfields, .. } = field {
                for subfield in subfields {
                    match convert(&subfield.value) {
                        Some(value) => converted.push(value),
                        None => return false,
                    }
                }
            }
        }
        let mut converted = converted.into_iter();
        for field in &mut self.fields {
            if let Field::Data { subfields, .. } = field {
                for subfield in subfields {
                    if let Some(value) = converted.next() {
                        subfield.value = value;
                    }
                }
            }
        }
        true
    }
}
