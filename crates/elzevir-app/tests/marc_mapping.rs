// SPDX-License-Identifier: GPL-3.0-or-later
//! `marc_to_bibframe` over records exported by the oracle's staff export with their items
//! (docs/specs/catalogo/mapeamento-marc21.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_domain::catalog::MaterialType;
use elzevir_marc::{Limits, Record, RecordRef};

/// The first 200 records of the test base, exported by `tools/export.pl` (ISO 2709, with
/// items) on 2026-11-13. Bibliographic data only; no patron data is embedded in `952`.
const STAFF_EXPORT: &[u8] = include_bytes!("data/koha-staff-bibs-1-200.mrc");

/// Records picked to cover the spec's tables: 110 and 490, 600 and 651, several 952,
/// 264 + 041 + 100$4 + 007, 090, 111, 856, 775, 952$e, 240, and a periodical.
const SNAPSHOT_POSITIONS: [usize; 11] = [0, 11, 16, 18, 27, 35, 57, 66, 90, 101, 134];

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

#[test]
fn every_exported_record_maps() {
    let records = records();
    assert_eq!(records.len(), 200);
    for (position, (raw, record)) in records.iter().enumerate() {
        let mapped = marc_to_bibframe(record, Some(raw.clone()))
            .unwrap_or_else(|e| panic!("record {position}: {e}"));
        assert!(!mapped.work_title.is_empty(), "record {position}");
        assert_eq!(mapped.source.as_deref(), Some(raw.as_slice()));
    }
}

#[test]
fn items_follow_koha_952_conventions() {
    let records = records();
    let items: Vec<_> = records
        .iter()
        .flat_map(|(_, record)| marc_to_bibframe(record, None).unwrap().items)
        .collect();
    assert_eq!(items.len(), 253, "one Item per 952");
    assert!(items.iter().all(|item| item.home_branch == "MAIN"));
    assert!(items.iter().all(|item| item.call_number.is_some()));
    assert!(items.iter().all(|item| item.barcode.is_some()));
}

#[test]
fn categories_come_from_the_leader() {
    let records = records();
    let periodicals = records
        .iter()
        .filter(|(_, record)| {
            marc_to_bibframe(record, None).unwrap().material_type == MaterialType::Periodical
        })
        .count();
    assert_eq!(periodicals, 1, "the corpus has one `as` record");
}

#[test]
fn mapping_snapshots() {
    let records = records();
    for position in SNAPSHOT_POSITIONS {
        let (_, record) = &records[position];
        let mapped = marc_to_bibframe(record, None).unwrap();
        insta::assert_yaml_snapshot!(format!("record_{position:03}"), mapped);
    }
}
