// SPDX-License-Identifier: GPL-3.0-or-later
//! The OPAC over the real store and index, with the 200 records of the staff export
//! (docs/specs/opac/busca-e-registro.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use elzevir_app::catalog::{CatalogCommand, CatalogReader, ItemData, ItemStatus, NewItem};
use elzevir_app::commands::WriteCommand;
use elzevir_app::memory::{MemoryProbe, MemoryReport};
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, SearchIndex, index_pending};
use elzevir_search::TantivyIndex;
use elzevir_store::ElzevirStore;
use elzevir_web::{AppState, OpacPorts, StaffPorts, WebConfig, router};
use elzevir_web_templates::I18n;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::imported;

struct NoCounters;

impl MemoryProbe for NoCounters {
    fn report(&self) -> MemoryReport {
        MemoryReport {
            allocator: "system",
            counters: None,
        }
    }
}

fn options() -> IndexOptions {
    IndexOptions {
        parallelism: Parallelism::Serial,
        batch: NonZeroUsize::new(1024).unwrap(),
    }
}

fn app(store: &Arc<ElzevirStore>, index: &Arc<TantivyIndex>) -> Router {
    router(AppState::new(
        Arc::clone(store) as _,
        Arc::new(NoCounters),
        OpacPorts {
            catalog: Arc::clone(store) as _,
            index: Arc::clone(index) as _,
        },
        StaffPorts {
            listing: Arc::clone(store) as _,
            records: Arc::clone(store) as _,
            editor: Arc::clone(store) as _,
            suggestions: Arc::clone(store) as _,
        },
        I18n::english().unwrap(),
        WebConfig::default(),
    ))
}

/// One request, on a runtime of its own: the store's writer is waited for synchronously,
/// which cannot happen inside a runtime, so the tests stay synchronous.
fn get(app: &Router, uri: &str) -> (StatusCode, String) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    })
}

#[test]
fn search_then_record_then_a_new_item_shows_up() {
    let (_dir, store) = imported();
    let store = Arc::new(store);
    let index = Arc::new(TantivyIndex::in_memory(Parallelism::Serial).unwrap());
    index_pending(&*store, &*store, &*index, &options()).unwrap();
    let app = app(&store, &index);

    let (status, results) = get(&app, "/search?q=anuario+estatistico");
    assert_eq!(status, StatusCode::OK);
    assert!(results.contains("Perfil - Anuário Estatístico"));
    assert!(results.contains("available"));
    let instance = index.search("anuario estatistico", 0, 1).unwrap().hits[0].instance;

    let uri = format!("/record/{}", instance.0);
    let (status, record) = get(&app, &uri);
    assert_eq!(status, StatusCode::OK);
    assert!(record.contains("310.62 P94"));
    let copies = record.matches("<tr>").count();

    store
        .submit(WriteCommand::Catalog(CatalogCommand::AddItem(Box::new(
            NewItem {
                instance,
                item: ItemData {
                    barcode: Some("OPAC-TEST-1".to_owned()),
                    inventory_number: None,
                    home_branch: store.items(instance).unwrap()[0].home_branch.clone(),
                    holding_branch: store.items(instance).unwrap()[0].home_branch.clone(),
                    item_type: store.items(instance).unwrap()[0].item_type.clone(),
                    call_number: Some("310.62 P94 ex.9".to_owned()),
                    location: None,
                    collection: None,
                    price: None,
                    replacement_price: None,
                    copy_number: None,
                    status: ItemStatus {
                        lost: 1,
                        ..ItemStatus::default()
                    },
                    acquired_on: None,
                },
            },
        ))))
        .unwrap()
        .wait()
        .unwrap();
    let (_, after) = get(&app, &uri);
    assert_eq!(after.matches("<tr>").count(), copies + 1);
    assert!(after.contains("310.62 P94 ex.9"));
    assert!(after.contains("Lost"));
}

#[test]
fn a_missing_record_is_404() {
    let (_dir, store) = imported();
    let store = Arc::new(store);
    let index = Arc::new(TantivyIndex::in_memory(Parallelism::Serial).unwrap());
    let (status, _) = get(&app(&store, &index), "/record/999999");
    assert_eq!(status, StatusCode::NOT_FOUND);
}
