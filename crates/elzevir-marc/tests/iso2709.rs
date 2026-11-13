// SPDX-License-Identifier: GPL-3.0-or-later
//! ISO 2709 reading: valid records, every rejection path, resynchronization, and
//! property tests that arbitrary input never panics and valid records round-trip.
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_marc::{Encoding, Limits, MarcReader, ReadError, RecordError, RecordRef, Tag};
use proptest::prelude::*;

/// Builds one ISO 2709 record from `(tag, field bytes without terminator)` pairs.
/// `coding` is leader position 9.
fn build(coding: u8, fields: &[(&[u8; 3], Vec<u8>)]) -> Vec<u8> {
    let mut directory = Vec::new();
    let mut data = Vec::new();
    for (tag, body) in fields {
        let start = data.len();
        data.extend_from_slice(body);
        data.push(0x1E);
        directory.extend_from_slice(*tag);
        directory.extend_from_slice(format!("{:04}{:05}", body.len() + 1, start).as_bytes());
    }
    directory.push(0x1E);
    let base = 24 + directory.len();
    let len = base + data.len() + 1;
    let mut record = format!("{len:05}nam {}22{base:05}   4500", char::from(coding)).into_bytes();
    record.extend_from_slice(&directory);
    record.extend_from_slice(&data);
    record.push(0x1D);
    record
}

/// A data field body: indicators, then `$code value` pairs.
fn data_field(indicators: [u8; 2], subfields: &[(u8, &str)]) -> Vec<u8> {
    let mut body = indicators.to_vec();
    for (code, value) in subfields {
        body.push(0x1F);
        body.push(*code);
        body.extend_from_slice(value.as_bytes());
    }
    body
}

fn sample() -> Vec<u8> {
    build(
        b'a',
        &[
            (b"001", b"000123".to_vec()),
            (
                b"245",
                data_field(
                    *b"10",
                    &[(b'a', "Dom Casmurro /"), (b'c', "Machado de Assis.")],
                ),
            ),
            (b"650", data_field(*b" 4", &[(b'a', "Ficção brasileira")])),
            (
                b"650",
                data_field(*b" 4", &[(b'a', "Romance"), (b'x', "Século XIX")]),
            ),
        ],
    )
}

fn read_all(input: &[u8], limits: Limits) -> Vec<Result<usize, String>> {
    let mut reader = MarcReader::new(input, limits);
    let mut out = Vec::new();
    while let Some(result) = reader.next_record() {
        out.push(result.map(|r| r.field_count()).map_err(|e| e.to_string()));
    }
    out
}

fn record_error(input: &[u8], limits: &Limits) -> RecordError {
    RecordRef::parse(input, limits).unwrap_err()
}

#[test]
fn reads_control_and_data_fields() {
    let raw = sample();
    let record = RecordRef::parse(&raw, &Limits::default()).unwrap();
    assert_eq!(record.encoding(), Encoding::Utf8);
    assert_eq!(record.field_count(), 4);

    let control = record.fields().next().unwrap();
    assert_eq!(control.tag(), Tag::new(*b"001"));
    assert_eq!(control.raw(), b"000123");
    assert_eq!(control.indicators(), None);
    assert_eq!(control.subfields().count(), 0);

    let title = record.fields_with_tag(Tag::new(*b"245")).next().unwrap();
    assert_eq!(title.indicators(), Some(*b"10"));
    let subfields: Vec<_> = title.subfields().collect();
    assert_eq!(
        subfields,
        [
            (b'a', &b"Dom Casmurro /"[..]),
            (b'c', &b"Machado de Assis."[..])
        ]
    );

    let subjects: Vec<_> = record
        .fields_with_tag(Tag::new(*b"650"))
        .flat_map(|f| f.subfield_values(b'a').collect::<Vec<_>>())
        .map(|v| std::str::from_utf8(v).unwrap())
        .collect();
    assert_eq!(subjects, ["Ficção brasileira", "Romance"]);
}

#[test]
fn legacy_coding_is_reported() {
    let raw = build(b' ', &[(b"001", b"1".to_vec())]);
    let record = RecordRef::parse(&raw, &Limits::default()).unwrap();
    assert_eq!(record.encoding(), Encoding::Legacy);
}

#[test]
fn reads_a_stream_and_ignores_a_trailing_newline() {
    let mut input = sample();
    input.extend(sample());
    input.extend(b"\r\n");
    assert_eq!(read_all(&input, Limits::default()), [Ok(4), Ok(4)]);
}

#[test]
fn a_bad_record_is_reported_and_the_next_one_is_read() {
    let mut broken = sample();
    *broken.first_mut().unwrap() = b'9'; // declared length no longer matches
    let mut input = sample();
    input.extend(&broken);
    input.extend(sample());

    let mut reader = MarcReader::new(&input[..], Limits::default());
    assert!(reader.next_record().unwrap().is_ok());
    match reader.next_record().unwrap() {
        Err(ReadError::Record {
            index,
            offset,
            error,
        }) => {
            assert_eq!(index, 1);
            assert_eq!(offset, sample().len() as u64);
            assert!(matches!(error, RecordError::LengthMismatch { .. }));
        }
        other => panic!("expected a record error, got {other:?}"),
    }
    assert!(reader.next_record().unwrap().is_ok());
    assert!(reader.next_record().is_none());
}

#[test]
fn input_ending_inside_a_record_is_truncated() {
    let raw = sample();
    let cut = &raw[..raw.len() - 10];
    let results = read_all(cut, Limits::default());
    assert_eq!(results.len(), 1);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .contains("input ended inside a record")
    );
}

#[test]
fn an_oversized_record_is_skipped_without_buffering_it() {
    let limits = Limits {
        max_record_len: 64,
        ..Limits::default()
    };
    let big = build(
        b'a',
        &[(b"500", data_field(*b"  ", &[(b'a', &"x".repeat(500))]))],
    );
    let small = build(b'a', &[(b"001", b"1".to_vec())]);
    assert!(small.len() <= 64);
    let mut input = big;
    input.extend(&small);

    let results = read_all(&input, limits);
    assert_eq!(results.len(), 2);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .contains("exceeds 64 bytes")
    );
    assert_eq!(results[1], Ok(1));
}

#[test]
fn too_many_fields_is_rejected() {
    let fields: Vec<_> = (0..11)
        .map(|_| (b"500", data_field(*b"  ", &[(b'a', "n")])))
        .collect();
    let raw = build(b'a', &fields);
    let limits = Limits {
        max_fields: 10,
        ..Limits::default()
    };
    assert_eq!(
        record_error(&raw, &limits),
        RecordError::TooManyFields {
            count: 11,
            limit: 10
        }
    );
}

#[test]
fn every_structural_defect_has_its_own_error() {
    let limits = Limits::default();
    let good = sample();

    assert_eq!(
        record_error(b"00010\x1d", &limits),
        RecordError::TooShort { len: 6 }
    );

    let mut no_terminator = good.clone();
    *no_terminator.last_mut().unwrap() = b'x';
    assert!(matches!(
        record_error(&no_terminator, &limits),
        RecordError::Truncated { .. }
    ));

    let mut letters_in_length = good.clone();
    letters_in_length[2] = b'X';
    assert_eq!(
        record_error(&letters_in_length, &limits),
        RecordError::LeaderNotNumeric { start: 0, end: 4 }
    );

    let mut base_past_end = good.clone();
    base_past_end[12..17].copy_from_slice(b"99999");
    assert!(matches!(
        record_error(&base_past_end, &limits),
        RecordError::BadBaseAddress { .. }
    ));

    // Field 245's start position points past the data area.
    let mut field_out_of_bounds = good.clone();
    let entry = 24 + 12; // second directory entry
    field_out_of_bounds[entry + 7..entry + 12].copy_from_slice(b"90000");
    assert_eq!(
        record_error(&field_out_of_bounds, &limits),
        RecordError::BadField {
            index: 1,
            tag: Tag::new(*b"245")
        }
    );

    let mut letters_in_entry = good;
    letters_in_entry[entry + 3] = b'?';
    assert_eq!(
        record_error(&letters_in_entry, &limits),
        RecordError::EntryNotNumeric {
            index: 1,
            tag: Tag::new(*b"245")
        }
    );
}

fn tag_strategy() -> impl Strategy<Value = [u8; 3]> {
    proptest::array::uniform3(b'0'..=b'9')
}

fn field_strategy() -> impl Strategy<Value = ([u8; 3], Vec<u8>)> {
    (
        tag_strategy(),
        proptest::collection::vec(
            any::<u8>().prop_filter("no ISO 2709 separators", |b| !matches!(b, 0x1D | 0x1E)),
            0..60,
        ),
    )
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2_000)) {
        let _ = read_all(&bytes, Limits::default());
        // Only the absence of a panic is under test; the result itself is irrelevant.
        let _result = RecordRef::parse(&bytes, &Limits::default());
    }

    #[test]
    fn mutated_records_never_panic(index in 0_usize..400, byte in any::<u8>()) {
        let mut raw = sample();
        let at = index % raw.len();
        raw[at] = byte;
        if let Ok(record) = RecordRef::parse(&raw, &Limits::default()) {
            for field in record.fields() {
                let _ = field.subfields().count();
            }
        }
    }

    #[test]
    fn valid_records_round_trip(fields in proptest::collection::vec(field_strategy(), 0..30)) {
        let refs: Vec<(&[u8; 3], Vec<u8>)> = fields.iter().map(|(t, b)| (t, b.clone())).collect();
        let raw = build(b'a', &refs);
        let record = RecordRef::parse(&raw, &Limits::default()).unwrap();
        let read: Vec<([u8; 3], Vec<u8>)> = record
            .fields()
            .map(|f| (*f.tag().as_bytes(), f.raw().to_vec()))
            .collect();
        prop_assert_eq!(read, fields);
    }
}
