// SPDX-License-Identifier: GPL-3.0-or-later
//! `elzevir import` and `elzevir export` as a user runs them
//! (docs/specs/catalogo/linha-de-comando.md).
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use std::path::Path;
use std::process::{Command, Output};

const STAFF_EXPORT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../elzevir-app/tests/data/koha-staff-bibs-1-200.mrc"
);

fn elzevir(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_elzevir"))
        .args(args)
        .env_remove("ELZEVIR_SERIAL")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_str().unwrap().to_owned()
}

#[test]
fn a_koha_export_goes_in_and_comes_back_out_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let data = path(dir.path(), "data");

    let first = elzevir(&[
        "import",
        "--data-dir",
        &data,
        "--create-codes",
        STAFF_EXPORT,
    ]);
    assert!(first.status.success(), "{}", text(&first.stderr));
    let stdout = text(&first.stdout);
    assert!(stdout.contains("200 records read, 200 stored"), "{stdout}");
    assert!(stdout.contains("253 items stored"), "{stdout}");

    let output = path(dir.path(), "out.mrc");
    let exported = elzevir(&["export", "--data-dir", &data, "--output", &output]);
    assert!(exported.status.success(), "{}", text(&exported.stderr));
    assert!(text(&exported.stderr).contains("200 records exported, 0 failed"));
    assert_eq!(
        std::fs::read(&output).unwrap(),
        std::fs::read(STAFF_EXPORT).unwrap()
    );

    // Standard output gives the same file.
    let piped = elzevir(&["export", "--data-dir", &data]);
    assert!(piped.status.success());
    assert_eq!(piped.stdout, std::fs::read(STAFF_EXPORT).unwrap());

    // Importing it again with the Koha biblionumber rule stores nothing.
    let again = elzevir(&[
        "import",
        "--data-dir",
        &data,
        "--match",
        "koha-biblionumber",
        STAFF_EXPORT,
    ]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    let stdout = text(&again.stdout);
    assert!(
        stdout.contains("200 records read, 0 stored, 200 matched"),
        "{stdout}"
    );
    assert!(
        stdout.contains("0 items stored, 0 replaced, 253 refused"),
        "{stdout}"
    );
}

#[test]
fn unknown_codes_are_refused_without_create_codes() {
    let dir = tempfile::tempdir().unwrap();
    let data = path(dir.path(), "data");
    let run = elzevir(&["import", "--data-dir", &data, STAFF_EXPORT]);
    assert!(run.status.success());
    // Records without Items go in; the others are refused whole.
    assert!(!text(&run.stdout).contains("200 stored"));
    assert!(text(&run.stderr).contains("unknown branch"));
}

#[test]
fn matching_options_need_a_rule() {
    let dir = tempfile::tempdir().unwrap();
    let data = path(dir.path(), "data");
    let run = elzevir(&[
        "import",
        "--data-dir",
        &data,
        "--on-match",
        "replace",
        STAFF_EXPORT,
    ]);
    assert!(!run.status.success());
}

#[test]
fn a_data_directory_in_use_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let lock = std::fs::File::create(data.join("elzevir.lock")).unwrap();
    lock.lock().unwrap();
    let run = elzevir(&["export", "--data-dir", data.to_str().unwrap()]);
    assert!(!run.status.success());
    assert!(text(&run.stderr).contains("another Elzevir process"));
    drop(lock);
    assert!(
        elzevir(&["export", "--data-dir", data.to_str().unwrap()])
            .status
            .success()
    );
}
