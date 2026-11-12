// SPDX-License-Identifier: GPL-3.0-or-later
//! File-size check: hand-written Rust files stay small enough to read and review.

use std::path::{Path, PathBuf};

/// Maximum number of lines in a hand-written `.rs` file (CLAUDE.md, "Estilo").
pub(crate) const MAX_LINES: usize = 3000;

/// Directories scanned, relative to the workspace root.
const ROOTS: [&str; 3] = ["crates", "xtask", "tools"];

/// Directory names skipped: build output and generated code.
const SKIPPED_DIRS: [&str; 2] = ["target", "generated"];

/// Fails when any scanned file exceeds [`MAX_LINES`].
pub(crate) fn run(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    for dir in ROOTS {
        collect_rust_files(&root.join(dir), &mut files)?;
    }
    let mut too_long = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        let lines = text.lines().count();
        if lines > MAX_LINES {
            let shown = file.strip_prefix(root).unwrap_or(file);
            too_long.push(format!("{} has {lines} lines", shown.display()));
        }
    }
    if too_long.is_empty() {
        println!(
            "check-file-size: {} files within {MAX_LINES} lines",
            files.len()
        );
        Ok(())
    } else {
        Err(format!(
            "files over {MAX_LINES} lines (split them):\n  {}",
            too_long.join("\n  ")
        ))
    }
}

/// Recursively collects `.rs` files under `dir`, skipping [`SKIPPED_DIRS`].
/// A missing directory contributes nothing.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            let skipped = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| SKIPPED_DIRS.contains(&n));
            if !skipped {
                collect_rust_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
