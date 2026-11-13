// SPDX-License-Identifier: GPL-3.0-or-later
//! Import throughput and allocations per record (task 1.7 criteria; run with
//! `cargo bench -p elzevir-store --bench import`). Prints numbers; it asserts nothing,
//! because speed depends on the machine.
#![expect(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    reason = "benchmark binary: fails loudly and prints its measurements"
)]

use std::alloc::System;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::time::Instant;

use elzevir_app::catalog::CatalogCommand;
use elzevir_app::commands::WriteCommand;
use elzevir_app::import::{ImportOptions, Parallelism, import_marc21};
use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_marc::{Field, Limits, MarcReader, Record};
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");
const COPIES: usize = 50;

fn main() {
    let file = without_barcodes(STAFF_EXPORT).repeat(COPIES);
    let records = 200 * COPIES;
    allocations(&file, records);
    let cores = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    for parallelism in [Parallelism::Serial, Parallelism::Threads(cores)] {
        throughput(&file, records, parallelism);
    }
}

/// The export without `952$p`, so that its copies do not collide on barcodes.
fn without_barcodes(file: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut reader = MarcReader::new(file, Limits::default());
    while let Some(record) = reader.next_record() {
        let mut record = Record::from_ref(&record.unwrap());
        for field in &mut record.fields {
            if let Field::Data { tag, subfields, .. } = field
                && tag.as_bytes() == b"952"
            {
                subfields.retain(|subfield| subfield.code != b'p');
            }
        }
        record.write_iso2709(&mut out).unwrap();
    }
    out
}

/// Branches and item types the export refers to.
fn codes(file: &[u8]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut branches = BTreeSet::new();
    let mut item_types = BTreeSet::new();
    let mut reader = MarcReader::new(file, Limits::default());
    while let Some(record) = reader.next_record() {
        let mut record = Record::from_ref(&record.unwrap());
        record.marc21_to_utf8();
        for item in marc_to_bibframe(&record, None).unwrap().items {
            branches.insert(item.home_branch);
            branches.insert(item.holding_branch);
            item_types.insert(item.item_type);
        }
    }
    (branches, item_types)
}

/// Allocations of reading, decoding and mapping, without the writer.
fn allocations(file: &[u8], records: usize) {
    let mut reader = MarcReader::new(file, Limits::default());
    let region = Region::new(GLOBAL);
    let started = Instant::now();
    let mut mapped = 0_usize;
    while let Some(record) = reader.next_record() {
        let mut record = Record::from_ref(&record.unwrap());
        record.marc21_to_utf8();
        mapped += marc_to_bibframe(&record, None).unwrap().items.len();
    }
    let seconds = started.elapsed().as_secs_f64();
    let stats = region.change();
    println!(
        "read+map on one thread: {} allocations per record, {:.0} records/s ({records} records, {mapped} items)",
        stats.allocations / records,
        records as f64 / seconds
    );
    let mut reader = MarcReader::new(file, Limits::default());
    let region = Region::new(GLOBAL);
    while let Some(record) = reader.next_record() {
        record.unwrap();
    }
    println!(
        "read only: {} allocations in total",
        region.change().allocations
    );
}

fn throughput(file: &[u8], records: usize, parallelism: Parallelism) {
    let dir = tempfile::tempdir().unwrap();
    let store = ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    let (branches, item_types) = codes(file);
    let commands = branches
        .into_iter()
        .map(|code| CatalogCommand::CreateBranch {
            name: code.clone(),
            code,
        })
        .chain(
            item_types
                .into_iter()
                .map(|code| CatalogCommand::CreateItemType {
                    description: code.clone(),
                    code,
                }),
        );
    for command in commands {
        store
            .submit(WriteCommand::Catalog(command))
            .unwrap()
            .wait()
            .unwrap();
    }
    let options = ImportOptions {
        parallelism,
        ..ImportOptions::default()
    };
    let started = Instant::now();
    let report = import_marc21(file, &store, &options).unwrap();
    let seconds = started.elapsed().as_secs_f64();
    println!(
        "{parallelism:?}: {} of {records} imported ({} refused) in {seconds:.2} s = {:.0} records/s",
        report.imported,
        report.failures.len(),
        records as f64 / seconds
    );
}
