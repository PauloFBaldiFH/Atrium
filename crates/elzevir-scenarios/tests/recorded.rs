// SPDX-License-Identifier: GPL-3.0-or-later
//! Every scenario in `scenarios/` has a current recording, and Elzevir reproduces it.

#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]

use std::path::{Path, PathBuf};

use elzevir_scenarios::{CheckError, Expected, check, find, load_recording, load_scenario};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pinned_koha() -> String {
    let lock = std::fs::read_to_string(repository().join("koha-lock.json")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    lock["version"].as_str().unwrap().to_owned()
}

#[test]
fn every_scenario_matches_its_oracle_recording() {
    let scenarios = find(&repository().join("scenarios")).unwrap();
    assert!(!scenarios.is_empty(), "no scenarios found");
    let pinned = pinned_koha();
    let mut failures = Vec::new();
    for files in scenarios {
        let scenario = load_scenario(&files.scenario).unwrap();
        let result = load_recording(&files.recording)
            .map_err(|e| e.to_string())
            .and_then(|recording| check(&scenario, &recording, &pinned).map_err(|e| e.to_string()));
        if let Err(error) = result {
            failures.push(format!("{}: {error}", files.scenario.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn recorded() -> (elzevir_scenarios::Scenario, elzevir_scenarios::Recording) {
    let dir = repository().join("scenarios/circulacao");
    let scenario = load_scenario(&dir.join("vencimento-emprestimo-padrao.yaml")).unwrap();
    let recording = load_recording(&dir.join("vencimento-emprestimo-padrao.koha.json")).unwrap();
    (scenario, recording)
}

#[test]
fn a_changed_scenario_needs_a_new_recording() {
    let (mut scenario, recording) = recorded();
    scenario.then = vec![Expected::Loan {
        due: "2026-01-01".to_owned(),
    }];
    assert_eq!(
        check(&scenario, &recording, &pinned_koha()),
        Err(CheckError::Stale)
    );
}

#[test]
fn a_different_koha_output_is_reported() {
    let (scenario, mut recording) = recorded();
    let elzevir_scenarios::RecordedOutput::Loan(loan) = &mut recording.outputs[0];
    loan.due_date = "2026-11-27T23:59:00-03:00".to_owned();
    let error = check(&scenario, &recording, &pinned_koha()).unwrap_err();
    assert!(matches!(error, CheckError::Mismatch { .. }), "{error}");
}

#[test]
fn another_koha_version_is_rejected() {
    let (scenario, recording) = recorded();
    let error = check(&scenario, &recording, "26.11.00.000").unwrap_err();
    assert!(matches!(error, CheckError::KohaVersion { .. }), "{error}");
}
