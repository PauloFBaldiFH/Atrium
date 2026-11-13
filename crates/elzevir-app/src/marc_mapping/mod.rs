// SPDX-License-Identifier: GPL-3.0-or-later
//! The MARC21 edge of the catalog: `marc_to_bibframe` (docs/specs/catalogo/mapeamento-marc21.md).
//!
//! This is the only place, with the export that will mirror it, that reads MARC tags.
//! Everything it does not map stays in the preserved source record.

mod export;
mod instance;
mod items;
mod text;
mod work;

use elzevir_marc::{Field, Record, Subfield};

use crate::catalog::NewRecord;

pub use export::bibframe_to_marc;

/// Why a MARC21 record could not be mapped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingError {
    /// The record has no title in `245`.
    #[error("the record has no title (245)")]
    MissingTitle,
    /// An item field has no home branch.
    #[error("item field {position} (952) has no home branch ($a)")]
    MissingHomeBranch {
        /// Zero-based position of the `952` among the record's `952` fields.
        position: usize,
    },
    /// An item field has no item type, and the record has no default one.
    #[error("item field {position} (952) has no item type ($y or 942$c)")]
    MissingItemType {
        /// Zero-based position of the `952` among the record's `952` fields.
        position: usize,
    },
}

/// Maps one MARC21 record, already in UTF-8, to a Work, its Instance and its Items.
///
/// `source` is the record as it arrived, kept byte for byte in `source_records`.
///
/// # Errors
///
/// When the record lacks what the model requires: see [`MappingError`].
pub fn marc_to_bibframe(
    record: &Record,
    source: Option<Vec<u8>>,
) -> Result<NewRecord, MappingError> {
    let marc = Marc(record);
    let work = work::map(&marc)?;
    let instance = instance::map(&marc, work.material_type);
    let items = items::map(&marc)?;
    Ok(NewRecord {
        work_title: work.title,
        material_type: work.material_type,
        hub: None,
        relations: Vec::new(),
        classification: work.classification,
        classification_scheme: work.classification_scheme,
        summary: work.summary,
        languages: work.languages,
        contributions: work.contributions,
        subjects: work.subjects,
        instance,
        items,
        source,
    })
}

/// Read helpers over a record.
struct Marc<'a>(&'a Record);

/// One data field: indicators and subfields.
#[derive(Clone, Copy)]
struct DataField<'a> {
    indicators: [u8; 2],
    subfields: &'a [Subfield],
}

impl<'a> Marc<'a> {
    fn leader_byte(&self, position: usize) -> u8 {
        self.0.leader.get(position).copied().unwrap_or(b' ')
    }

    fn control(&self, tag: [u8; 3]) -> Option<&'a [u8]> {
        self.0.fields.iter().find_map(|field| match field {
            Field::Control { tag: t, value } if *t.as_bytes() == tag => Some(value.as_slice()),
            _ => None,
        })
    }

    fn data(&self, tag: &'a [u8; 3]) -> impl Iterator<Item = DataField<'a>> + 'a {
        self.0.fields.iter().filter_map(move |field| match field {
            Field::Data {
                tag: t,
                indicators,
                subfields,
            } if t.as_bytes() == tag => Some(DataField {
                indicators: *indicators,
                subfields,
            }),
            _ => None,
        })
    }

    /// Data fields whose tag is in `tags`, in record order, with their tag.
    fn data_in(
        &self,
        tags: &'a [&'a [u8; 3]],
    ) -> impl Iterator<Item = (&'a [u8; 3], DataField<'a>)> + 'a {
        self.0.fields.iter().filter_map(move |field| match field {
            Field::Data {
                tag,
                indicators,
                subfields,
            } => tags
                .iter()
                .find(|wanted| **wanted == tag.as_bytes())
                .map(|wanted| {
                    (
                        *wanted,
                        DataField {
                            indicators: *indicators,
                            subfields,
                        },
                    )
                }),
            Field::Control { .. } => None,
        })
    }

    fn first(&self, tag: &'a [u8; 3]) -> Option<DataField<'a>> {
        self.data(tag).next()
    }

    /// The cleaned first `$code` of the first `tag` field.
    fn first_text(&self, tag: &'a [u8; 3], code: u8) -> Option<String> {
        self.first(tag).and_then(|field| field.text(code))
    }
}

impl<'a> DataField<'a> {
    fn values(self, code: u8) -> impl Iterator<Item = &'a [u8]> {
        self.subfields
            .iter()
            .filter(move |subfield| subfield.code == code)
            .map(|subfield| subfield.value.as_slice())
    }

    /// The cleaned first `$code`, if any.
    fn text(self, code: u8) -> Option<String> {
        self.values(code).find_map(text::clean)
    }

    /// The first `$code` exactly as written, trimmed of spaces.
    fn raw(self, code: u8) -> Option<String> {
        self.values(code).find_map(text::trimmed)
    }

    /// Trimmed values of the listed codes, in field order, joined by a space, punctuation kept.
    fn raw_joined(self, codes: &[u8]) -> Option<String> {
        let parts: Vec<String> = self
            .subfields
            .iter()
            .filter(|subfield| codes.contains(&subfield.code))
            .filter_map(|subfield| text::trimmed(&subfield.value))
            .collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    /// Cleaned values of the listed codes, in field order, joined by `separator`.
    fn joined(self, codes: &[u8], separator: &str) -> Option<String> {
        let parts: Vec<String> = self
            .subfields
            .iter()
            .filter(|subfield| codes.contains(&subfield.code))
            .filter_map(|subfield| text::clean(&subfield.value))
            .collect();
        (!parts.is_empty()).then(|| parts.join(separator))
    }
}
