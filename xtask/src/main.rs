// SPDX-License-Identifier: GPL-3.0-or-later
//! Repository automation for Atrium, run as `cargo xtask <command>`.
//!
//! Commands:
//! - `check-layers`: every workspace crate depends only on the Atrium crates that
//!   `tools/layers.json` allows.
//! - `check-file-size`: no hand-written Rust file exceeds [`file_size::MAX_LINES`] lines.
//! - `ci`: runs every check above.
#![expect(
    clippy::print_stderr,
    reason = "xtask reports failures on the terminal"
)]

mod file_size;
mod layers;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();
    let root = workspace_root();
    let result = match command.as_str() {
        "check-layers" => layers::run(&root),
        "check-file-size" => file_size::run(&root),
        "ci" => layers::run(&root).and_then(|()| file_size::run(&root)),
        _ => Err(format!(
            "unknown command `{command}`; expected one of: check-layers, check-file-size, ci"
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

/// The workspace root is the parent of this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf()
}
