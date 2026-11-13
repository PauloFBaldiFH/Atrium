// SPDX-License-Identifier: GPL-3.0-or-later
//! ISO 2709 and MARCXML writing and atomic publication (docs/specs/marc/escrita.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_marc::{
    Field, Limits, MARCXML_COLLECTION_FOOTER, MARCXML_COLLECTION_HEADER, Record, RecordRef,
    Subfield, Tag, WriteError, publish_atomically,
};
use proptest::prelude::*;

const LEADER: [u8; 24] = *b"00000nam a2200000 a 4500";

fn data(tag: [u8; 3], indicators: [u8; 2], subfields: &[(u8, &str)]) -> Field {
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

fn control(tag: [u8; 3], value: &str) -> Field {
    Field::Control {
        tag: Tag::new(tag),
        value: value.as_bytes().to_vec(),
    }
}

fn iso2709(record: &Record) -> Vec<u8> {
    let mut out = Vec::new();
    record.write_iso2709(&mut out).unwrap();
    out
}

#[test]
fn empty_record_has_base_25_and_length_26() {
    let record = Record {
        leader: *b"?????nam a??????? a ????",
        fields: Vec::new(),
    };
    assert_eq!(iso2709(&record), b"00026nam a2200025 a 4500\x1e\x1d");
}

#[test]
fn fields_directory_and_leader_follow_marc_record() {
    let record = Record {
        leader: LEADER,
        fields: vec![
            control(*b"001", "42"),
            data(*b"245", *b"10", &[(b'a', "Title"), (b'c', "Me")]),
            data(*b"500", *b"  ", &[]),
        ],
    };
    let expected = b"00082nam a2200061 a 4500\
        001000300000245001400003500000300017\x1e\
        42\x1e10\x1faTitle\x1fcMe\x1e  \x1e\x1d";
    assert_eq!(iso2709(&record), expected);
}

#[test]
fn oversized_or_delimited_records_are_refused_and_output_untouched() {
    let long = "x".repeat(9_997);
    let mut out = b"previous".to_vec();
    let too_long_field = Record {
        leader: LEADER,
        fields: vec![data(*b"520", *b"  ", &[(b'a', &long)])],
    };
    assert_eq!(
        too_long_field.write_iso2709(&mut out),
        Err(WriteError::FieldTooLong {
            tag: Tag::new(*b"520"),
            len: 10_002
        })
    );
    let chunk = "x".repeat(9_000);
    let too_long_record = Record {
        leader: LEADER,
        fields: (0..12)
            .map(|_| data(*b"520", *b"  ", &[(b'a', &chunk)]))
            .collect(),
    };
    assert!(matches!(
        too_long_record.write_iso2709(&mut out),
        Err(WriteError::RecordTooLong { .. })
    ));
    for field in [
        control(*b"001", "a\x1eb"),
        data(*b"245", *b"1\x1f", &[]),
        data(*b"245", *b"  ", &[(0x1d, "x")]),
        data(*b"245", *b"  ", &[(b'a', "x\x1fy")]),
    ] {
        let tag = field.tag();
        let record = Record {
            leader: LEADER,
            fields: vec![field],
        };
        assert_eq!(
            record.write_iso2709(&mut out),
            Err(WriteError::DelimiterInData { tag })
        );
    }
    assert_eq!(out, b"previous");
}

#[test]
fn reading_keeps_what_marc_record_keeps() {
    // Junk between the indicators and the first subfield, an empty codeless subfield, and
    // a data field one byte long.
    let mut source = Record {
        leader: LEADER,
        fields: Vec::new(),
    };
    let raw = b"00063nam a2200049 a 4500245001100000500000200011\x1e\
        10junk\x1faT\x1f\x1e1\x1e\x1d";
    let read = Record::from_ref(&RecordRef::parse(raw, &Limits::default()).unwrap());
    source.fields = vec![
        data(*b"245", *b"10", &[(b'a', "T")]),
        data(*b"500", *b"1 ", &[]),
    ];
    assert_eq!(read.fields, source.fields);
}

#[test]
fn marcxml_document_layout_and_escapes() {
    let record = Record {
        leader: LEADER,
        fields: vec![
            control(*b"001", "<1>"),
            data(*b"245", *b"1\"", &[(b'a', "Tom & \"Jerry\" 'n' <me>")]),
        ],
    };
    let mut xml = String::new();
    record.write_marcxml_document(&mut xml).unwrap();
    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<record\n    \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n    \
         xsi:schemaLocation=\"http://www.loc.gov/MARC21/slim http://www.loc.gov/standards/marcxml/schema/MARC21slim.xsd\"\n    \
         xmlns=\"http://www.loc.gov/MARC21/slim\">\n\n  \
         <leader>00000nam a2200000 a 4500</leader>\n  \
         <controlfield tag=\"001\">&lt;1&gt;</controlfield>\n  \
         <datafield tag=\"245\" ind1=\"1\" ind2=\"&quot;\">\n    \
         <subfield code=\"a\">Tom &amp; &quot;Jerry&quot; &apos;n&apos; &lt;me&gt;</subfield>\n  \
         </datafield>\n</record>\n"
    );
}

#[test]
fn marcxml_collection_layout() {
    let record = Record {
        leader: LEADER,
        fields: vec![control(*b"001", "1")],
    };
    let mut xml = MARCXML_COLLECTION_HEADER.to_owned();
    record.write_marcxml_collection_record(&mut xml).unwrap();
    xml.push_str(MARCXML_COLLECTION_FOOTER);
    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection\n  \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n  \
         xsi:schemaLocation=\"http://www.loc.gov/MARC21/slim http://www.loc.gov/standards/marcxml/schema/MARC21slim.xsd\"\n  \
         xmlns=\"http://www.loc.gov/MARC21/slim\">\n\n\
         <record>\n  <leader>00000nam a2200000 a 4500</leader>\n  \
         <controlfield tag=\"001\">1</controlfield>\n</record>\n\n</collection>\n"
    );
}

#[test]
fn marcxml_requires_utf8() {
    let mut legacy = Record {
        leader: *b"00000nam  2200000 a 4500",
        fields: Vec::new(),
    };
    let mut xml = String::from("kept");
    assert_eq!(
        legacy.write_marcxml_document(&mut xml),
        Err(WriteError::NotUtf8 { tag: Tag::LEADER })
    );
    legacy.leader = LEADER;
    legacy.fields.push(Field::Control {
        tag: Tag::new(*b"001"),
        value: vec![0xff],
    });
    assert_eq!(
        legacy.write_marcxml_collection_record(&mut xml),
        Err(WriteError::NotUtf8 {
            tag: Tag::new(*b"001")
        })
    );
    assert_eq!(xml, "kept");
}

#[test]
fn publication_replaces_the_file_whole_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.mrc");
    std::fs::write(&path, b"old").unwrap();

    let failed = publish_atomically(&path, |writer| {
        std::io::Write::write_all(writer, b"partial")?;
        Err(std::io::Error::other("disk full"))
    });
    assert_eq!(failed.unwrap_err().to_string(), "disk full");
    assert_eq!(std::fs::read(&path).unwrap(), b"old");

    publish_atomically(&path, |writer| std::io::Write::write_all(writer, b"new")).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");

    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "temporary files left behind");

    let missing = dir.path().join("missing").join("export.mrc");
    assert!(publish_atomically(&missing, |_| Ok(())).is_err());
}

fn value() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(
        any::<u8>().prop_filter("delimiter", |b| *b < 0x1d || *b > 0x1f),
        0..40,
    )
}

fn field() -> impl Strategy<Value = Field> {
    let control = (0_u8..10, value()).prop_map(|(n, value)| Field::Control {
        tag: Tag::new([b'0', b'0', b'0' + n]),
        value,
    });
    let subfield = (b'a'..=b'z', value()).prop_map(|(code, value)| Subfield { code, value });
    let data = (
        100_u16..1000,
        proptest::array::uniform2(b' '..=b'9'),
        proptest::collection::vec(subfield, 0..5),
    )
        .prop_map(|(n, indicators, subfields)| {
            let digits: [u8; 3] = format!("{n:03}").into_bytes().try_into().unwrap();
            Field::Data {
                tag: Tag::new(digits),
                indicators,
                subfields,
            }
        });
    prop_oneof![control, data]
}

proptest! {
    #[test]
    fn written_records_read_back_equal(fields in proptest::collection::vec(field(), 0..30)) {
        let record = Record { leader: LEADER, fields };
        let bytes = iso2709(&record);
        let read = Record::from_ref(&RecordRef::parse(&bytes, &Limits::default()).unwrap());
        prop_assert_eq!(&read.fields, &record.fields);
        prop_assert_eq!(iso2709(&read), bytes);
    }
}
