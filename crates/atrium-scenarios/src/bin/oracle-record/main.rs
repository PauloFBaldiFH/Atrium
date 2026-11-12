// SPDX-License-Identifier: GPL-3.0-or-later
//! Records scenarios against the Koha oracle (doc 05, §4 "Gravação do oráculo").
//!
//! ```sh
//! ATRIUM_ORACLE_URL=https://staff.example \
//! ATRIUM_ORACLE_BASIC_USER=... ATRIUM_ORACLE_BASIC_PASSWORD=... \
//! ATRIUM_ORACLE_USER=... ATRIUM_ORACLE_PASSWORD=... \
//! cargo run -p atrium-scenarios --bin oracle-record -- scenarios/circulacao/*.yaml
//! ```
//!
//! Each checkout it makes is checked in again before it moves on. Credentials are read from
//! the environment and passed to `curl` on its standard input, never on the command line.

#![expect(clippy::print_stdout, reason = "command-line tool reporting progress")]

mod oracle;

use std::path::Path;

use atrium_scenarios::{
    Expected, LoanRecorded, RecordedInputs, RecordedOutput, Recording, Scenario, Step,
    load_scenario,
};

use crate::oracle::Oracle;

type Error = Box<dyn std::error::Error>;

fn main() -> Result<(), Error> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("usage: oracle-record <scenario.yaml>...".into());
    }
    let pinned = pinned_koha_version()?;
    let oracle = Oracle::connect()?;
    let koha = oracle.koha_version()?;
    if !pinned.starts_with(&format!("{koha}.")) && pinned != koha {
        return Err(
            format!("the oracle runs Koha {koha}, but koha-lock.json pins {pinned}").into(),
        );
    }
    for path in paths {
        let path = Path::new(&path);
        let scenario = load_scenario(path)?;
        let recording = record(&oracle, scenario, &pinned)?;
        let out = path.with_extension("koha.json");
        std::fs::write(&out, serde_json::to_string_pretty(&recording)? + "\n")?;
        println!("recorded {}", out.display());
    }
    Ok(())
}

fn record(oracle: &Oracle, scenario: Scenario, pinned: &str) -> Result<Recording, Error> {
    let patron = oracle.example_patron(&scenario.given.patron.category)?;
    let item = oracle.available_item(&scenario.given.item.itemtype, &scenario.given.item.branch)?;
    let mut outputs = Vec::new();
    let mut inputs: Option<RecordedInputs> = None;
    for step in &scenario.when {
        match step {
            Step::Checkout(checkout) => {
                inputs = Some(oracle.inputs(
                    &checkout.branch,
                    &scenario.given.patron.category,
                    &scenario.given.item.itemtype,
                )?);
                let loan: LoanRecorded = oracle.checkout_and_return(patron, &item)?;
                outputs.push(RecordedOutput::Loan(loan));
            }
        }
    }
    if scenario
        .then
        .iter()
        .any(|e| !matches!(e, Expected::Loan { .. }))
    {
        return Err("unsupported expectation".into());
    }
    let inputs = inputs.ok_or("the scenario has no steps")?;
    Ok(Recording {
        koha_version: pinned.to_owned(),
        scenario,
        inputs,
        outputs,
    })
}

fn pinned_koha_version() -> Result<String, Error> {
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("koha-lock.json")?)?;
    Ok(lock
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or("koha-lock.json has no version")?
        .to_owned())
}
