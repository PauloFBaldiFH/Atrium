// SPDX-License-Identifier: GPL-3.0-or-later
//! Import → export round trips over the oracle's staff export
//! (docs/specs/catalogo/mapeamento-marc21.md, §7; vision A.4, item 3).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_app::catalog::{ItemData, NewRecord};
use elzevir_app::marc_mapping::{bibframe_to_marc, marc_to_bibframe};
use elzevir_marc::{Field, Limits, Record, RecordRef};

const STAFF_EXPORT: &[u8] = include_bytes!("data/koha-staff-bibs-1-200.mrc");

/// Raw bytes and the decoded record of each exported record.
fn records() -> Vec<(Vec<u8>, Record)> {
    STAFF_EXPORT
        .split_inclusive(|&b| b == 0x1d)
        .filter(|raw| raw.len() > 1)
        .map(|raw| {
            let mut record = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
            record.marc21_to_utf8();
            (raw.to_vec(), record)
        })
        .collect()
}

fn iso2709(record: &Record) -> Vec<u8> {
    let mut out = Vec::new();
    record.write_iso2709(&mut out).unwrap();
    out
}

fn tags(record: &Record, tag: [u8; 3]) -> Vec<Field> {
    record
        .fields
        .iter()
        .filter(|field| *field.tag().as_bytes() == tag)
        .cloned()
        .collect()
}

fn without(record: &Record, tag: [u8; 3]) -> Vec<Field> {
    record
        .fields
        .iter()
        .filter(|field| *field.tag().as_bytes() != tag)
        .cloned()
        .collect()
}

fn model(record: &Record) -> NewRecord {
    marc_to_bibframe(record, None).unwrap()
}

#[test]
fn unedited_records_come_back_byte_for_byte() {
    for (position, (raw, record)) in records().iter().enumerate() {
        let exported = bibframe_to_marc(&model(record), Some(record));
        assert_eq!(&exported, record, "record {position}");
        assert_eq!(
            &iso2709(&exported),
            raw,
            "record {position}: ISO 2709 bytes"
        );
    }
}

#[test]
fn a_record_written_from_scratch_maps_back_to_the_same_model() {
    for (position, (_, record)) in records().iter().enumerate() {
        let original = model(record);
        let written = bibframe_to_marc(&original, None);
        let again = marc_to_bibframe(&written, None).unwrap();
        assert_eq!(again, original, "record {position}");
    }
}

#[test]
fn editing_the_title_touches_only_the_245() {
    let (_, record) = &records()[18];
    let mut edited = model(record);
    edited.instance.title = "A presença inquietante : edição revista".to_owned();
    let exported = bibframe_to_marc(&edited, Some(record));
    assert_eq!(without(&exported, *b"245"), without(record, *b"245"));
    assert_eq!(model(&exported).instance.title, edited.instance.title);
    // The statement of responsibility ($c) stays.
    let Field::Data { subfields, .. } = &tags(&exported, *b"245")[0] else {
        panic!("245 is a data field");
    };
    assert!(subfields.iter().any(|s| s.code == b'c'));
}

#[test]
fn editing_one_item_keeps_the_others_and_koha_subfields() {
    let records = records();
    let (position, (_, record)) = records
        .iter()
        .enumerate()
        .find(|(_, (_, r))| tags(r, *b"952").len() >= 3)
        .unwrap();
    let mut edited = model(record);
    edited.items[1].call_number = Some("320 X99".to_owned());
    let exported = bibframe_to_marc(&edited, Some(record));
    let (before, after) = (tags(record, *b"952"), tags(&exported, *b"952"));
    assert_eq!(after.len(), before.len(), "record {position}");
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2..], before[2..]);
    let (Field::Data { subfields: old, .. }, Field::Data { subfields: new, .. }) =
        (&before[1], &after[1])
    else {
        panic!("952 is a data field");
    };
    let changed: Vec<u8> = old
        .iter()
        .zip(new)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.code)
        .collect();
    assert_eq!(changed, [b'o'], "only $o changes; $9 and $6 stay");
    assert_eq!(without(&exported, *b"952"), without(record, *b"952"));
}

#[test]
fn a_new_item_becomes_a_new_952_after_the_others() {
    let (_, record) = &records()[0];
    let mut edited = model(record);
    let mut item: ItemData = edited.items[0].clone();
    item.barcode = Some("NOVO-1".to_owned());
    item.inventory_number = Some("2026/0001".to_owned());
    edited.items.push(item.clone());
    let exported = bibframe_to_marc(&edited, Some(record));
    let items = tags(&exported, *b"952");
    assert_eq!(items.len(), tags(record, *b"952").len() + 1);
    assert_eq!(model(&exported).items.last(), Some(&item));
    assert!(matches!(exported.fields.last(), Some(f) if f.tag().as_bytes() >= b"952"));
}

#[test]
fn accents_and_call_numbers_survive_an_edit_elsewhere() {
    for (position, (_, record)) in records().iter().enumerate() {
        let mut edited = model(record);
        edited.summary = Some("Resumo acrescentado à mão".to_owned());
        let exported = bibframe_to_marc(&edited, Some(record));
        for tag in [b"082", b"090", b"245", b"100", b"650", b"952"] {
            assert_eq!(
                tags(&exported, *tag),
                tags(record, *tag),
                "record {position}, {tag:?}"
            );
        }
        assert_eq!(model(&exported).summary, edited.summary);
    }
}

#[test]
fn editing_a_list_keeps_untouched_fields_with_their_authority_links() {
    let (_, record) = &records()[18];
    let mut edited = model(record);
    // Drop the first subject and the main author's role; the added author keeps its links.
    edited.subjects.remove(0);
    edited.contributions[0].role = Some("edt".to_owned());
    let exported = bibframe_to_marc(&edited, Some(record));
    assert_eq!(tags(&exported, *b"650"), tags(record, *b"650")[1..]);
    assert_eq!(tags(&exported, *b"700"), tags(record, *b"700"));
    let again = model(&exported);
    assert_eq!(again.subjects, edited.subjects);
    assert_eq!(again.contributions, edited.contributions);
}

#[test]
fn notes_map_both_ways_and_an_edited_note_touches_only_its_field() {
    use elzevir_app::catalog::NewNote;
    use elzevir_domain::catalog::NoteKind;

    let records = records();
    let with_notes = records
        .iter()
        .filter(|(_, record)| !model(record).instance.notes.is_empty())
        .count();
    assert!(with_notes > 0, "the staff export has notes");
    // Record 147 has a contents note with "Karl Marx" (docs/specs/catalogo/busca.md, §4).
    let (_, record) = &records[146];
    let mut edited = model(record);
    let contents = edited
        .instance
        .notes
        .iter_mut()
        .find(|note| note.kind == NoteKind::Contents)
        .unwrap();
    assert!(contents.text.contains("Marx"));
    contents.text = "Nota de conteúdo reescrita.".to_owned();
    edited.instance.notes.push(NewNote {
        kind: NoteKind::Local,
        text: "Doação da família.".to_owned(),
    });
    let exported = bibframe_to_marc(&edited, Some(record));
    for tag in [*b"500", *b"505", *b"590"] {
        let others = |r: &Record| {
            r.fields
                .iter()
                .filter(|f| ![*b"500", *b"505", *b"590"].contains(f.tag().as_bytes()))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            others(&exported),
            others(record),
            "only notes change ({tag:?})"
        );
    }
    assert_eq!(tags(&exported, *b"500"), tags(record, *b"500"));
    assert_eq!(model(&exported).instance.notes, edited.instance.notes);
}
