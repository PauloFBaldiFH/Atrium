// SPDX-License-Identifier: GPL-3.0-or-later
//! Byte parity with Koha's MARC export, against record 1 exported by the oracle's OPAC on
//! 2026-11-12 (`opac-export.pl`, formats `utf8` and `marcxml`).
//! See docs/specs/marc/escrita.md, §3.
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_marc::{Field, LEADER_LEN, Limits, Record, RecordRef};
use unicode_normalization::UnicodeNormalization;

const KOHA_ISO2709: &[u8] = include_bytes!("data/koha-opac-bib1.mrc");
const KOHA_MARCXML: &str = include_str!("data/koha-opac-bib1.xml");

fn koha_record() -> Record {
    Record::from_ref(&RecordRef::parse(KOHA_ISO2709, &Limits::default()).unwrap())
}

#[test]
fn iso2709_round_trips_byte_for_byte() {
    let mut written = Vec::new();
    koha_record().write_iso2709(&mut written).unwrap();
    assert_eq!(written, KOHA_ISO2709);
}

/// The OPAC's `utf8` export is in NFD (`C4::Charset::SetUTF8Flag`); its `marcxml` export
/// writes the stored record, whose leader lengths are stale, then applies NFC and `&#x…;`
/// entities (`C4::Record::_entity_encode`). Undoing the first and redoing the second here
/// leaves only the MARCXML layout to compare.
#[test]
fn marcxml_document_matches_koha() {
    let mut record = koha_record();
    let leader = KOHA_MARCXML
        .split_once("<leader>")
        .and_then(|(_, rest)| rest.split_once("</leader>"))
        .unwrap()
        .0;
    record.leader = <[u8; LEADER_LEN]>::try_from(leader.as_bytes()).unwrap();
    for field in &mut record.fields {
        if let Field::Data { subfields, .. } = field {
            for subfield in subfields {
                let text = std::str::from_utf8(&subfield.value).unwrap();
                subfield.value = text.nfc().collect::<String>().into_bytes();
            }
        }
    }

    let mut xml = String::new();
    record.write_marcxml_document(&mut xml).unwrap();
    assert_eq!(entity_encode(&xml), KOHA_MARCXML);
}

/// Koha: `C4::Record::_entity_encode`, minus the NFC step (already applied).
fn entity_encode(xml: &str) -> String {
    xml.chars()
        .map(|c| {
            if ('\u{80}'..='\u{fffd}').contains(&c) {
                format!("&#x{:X};", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect()
}
