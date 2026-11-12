// SPDX-License-Identifier: GPL-3.0-or-later
//! Repository automation for Atrium, run as `cargo xtask <command>`.
//!
//! Commands:
//! - `check-layers`: every workspace crate depends only on the Atrium crates that
//!   `tools/layers.json` allows.
//! - `check-file-size`: no hand-written Rust file exceeds [`file_size::MAX_LINES`] lines.
//! - `codegen [--check]`: writes (or checks) `crates/atrium-data/src/generated/` from
//!   `koha-lock.json` and `data/koha-<series>/`.
//! - `check-deviations`: validates `docs/desvios-conhecidos.md`.
//! - `ledger [where <name>]`: port ledger, Koha subs cited with `/// Koha:` (needs
//!   `reference/koha`).
//! - `ci`: runs every check above, with `codegen` in check mode.
#![expect(
    clippy::print_stderr,
    reason = "xtask reports failures on the terminal"
)]

mod codegen;
mod deviations;
mod file_size;
mod layers;
mod ledger;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();
    let root = workspace_root();
    let result = match command.as_str() {
        "check-layers" => layers::run(&root),
        "check-file-size" => file_size::run(&root),
        "codegen" => codegen::run(&root, codegen_mode()),
        "check-deviations" => deviations::run(&root),
        "ledger" => ledger::run(&root, &std::env::args().skip(2).collect::<Vec<_>>()),
        "ci" => layers::run(&root)
            .and_then(|()| file_size::run(&root))
            .and_then(|()| codegen::run(&root, codegen::Mode::Check))
            .and_then(|()| deviations::run(&root)),
        _ => Err(format!(
            "unknown command `{command}`; expected one of: check-layers, check-file-size, codegen [--check], check-deviations, ledger, ci"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// `codegen --check` compares; plain `codegen` writes.
fn codegen_mode() -> codegen::Mode {
    if std::env::args().any(|arg| arg == "--check") {
        codegen::Mode::Check
    } else {
        codegen::Mode::Write
    }
}

/// The workspace root is the parent of this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf()
}
