// SPDX-License-Identifier: GPL-3.0-or-later
//! `bibframe_to_marc`: the model back to MARC21 (docs/specs/catalogo/mapeamento-marc21.md, §7).
//!
//! The original record is the starting point. Only the parts of the model that differ from
//! what the original maps to are rewritten, and a rewritten list reuses every original field
//! whose mapping did not change. An unedited record therefore comes back byte for byte.

use elzevir_domain::catalog::{AgentKind, IdentifierKind, MaterialType, NoteKind, SubjectKind};
use elzevir_marc::{Field, Record, Subfield, Tag};

use super::instance::{IDENTIFIER_TAGS, NOTE_TAGS, identifier_of, note_of};
use super::items::{defaults, item_of};
use super::work::{CONTRIBUTION_TAGS, SUBJECT_TAGS, contribution_of, subject_of};
use super::{DataField, Marc, marc_to_bibframe};
use crate::catalog::{ItemData, NewContribution, NewIdentifier, NewNote, NewRecord, NewSubject};

mod fields;

use fields::{data_field, first_index, insert_in_tag_order, set_subfield};

/// Writes a model record as MARC21, starting from `original` when there is one.
///
/// Not written in this version: Hub and relations between Works (spec §6).
#[must_use]
pub fn bibframe_to_marc(model: &NewRecord, original: Option<&Record>) -> Record {
    let before = original.and_then(|record| marc_to_bibframe(record, None).ok());
    let mut out = original.cloned().unwrap_or_else(|| Record {
        leader: *b"00000nam a2200000 i 4500",
        fields: Vec::new(),
    });
    let changed = |part: &dyn Fn(&NewRecord) -> bool| before.as_ref().is_none_or(|b| !part(b));
    if changed(&|b| b.material_type == model.material_type) {
        set_material_type(&mut out, model.material_type);
    }
    if changed(&|b| b.instance.material_type == model.instance.material_type) {
        set_instance_material_type(&mut out, model.instance.material_type);
    }
    write_titles(&mut out, model, before.as_ref());
    write_publication(&mut out, model, before.as_ref());
    write_lists(&mut out, model, before.as_ref());
    out
}

fn set_material_type(out: &mut Record, material_type: MaterialType) {
    let (record_type, level) = match material_type {
        MaterialType::Book => (b'a', b'm'),
        MaterialType::Periodical => (b'a', b's'),
        MaterialType::Map => (b'e', b'm'),
        MaterialType::Score => (b'c', b'm'),
        MaterialType::SoundRecording => (b'j', b'm'),
        MaterialType::Video => (b'g', b'm'),
        MaterialType::Digital => (b'm', b'm'),
        MaterialType::VisualMaterial => (b'k', b'm'),
        MaterialType::Mixed => (b'p', b'c'),
    };
    if let Some([six, seven]) = out
        .leader
        .get_mut(6..8)
        .and_then(|s| <&mut [u8; 2]>::try_from(s).ok())
    {
        *six = record_type;
        *seven = level;
    }
}

/// An electronic Instance gets a `007` for a remote electronic resource when it has none.
fn set_instance_material_type(out: &mut Record, material_type: Option<MaterialType>) {
    let has_007 = out.fields.iter().any(|f| f.tag().as_bytes() == b"007");
    if material_type == Some(MaterialType::Digital) && !has_007 {
        insert_in_tag_order(
            out,
            vec![Field::Control {
                tag: Tag::new(*b"007"),
                value: b"cr".to_vec(),
            }],
        );
    }
}

fn write_titles(out: &mut Record, model: &NewRecord, before: Option<&NewRecord>) {
    let instance = &model.instance;
    if before.is_none_or(|b| b.instance.title != instance.title) {
        let (main, rest) = match instance.title.split_once(" : ") {
            Some((main, rest)) => (main, Some(rest)),
            None => (instance.title.as_str(), None),
        };
        set_subfield(out, *b"245", *b"10", b'a', Some(main));
        set_subfield(out, *b"245", *b"10", b'b', rest);
        set_subfield(out, *b"245", *b"10", b'n', None);
        set_subfield(out, *b"245", *b"10", b'p', None);
    }
    if before.is_none_or(|b| b.work_title != model.work_title) {
        let main_title = instance.title.split(" : ").next().unwrap_or_default();
        let tag = if first_index(out, *b"130").is_some() {
            Some(*b"130")
        } else if first_index(out, *b"240").is_some() || model.work_title != main_title {
            Some(*b"240")
        } else {
            None
        };
        if let Some(tag) = tag {
            set_subfield(out, tag, *b"10", b'a', Some(&model.work_title));
        }
    }
    let scalar =
        |part: fn(&NewRecord) -> &Option<String>| before.is_none_or(|b| part(b) != part(model));
    if scalar(|r| &r.instance.edition) {
        set_subfield(out, *b"250", *b"  ", b'a', instance.edition.as_deref());
    }
    if scalar(|r| &r.summary) {
        set_subfield(out, *b"520", *b"  ", b'a', model.summary.as_deref());
    }
    if scalar(|r| &r.instance.extent) {
        set_subfield(out, *b"300", *b"  ", b'a', instance.extent.as_deref());
        set_subfield(out, *b"300", *b"  ", b'b', None);
        set_subfield(out, *b"300", *b"  ", b'c', None);
    }
    if scalar(|r| &r.instance.series) {
        let (name, number) = match instance
            .series
            .as_deref()
            .map(|s| s.rsplit_once(" ; ").unwrap_or((s, "")))
        {
            Some((name, number)) => (Some(name), (!number.is_empty()).then_some(number)),
            None => (None, None),
        };
        set_subfield(out, *b"490", *b"0 ", b'a', name);
        set_subfield(out, *b"490", *b"0 ", b'v', number);
    }
    if before.is_none_or(|b| {
        (&b.classification, &b.classification_scheme)
            != (&model.classification, &model.classification_scheme)
    }) {
        let tag = match model.classification_scheme.as_deref() {
            Some("udc") => *b"080",
            Some("lcc") => *b"050",
            _ => *b"082",
        };
        set_subfield(out, tag, *b"04", b'a', model.classification.as_deref());
    }
}

fn write_publication(out: &mut Record, model: &NewRecord, before: Option<&NewRecord>) {
    let instance = &model.instance;
    let parts = |r: &NewRecord| {
        (
            r.instance.place.clone(),
            r.instance.publisher.clone(),
            r.instance.date.clone(),
        )
    };
    if before.is_some_and(|b| parts(b) == parts(model)) {
        return;
    }
    let tag = if out.fields.iter().any(|f| matches!(f, Field::Data { tag, indicators, .. } if tag.as_bytes() == b"264" && indicators[1] == b'1')) || first_index(out, *b"260").is_none() {
        *b"264"
    } else {
        *b"260"
    };
    let indicators = if tag == *b"264" { *b" 1" } else { *b"  " };
    set_subfield(out, tag, indicators, b'a', instance.place.as_deref());
    set_subfield(out, tag, indicators, b'b', instance.publisher.as_deref());
    set_subfield(out, tag, indicators, b'c', instance.date.as_deref());
}

fn write_lists(out: &mut Record, model: &NewRecord, before: Option<&NewRecord>) {
    if before.is_none_or(|b| b.languages != model.languages) {
        write_languages(out, &model.languages);
    }
    if before.is_none_or(|b| b.contributions != model.contributions) {
        let current = mapped(out, &CONTRIBUTION_TAGS, |tag, field| {
            contribution_of(tag, field)
        });
        rebuild(
            out,
            &current,
            &model.contributions,
            contribution_field,
            |_, _| None,
        );
    }
    if before.is_none_or(|b| b.subjects != model.subjects) {
        let current = mapped(out, &SUBJECT_TAGS, subject_of);
        rebuild(out, &current, &model.subjects, subject_field, |_, _| None);
    }
    if before.is_none_or(|b| b.instance.identifiers != model.instance.identifiers) {
        let current = mapped(out, &IDENTIFIER_TAGS, |tag, field| {
            identifier_of(tag, field)
        });
        rebuild(
            out,
            &current,
            &model.instance.identifiers,
            identifier_field,
            |_, _| None,
        );
    }
    if before.is_none_or(|b| b.instance.notes != model.instance.notes) {
        let current = mapped(out, &NOTE_TAGS, note_of);
        rebuild(out, &current, &model.instance.notes, note_field, |_, _| {
            None
        });
    }
    if before.is_none_or(|b| b.items != model.items) {
        let defaults = defaults(&Marc(out));
        let mut position = 0;
        let current = mapped(out, &[b"952"], |_, field| {
            let item = item_of(position, field, &defaults).ok();
            position += 1;
            item
        });
        rebuild(
            out,
            &current,
            &model.items,
            |item| item_field(item, None),
            |item, current| {
                // A changed Item keeps the subfields Koha maintains (`$9`, `$6`...) of the
                // original field with the same barcode.
                item.barcode.as_ref().and_then(|barcode| {
                    current
                        .iter()
                        .find(|(_, original, _)| original.barcode.as_ref() == Some(barcode))
                        .map(|(_, _, field)| item_field(item, Some(field)))
                })
            },
        );
    }
}

fn write_languages(out: &mut Record, languages: &[String]) {
    let first = first_index(out, *b"041");
    let kept: Vec<Subfield> = first
        .and_then(|index| match out.fields.get(index) {
            Some(Field::Data { subfields, .. }) => Some(
                subfields
                    .iter()
                    .filter(|s| s.code != b'a')
                    .cloned()
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    out.fields.retain(|field| field.tag().as_bytes() != b"041");
    if languages.is_empty() {
        return;
    }
    let mut subfields: Vec<Subfield> = languages
        .iter()
        .map(|code| Subfield {
            code: b'a',
            value: code.as_bytes().to_vec(),
        })
        .collect();
    subfields.extend(kept);
    let field = Field::Data {
        tag: Tag::new(*b"041"),
        indicators: *b"0 ",
        subfields,
    };
    match first {
        Some(index) if index <= out.fields.len() => out.fields.insert(index, field),
        _ => insert_in_tag_order(out, vec![field]),
    }
}

/// The fields with one of `tags` that map to a value, with their index and a copy.
fn mapped<T>(
    out: &Record,
    tags: &[&[u8; 3]],
    mut map: impl FnMut([u8; 3], DataField<'_>) -> Option<T>,
) -> Vec<(usize, T, Field)> {
    out.fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            let Field::Data {
                tag,
                indicators,
                subfields,
            } = field
            else {
                return None;
            };
            if !tags.contains(&tag.as_bytes()) {
                return None;
            }
            map(
                *tag.as_bytes(),
                DataField {
                    indicators: *indicators,
                    subfields,
                },
            )
            .map(|value| (index, value, field.clone()))
        })
        .collect()
}

/// Replaces the mapped fields with one field per wanted value: an unchanged original field
/// when one maps to exactly that value, else `changed` (from a related original), else `new`.
fn rebuild<T: PartialEq>(
    out: &mut Record,
    current: &[(usize, T, Field)],
    wanted: &[T],
    new: impl Fn(&T) -> Field,
    changed: impl Fn(&T, &[(usize, T, Field)]) -> Option<Field>,
) {
    let mut used = vec![false; current.len()];
    let fields: Vec<Field> = wanted
        .iter()
        .map(|value| {
            let reusable = current
                .iter()
                .zip(used.iter_mut())
                .find(|((_, original, _), used)| !**used && original == value);
            match reusable {
                Some(((_, _, field), used)) => {
                    *used = true;
                    field.clone()
                }
                None => changed(value, current).unwrap_or_else(|| new(value)),
            }
        })
        .collect();
    let insert_at = current.first().map(|(index, _, _)| *index);
    for (index, _, _) in current.iter().rev() {
        if *index < out.fields.len() {
            out.fields.remove(*index);
        }
    }
    match insert_at {
        Some(index) if index <= out.fields.len() => {
            out.fields.splice(index..index, fields);
        }
        _ => insert_in_tag_order(out, fields),
    }
}

fn contribution_field(contribution: &NewContribution) -> Field {
    let (main, added, first_indicator) = match contribution.kind {
        AgentKind::Person => (*b"100", *b"700", b'1'),
        AgentKind::Family => (*b"100", *b"700", b'3'),
        AgentKind::Organization => (*b"110", *b"710", b'2'),
        AgentKind::Meeting => (*b"111", *b"711", b'2'),
    };
    let tag = if contribution.primary { main } else { added };
    let mut subfields = vec![(b'a', contribution.heading.as_str())];
    if let Some(role) = contribution.role.as_deref() {
        let code = role.len() == 3 && role.bytes().all(|b| b.is_ascii_lowercase());
        subfields.push((if code { b'4' } else { b'e' }, role));
    }
    data_field(tag, [first_indicator, b' '], &subfields)
}

fn subject_field(subject: &NewSubject) -> Field {
    let (tag, first_indicator) = match subject.kind {
        SubjectKind::Agent => (*b"600", b'1'),
        SubjectKind::Title => (*b"630", b'0'),
        SubjectKind::Temporal => (*b"648", b' '),
        SubjectKind::Topic => (*b"650", b' '),
        SubjectKind::Place => (*b"651", b' '),
        SubjectKind::GenreForm => (*b"655", b' '),
    };
    let (second_indicator, source) = match subject.thesaurus.as_deref() {
        None => (b'4', None),
        Some("lcsh") => (b'0', None),
        Some("lcac") => (b'1', None),
        Some("mesh") => (b'2', None),
        Some("nal") => (b'3', None),
        Some("cash") => (b'5', None),
        Some("rvm") => (b'6', None),
        Some(other) => (b'7', Some(other)),
    };
    let mut parts = subject.heading.split(" -- ");
    let mut subfields: Vec<(u8, &str)> =
        parts.next().map(|main| (b'a', main)).into_iter().collect();
    subfields.extend(parts.map(|part| (b'x', part)));
    subfields.extend(source.map(|source| (b'2', source)));
    data_field(tag, [first_indicator, second_indicator], &subfields)
}

fn identifier_field(identifier: &NewIdentifier) -> Field {
    let (tag, indicators, source) = match identifier.kind {
        IdentifierKind::Isbn => (*b"020", *b"  ", None),
        IdentifierKind::Issn => (*b"022", *b"  ", None),
        IdentifierKind::Lccn => (*b"010", *b"  ", None),
        IdentifierKind::Ean => (*b"024", *b"3 ", None),
        IdentifierKind::Ismn => (*b"024", *b"2 ", None),
        IdentifierKind::Doi => (*b"024", *b"7 ", Some("doi")),
        IdentifierKind::SystemControlNumber => (*b"035", *b"  ", None),
    };
    let mut subfields = vec![(b'a', identifier.value.as_str())];
    subfields.extend(identifier.qualifier.as_deref().map(|q| (b'q', q)));
    subfields.extend(source.map(|s| (b'2', s)));
    data_field(tag, indicators, &subfields)
}

fn note_field(note: &NewNote) -> Field {
    let (tag, indicators) = match note.kind {
        NoteKind::General => (*b"500", *b"  "),
        // First indicator 0: the contents are complete.
        NoteKind::Contents => (*b"505", *b"0 "),
        NoteKind::Local => (*b"590", *b"  "),
    };
    data_field(tag, indicators, &[(b'a', note.text.as_str())])
}

/// A `952` for `item`: the mapped subfields set on `base` (or on a new field).
fn item_field(item: &ItemData, base: Option<&Field>) -> Field {
    let money = |m: Option<elzevir_domain::catalog::Money>| {
        m.map(|m| format!("{}.{:02}", m.0 / 100, m.0 % 100))
    };
    let number = |n: i64| (n != 0).then(|| n.to_string());
    let values: [(u8, Option<String>); 17] = [
        (b'0', number(item.status.withdrawn)),
        (b'1', number(item.status.lost)),
        (b'4', number(item.status.damaged)),
        (b'5', number(item.status.restricted)),
        (b'7', number(item.status.not_for_loan)),
        (b'8', item.collection.clone()),
        (b'a', Some(item.home_branch.clone())),
        (b'b', Some(item.holding_branch.clone())),
        (b'c', item.location.clone()),
        (b'd', item.acquired_on.clone()),
        (b'g', money(item.price)),
        (b'i', item.inventory_number.clone()),
        (b'o', item.call_number.clone()),
        (b'p', item.barcode.clone()),
        (b't', item.copy_number.clone()),
        (b'v', money(item.replacement_price)),
        (b'y', Some(item.item_type.clone())),
    ];
    let mut record = Record {
        leader: [b' '; 24],
        fields: vec![
            base.cloned()
                .unwrap_or_else(|| data_field(*b"952", *b"  ", &[])),
        ],
    };
    for (code, value) in &values {
        let keep_zero = base.is_some() && matches!(code, b'0' | b'1' | b'4' | b'5' | b'7');
        let value = match value {
            None if keep_zero => Some("0".to_owned()),
            other => other.clone(),
        };
        set_subfield(&mut record, *b"952", *b"  ", *code, value.as_deref());
    }
    record
        .fields
        .pop()
        .unwrap_or_else(|| data_field(*b"952", *b"  ", &[]))
}
