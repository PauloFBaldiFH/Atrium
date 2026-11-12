// SPDX-License-Identifier: GPL-3.0-or-later
//! MARC-8 and ISO 5426 conversion (docs/specs/marc/conversao-de-caracteres.md). The MARC-8
//! cases citing `MARC::Charset` tests (`t/…`) are the expectations of that library's own tests.

use std::borrow::Cow;

use atrium_marc::{DetectedEncoding, Field, Record, Subfield, Tag, iso5426_to_utf8, marc8_to_utf8};

const ESC: &str = "\x1b";

fn marc8(bytes: &[u8]) -> String {
    let decoded = marc8_to_utf8(bytes);
    assert!(
        decoded.unmapped.is_empty(),
        "unmapped bytes at {:?}",
        decoded.unmapped
    );
    decoded.text.into_owned()
}

#[test]
fn plain_ascii_is_borrowed() {
    assert!(matches!(
        marc8_to_utf8(b"Dom Casmurro\t1899").text,
        Cow::Borrowed(_)
    ));
    assert!(matches!(iso5426_to_utf8(b"Dom Casmurro"), Cow::Borrowed(_)));
}

#[test]
fn ansel_diacritics_move_after_their_letter() {
    assert_eq!(
        marc8(b"Cora\xf0cao, Jos\xe2e"),
        "Corac\u{327}ao, Jose\u{301}"
    );
    // Two diacritics on one letter keep their order.
    assert_eq!(marc8(b"\xe3\xe2a"), "a\u{302}\u{301}");
}

#[test]
fn double_diacritics_drop_the_second_half() {
    // `MARC::Charset` t/double-diacritics.t
    assert_eq!(
        marc8(b"Ha\xfan\xfbgin Il\xebi\xecushin"),
        "Han\u{360}gin Ili\u{361}ushin"
    );
}

#[test]
fn trailing_diacritics_are_lost_and_spaces_do_not_take_them() {
    assert_eq!(marc8(b"abc\xe2"), "abc");
    assert_eq!(marc8(b"\xe2 e"), " e\u{301}");
}

#[test]
fn escapes_switch_character_sets() {
    // `MARC::Charset` t/escape1.t and t/escape2.t
    let greek = [
        b"it is all greek ",
        ESC.as_bytes(),
        b"gabc",
        ESC.as_bytes(),
        b"s to me",
    ]
    .concat();
    assert_eq!(marc8(&greek), "it is all greek \u{3b1}\u{3b2}\u{3b3} to me");
    let subscript = [
        b"x".as_slice(),
        ESC.as_bytes(),
        b"b10",
        ESC.as_bytes(),
        b"s",
    ]
    .concat();
    assert_eq!(marc8(&subscript), "x\u{2081}\u{2080}");
    let arabic = [ESC.as_bytes(), b"(3", ESC.as_bytes(), b")4M\xba"].concat();
    assert_eq!(marc8(&arabic), "\u{62d}\u{68e}");
    let mixed = [
        ESC.as_bytes(),
        b"(3",
        ESC.as_bytes(),
        b")4G",
        ESC.as_bytes(),
        b"(2q\xe9",
    ]
    .concat();
    assert_eq!(marc8(&mixed), "\u{627}\u{5e1}\u{6bb}");
}

#[test]
fn cjk_and_the_iii_additions() {
    let cjk = [
        b"a ".as_slice(),
        ESC.as_bytes(),
        b"$1!0!",
        ESC.as_bytes(),
        b"(B z",
    ]
    .concat();
    assert_eq!(marc8(&cjk), "a \u{4e00} z");
    // `MARC::Charset` t/iii.t
    let iii = [
        b"a ".as_slice(),
        ESC.as_bytes(),
        b"$1! =! @\x7f \x14\x7f \x19\x7f  \x7f!\"",
        ESC.as_bytes(),
        b"(B z",
    ]
    .concat();
    assert_eq!(
        marc8(&iii),
        "a \u{2026}\u{201c}\u{2014}\u{2019}\u{201d}\u{2122} z"
    );
}

#[test]
fn multibyte_g1_escapes_designate_g0_as_marc_charset_does() {
    // `ESC $ ) 1` should put EACC in G1; MARC::Charset makes `)` the G0 set instead, so the
    // `1` and the letters after it have no mapping.
    let decoded = marc8_to_utf8(b"\x1b$)1ab");
    assert_eq!(decoded.text, "");
    assert_eq!(decoded.unmapped, vec![3, 4, 5]);
}

#[test]
fn unmapped_bytes_are_skipped_and_reported() {
    // `MARC::Charset` t/ignore_errors.t
    let decoded = marc8_to_utf8(b"Caf\xe2e \xbf x");
    assert_eq!(decoded.text, "Cafe\u{301}  x");
    assert_eq!(decoded.unmapped, vec![6]);
    // A lone ESC is a character.
    assert_eq!(marc8(b"a\x1b"), "a\u{1b}");
}

#[test]
fn iso5426_follows_char_decode5426() {
    // Diacritic + base, a single-byte entry, and an unknown byte kept as its own code point.
    let expected_pair = atrium_data::iso5426_char(0xc141).unwrap();
    assert_eq!(iso5426_to_utf8(b"\xc1A"), expected_pair.to_string());
    // 0x00, 0x01 and 0x0F are removed.
    assert_eq!(iso5426_to_utf8(b"a\x00b\x01c\x0fd"), "abcd");
    // A diacritic at the end has no pair: the byte alone is looked up.
    let lone = atrium_data::iso5426_char(0xc1).unwrap_or('\u{c1}');
    assert_eq!(iso5426_to_utf8(b"x\xc1"), format!("x{lone}"));
    // Only the first `ESC x`, `0x0E` and `ESC [` are removed.
    assert_eq!(iso5426_to_utf8(b"a\x1bxb\x1byc"), "ab\u{1b}yc");
    assert_eq!(iso5426_to_utf8(b"\x0ea\x0e"), "a\u{e}");
}

fn record(control: &[u8], values: &[&[u8]]) -> Record {
    let mut fields = vec![Field::Control {
        tag: Tag::new(*b"001"),
        value: control.to_vec(),
    }];
    fields.extend(values.iter().map(|value| Field::Data {
        tag: Tag::new(*b"245"),
        indicators: *b"10",
        subfields: vec![Subfield {
            code: b'a',
            value: value.to_vec(),
        }],
    }));
    Record {
        leader: *b"00000nam  2200000 a 4500",
        fields,
    }
}

fn values(record: &Record) -> Vec<Vec<u8>> {
    record
        .fields
        .iter()
        .filter_map(|field| match field {
            Field::Data { subfields, .. } => {
                subfields.first().map(|subfield| subfield.value.clone())
            }
            Field::Control { .. } => None,
        })
        .collect()
}

#[test]
fn records_are_detected_as_utf8_then_marc8_then_latin1() {
    let mut utf8 = record(b"1", &["Coração".as_bytes()]);
    assert_eq!(utf8.marc21_to_utf8(), DetectedEncoding::Utf8);
    assert_eq!(values(&utf8), vec!["Coração".as_bytes().to_vec()]);
    assert_eq!(utf8.leader[9], b'a');

    let mut marc8 = record(b"1", &[b"Cora\xf0cao"]);
    assert_eq!(marc8.marc21_to_utf8(), DetectedEncoding::Marc8);
    assert_eq!(values(&marc8), vec!["Corac\u{327}ao".as_bytes().to_vec()]);

    // One unmapped byte anywhere sends the whole record to ISO-8859-1, including the value
    // that MARC-8 would have converted.
    let mut latin1 = record(b"1", &[b"Cora\xf0cao", b"Ol\xe1 \xbf"]);
    assert_eq!(latin1.marc21_to_utf8(), DetectedEncoding::Latin1);
    assert_eq!(
        values(&latin1),
        vec!["Coraðcao".as_bytes().to_vec(), "Olá ¿".as_bytes().to_vec()]
    );
    assert_eq!(latin1.leader[9], b'a');

    // Control fields are never converted.
    let mut control = record(b"\xe9", &[b"x"]);
    assert_eq!(control.marc21_to_utf8(), DetectedEncoding::Marc8);
    assert_eq!(
        control.fields[0],
        Field::Control {
            tag: Tag::new(*b"001"),
            value: vec![0xe9]
        }
    );
}
