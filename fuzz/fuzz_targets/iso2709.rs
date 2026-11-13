// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz target: the ISO 2709 reader and every accessor must handle any input without
//! panicking, looping forever or allocating past the record limit.
#![no_main]

use elzevir_marc::{Limits, MarcReader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut reader = MarcReader::new(data, Limits::default());
    while let Some(result) = reader.next_record() {
        let Ok(record) = result else { continue };
        let _ = record.encoding();
        for field in record.fields() {
            let _ = field.indicators();
            for (_code, value) in field.subfields() {
                std::hint::black_box(value);
            }
        }
    }
});
