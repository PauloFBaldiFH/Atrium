// SPDX-License-Identifier: GPL-3.0-or-later
//! Static assets compiled into the binary (see `assets/README.md`).

use axum::http::header;
use axum::response::IntoResponse;

const STYLESHEET: &str = include_str!("../assets/elzevir.css");
const CATALOG_SCRIPT: &str = include_str!("../assets/elzevir-catalog.js");
const ICONS: &str = include_str!("../assets/elzevir-icons.svg");
const HTMX: &str = include_str!("../assets/htmx-2.0.10.min.js");
const CACHE: &str = "public, max-age=3600";

pub(crate) async fn stylesheet() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE),
        ],
        STYLESHEET,
    )
}

pub(crate) async fn htmx() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE),
        ],
        HTMX,
    )
}

pub(crate) async fn catalog_script() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE),
        ],
        CATALOG_SCRIPT,
    )
}

pub(crate) async fn icons() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE),
        ],
        ICONS,
    )
}
