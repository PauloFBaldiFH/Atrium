// SPDX-License-Identifier: GPL-3.0-or-later
//! Differential scenarios (docs/arquitetura/05-definicoes-de-implementacao.md, §4).
//!
//! A scenario (`scenarios/<area>/<name>.yaml`) says what to do; the oracle recording next to it
//! (`<name>.koha.json`) holds the instant Koha ran it, the effective inputs Koha used and
//! Koha's outputs. [`check`] recomputes the outputs with the Elzevir domain from those inputs.

#![forbid(unsafe_code)]

mod model;
mod replay;

pub use model::{
    CalendarSnapshot, Checkout, Expected, Given, ItemGiven, LoanRecorded, PatronGiven,
    RecordedInputs, RecordedOutput, Recording, RuleSnapshot, Scenario, Step,
};
pub use replay::{CheckError, check};

use std::path::{Path, PathBuf};

/// A scenario file and the recording next to it.
#[derive(Debug)]
pub struct ScenarioFiles {
    /// `scenarios/<area>/<name>.yaml`.
    pub scenario: PathBuf,
    /// `scenarios/<area>/<name>.koha.json`.
    pub recording: PathBuf,
}

/// Every scenario under `root` (one level of area directories), sorted by path.
///
/// # Errors
///
/// Fails if a directory cannot be listed.
pub fn find(root: &Path) -> std::io::Result<Vec<ScenarioFiles>> {
    let mut found = Vec::new();
    for area in std::fs::read_dir(root)? {
        let area = area?.path();
        if !area.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&area)? {
            let scenario = entry?.path();
            if scenario.extension().is_some_and(|e| e == "yaml") {
                let recording = scenario.with_extension("koha.json");
                found.push(ScenarioFiles {
                    scenario,
                    recording,
                });
            }
        }
    }
    found.sort_by(|a, b| a.scenario.cmp(&b.scenario));
    Ok(found)
}

/// A scenario or recording file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// File.
        path: PathBuf,
        /// Cause.
        source: std::io::Error,
    },
    /// The scenario is not valid YAML for [`Scenario`].
    #[error("invalid scenario {path}: {source}")]
    Scenario {
        /// File.
        path: PathBuf,
        /// Cause.
        source: serde_norway::Error,
    },
    /// The recording is not valid JSON for [`Recording`].
    #[error("invalid recording {path}: {source}")]
    Recording {
        /// File.
        path: PathBuf,
        /// Cause.
        source: serde_json::Error,
    },
}

/// Reads a scenario.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_scenario(path: &Path) -> Result<Scenario, LoadError> {
    let text = read(path)?;
    serde_norway::from_str(&text).map_err(|source| LoadError::Scenario {
        path: path.to_owned(),
        source,
    })
}

/// Reads a recording.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_recording(path: &Path) -> Result<Recording, LoadError> {
    let text = read(path)?;
    serde_json::from_str(&text).map_err(|source| LoadError::Recording {
        path: path.to_owned(),
        source,
    })
}

fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })
}
