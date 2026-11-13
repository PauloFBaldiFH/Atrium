// SPDX-License-Identifier: GPL-3.0-or-later
//! Atomic publication of an exported file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Writes a file so that readers of `path` see either the previous file or the complete new
/// one, never a partial write: `write` fills a temporary file in the same directory, which
/// is flushed to disk and then renamed over `path`. If anything fails, the temporary file
/// is removed and `path` is untouched.
///
/// # Errors
///
/// The first I/O error, from `write` or from creating, syncing or renaming the file.
pub fn publish_atomically<F>(path: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&mut BufWriter<File>) -> io::Result<()>,
{
    let temporary = temporary_path(path)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = fill(file, write).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        // The original error is what the caller needs; a failure to clean up the temporary
        // file on top of it adds nothing they can act on.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "cleanup after an error already being returned; see comment"
        )]
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn fill<F>(file: File, write: F) -> io::Result<()>
where
    F: FnOnce(&mut BufWriter<File>) -> io::Result<()>,
{
    let mut writer = BufWriter::new(file);
    write(&mut writer)?;
    writer.flush()?;
    writer
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()
}

/// `.<name>.<pid>.tmp` next to `path`: same directory, so the rename never crosses file
/// systems, and hidden from directory listings on Unix.
fn temporary_path(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "export path has no file name")
    })?;
    let mut temporary = std::ffi::OsString::from(".");
    temporary.push(name);
    temporary.push(format!(".{}.tmp", std::process::id()));
    Ok(path.with_file_name(temporary))
}
