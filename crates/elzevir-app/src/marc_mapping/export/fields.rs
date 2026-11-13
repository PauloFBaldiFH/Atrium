// SPDX-License-Identifier: GPL-3.0-or-later
//! Small edits on an owned record, used by the export.

use elzevir_marc::{Field, Record, Subfield, Tag};

/// A data field from `(code, value)` pairs.
pub(super) fn data_field(tag: [u8; 3], indicators: [u8; 2], subfields: &[(u8, &str)]) -> Field {
    Field::Data {
        tag: Tag::new(tag),
        indicators,
        subfields: subfields
            .iter()
            .map(|(code, value)| Subfield {
                code: *code,
                value: value.as_bytes().to_vec(),
            })
            .collect(),
    }
}

/// Index of the first field with `tag`.
pub(super) fn first_index(record: &Record, tag: [u8; 3]) -> Option<usize> {
    record
        .fields
        .iter()
        .position(|field| *field.tag().as_bytes() == tag)
}

/// Inserts `fields` before the first field with a greater tag (all with the first's tag).
pub(super) fn insert_in_tag_order(record: &mut Record, fields: Vec<Field>) {
    let Some(tag) = fields.first().map(Field::tag) else {
        return;
    };
    let at = record
        .fields
        .iter()
        .position(|field| field.tag().as_bytes() > tag.as_bytes())
        .unwrap_or(record.fields.len());
    record.fields.splice(at..at, fields);
}

/// Sets `$code` of the first `tag` field to `value`, replacing every `$code` there.
///
/// `None` removes `$code`; a field left without subfields is removed. A missing field is
/// created with `indicators`, in tag order, only when there is a value to write.
pub(super) fn set_subfield(
    record: &mut Record,
    tag: [u8; 3],
    indicators: [u8; 2],
    code: u8,
    value: Option<&str>,
) {
    let Some(index) = first_index(record, tag) else {
        if let Some(value) = value {
            insert_in_tag_order(record, vec![data_field(tag, indicators, &[(code, value)])]);
        }
        return;
    };
    let Some(Field::Data { subfields, .. }) = record.fields.get_mut(index) else {
        return;
    };
    let position = subfields.iter().position(|subfield| subfield.code == code);
    subfields.retain(|subfield| subfield.code != code);
    if let Some(value) = value {
        let subfield = Subfield {
            code,
            value: value.as_bytes().to_vec(),
        };
        let at = position.unwrap_or(subfields.len()).min(subfields.len());
        subfields.insert(at, subfield);
    }
    if subfields.is_empty() {
        record.fields.remove(index);
    }
}
