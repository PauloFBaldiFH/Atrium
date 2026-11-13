// SPDX-License-Identifier: GPL-3.0-or-later
//! Every case converted from Koha's own tests holds in Elzevir.

use std::path::{Path, PathBuf};

use elzevir_scenarios::{CaseError, find_koha_tests, load_koha_test, run_case, run_koha_test};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_koha_test_case_holds() {
    let files = find_koha_tests(&repository().join("scenarios")).unwrap();
    assert!(files.len() >= 2, "expected CalcDateDue.t and CalcFine.t");
    let mut failures = Vec::new();
    for path in files {
        let scenario = load_koha_test(&path).unwrap();
        assert!(!scenario.cases.is_empty(), "{}: no cases", path.display());
        assert!(
            repository()
                .join("reference/koha")
                .join(&scenario.source)
                .exists()
                || !repository().join("reference/koha/C4").exists(),
            "{}: source {} not found",
            path.display(),
            scenario.source
        );
        for (name, error) in run_koha_test(&scenario) {
            failures.push(format!("{} / {name}: {error}", path.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_wrong_expectation_is_reported() {
    let path = repository().join("scenarios/circulacao/calcfine.t.yaml");
    let scenario = load_koha_test(&path).unwrap();
    let mut case = scenario.cases.first().unwrap().clone();
    case.expect.fine = Some("28.00".to_owned());
    assert_eq!(
        run_case(&scenario.defaults, &case),
        Err(CaseError::Mismatch {
            what: "fine",
            elzevir: "29.00".to_owned(),
            koha: "28.00".to_owned(),
        })
    );
}
