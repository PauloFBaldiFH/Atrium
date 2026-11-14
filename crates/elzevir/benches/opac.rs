// SPDX-License-Identifier: GPL-3.0-or-later
//! OPAC and staff listing latency over a large catalog (task 1.9 criteria: p95 under 50 ms
//! for a search and under 30 ms for a record; task 2.3: p95 under 50 ms for a page of the
//! staff listing, browsing or searching; all over one million records). Run with
//! `cargo bench -p elzevir --bench opac`; `ELZEVIR_BENCH_RECORDS` sets the catalog size
//! (default 1 000 000). Prints numbers; it asserts nothing, because speed depends on the
//! machine.
//!
//! The catalog is the staff export repeated, without barcodes so the copies do not
//! collide: a word query matches thousands of records, which is the hard case for ranking
//! and counting. Requests go through the whole router, rendering included.
#![expect(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::float_arithmetic,
    reason = "benchmark binary: fails loudly and prints its measurements"
)]

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use elzevir_app::catalog::CatalogCommand;
use elzevir_app::commands::{StaffCommand, WriteCommand};
use elzevir_app::import::{ImportOptions, import_marc21};
use elzevir_app::listing::sort_key;
use elzevir_app::marc_mapping::marc_to_bibframe;
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, index_pending};
use elzevir_app::staff::{Permission, hash_password};
use elzevir_marc::{Field, Limits, MarcReader, Record};
use elzevir_search::TantivyIndex;
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};
use elzevir_web::{AppState, OpacPorts, StaffPorts, WebConfig, router};
use elzevir_web_templates::{I18n, url_encode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const STAFF_EXPORT: &[u8] =
    include_bytes!("../../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc");
const QUERIES: &str = include_str!("../tests/data/koha-opac-search.tsv");
const REQUESTS: usize = 2000;

struct NoCounters;

impl MemoryProbe for NoCounters {
    fn report(&self) -> MemoryReport {
        MemoryReport {
            allocator: "mimalloc",
            counters: None,
        }
    }
}

fn main() {
    // `cargo bench` passes `--bench`; test runners that list every target get nothing.
    if !std::env::args().any(|arg| arg == "--bench") {
        return;
    }
    let records: usize = std::env::var("ELZEVIR_BENCH_RECORDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000_000);
    let directory = tempfile::tempdir().unwrap();
    let (store, index) = catalog(directory.path(), records);
    let store = Arc::new(store);
    let index = Arc::new(index);
    let app = router(AppState::new(
        Arc::clone(&store) as _,
        Arc::new(NoCounters),
        OpacPorts {
            catalog: Arc::clone(&store) as _,
            index: Arc::clone(&index) as _,
        },
        StaffPorts {
            listing: Arc::clone(&store) as _,
            records: Arc::clone(&store) as _,
            editor: Arc::clone(&store) as _,
            suggestions: Arc::clone(&store) as _,
        },
        I18n::english().unwrap(),
        WebConfig::default(),
    ));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(measure(&app, records));
    create_account(&store);
    let session = runtime.block_on(sign_in(&app));
    runtime.block_on(measure_listing(&app, &session, records));
}

/// Creates the staff account the listing is measured with.
fn create_account(store: &ElzevirStore) {
    let password_hash = hash_password("bench-password").unwrap();
    store
        .submit(WriteCommand::Staff(StaffCommand::Create {
            login: "bench".to_owned(),
            password_hash,
            permissions: [Permission::StaffInterface].into(),
        }))
        .unwrap()
        .wait()
        .unwrap();
}

/// Signs in; the session cookie.
async fn sign_in(app: &Router) -> String {
    let form = app
        .clone()
        .oneshot(Request::get("/equipe/login").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let login_cookie = cookie(&form, "elzevir_login_csrf");
    let html = String::from_utf8(
        form.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let csrf = html
        .split(r#"name="csrf" value=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    let signed = app
        .clone()
        .oneshot(
            Request::post("/equipe/login")
                .header(header::COOKIE, login_cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "login=bench&password=bench-password&csrf={csrf}"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    cookie(&signed, "elzevir_session")
}

/// The `name=value` part of the response cookie called `name`.
fn cookie(response: &axum::http::Response<Body>, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .and_then(|value| value.split(';').next())
        .unwrap()
        .to_owned()
}

/// Staff listing pages: browsing from cursors spread over the catalog, and searching.
async fn measure_listing(app: &Router, session: &str, records: usize) {
    let titles = titles(STAFF_EXPORT);
    let mut browse = Vec::with_capacity(REQUESTS);
    browse.push(time_as(app, "/equipe/acervo", session).await);
    for n in 1..REQUESTS {
        // A real cursor shape: a title's key and a Work id somewhere in the catalog.
        let key = sort_key(titles.get(n % titles.len()).unwrap());
        let work = (n * 7919) % records + 1;
        let token = url_encode(&format!("w{work}.{key}"));
        browse.push(time_as(app, &format!("/equipe/acervo?after={token}"), session).await);
    }
    report("staff listing, browse page", browse);
    let queries: Vec<&str> = QUERIES
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    let mut search = Vec::with_capacity(REQUESTS);
    for n in 0..REQUESTS {
        let query = url_encode(queries.get(n % queries.len()).unwrap());
        let from = n / queries.len() % 3 * 30;
        search.push(
            time_as(
                app,
                &format!("/equipe/acervo?q={query}&from={from}"),
                session,
            )
            .await,
        );
    }
    report("staff listing, search page", search);
}

/// The Work titles (`245 $a`) of the export.
fn titles(file: &[u8]) -> Vec<String> {
    let mut titles = Vec::new();
    let mut reader = MarcReader::new(file, Limits::default());
    while let Some(record) = reader.next_record() {
        let mut record = Record::from_ref(&record.unwrap());
        record.marc21_to_utf8();
        titles.push(marc_to_bibframe(&record, None).unwrap().work_title);
    }
    titles
}

/// A store and an index with `records` records.
fn catalog(directory: &std::path::Path, records: usize) -> (ElzevirStore, TantivyIndex) {
    let store = ElzevirStore::open(
        &directory.join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap();
    let file = without_barcodes(STAFF_EXPORT);
    create_codes(&store, &file);
    let cores = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let options = ImportOptions {
        parallelism: Parallelism::Threads(cores),
        ..ImportOptions::default()
    };
    let started = Instant::now();
    let mut imported = 0;
    // A thousand copies at a time keeps the input in memory small.
    let chunk = file.repeat(1000);
    while imported < records {
        let copies = (records - imported).div_ceil(200).min(1000);
        let input = chunk.get(..file.len() * copies).unwrap();
        imported +=
            usize::try_from(import_marc21(input, &store, &options).unwrap().imported).unwrap();
    }
    println!(
        "imported {imported} records in {:.0} s",
        started.elapsed().as_secs_f64()
    );
    std::fs::create_dir(directory.join("index")).unwrap();
    let index = TantivyIndex::open(&directory.join("index"), Parallelism::Threads(cores)).unwrap();
    let started = Instant::now();
    let report = index_pending(
        &store,
        &store,
        &index,
        &IndexOptions {
            parallelism: Parallelism::Threads(cores),
            batch: NonZeroUsize::new(50_000).unwrap(),
        },
    )
    .unwrap();
    println!(
        "indexed {} records in {:.0} s",
        report.indexed,
        started.elapsed().as_secs_f64()
    );
    (store, index)
}

async fn measure(app: &Router, records: usize) {
    let queries: Vec<&str> = QUERIES
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    let mut search = Vec::with_capacity(REQUESTS);
    for n in 0..REQUESTS {
        let query = url_encode(queries.get(n % queries.len()).unwrap());
        let page = n / queries.len() % 3 + 1;
        search.push(time(app, &format!("/search?q={query}&page={page}")).await);
    }
    report("search", search);
    // Spread over the catalog with a fixed stride; every id is visited once cold, then again.
    let ids: Vec<usize> = (0..REQUESTS)
        .map(|n| (n * 7919) % records + 1)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut cold = Vec::with_capacity(ids.len());
    for id in &ids {
        cold.push(time(app, &format!("/record/{id}")).await);
    }
    report("record, first view", cold);
    let mut warm = Vec::with_capacity(ids.len());
    for id in &ids {
        warm.push(time(app, &format!("/record/{id}")).await);
    }
    report("record, cached", warm);
}

async fn time(app: &Router, uri: &str) -> Duration {
    time_request(app, Request::get(uri).body(Body::empty()).unwrap(), uri).await
}

async fn time_as(app: &Router, uri: &str, session: &str) -> Duration {
    let request = Request::get(uri)
        .header(header::COOKIE, session)
        .body(Body::empty())
        .unwrap();
    time_request(app, request, uri).await
}

async fn time_request(app: &Router, request: Request<Body>, uri: &str) -> Duration {
    let started = Instant::now();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    response.into_body().collect().await.unwrap();
    started.elapsed()
}

fn report(what: &str, mut samples: Vec<Duration>) {
    samples.sort();
    // Nearest rank: the sample below which `percent` of the samples fall.
    let at = |percent: usize| {
        let rank = (samples.len() * percent).div_ceil(100).max(1);
        samples.get(rank - 1).unwrap().as_secs_f64() * 1000.0
    };
    println!(
        "{what}: {} requests, p50 {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        samples.len(),
        at(50),
        at(95),
        at(99),
        at(100)
    );
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

/// Creates the branches and item types the export refers to.
fn create_codes(store: &ElzevirStore, file: &[u8]) {
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
}
