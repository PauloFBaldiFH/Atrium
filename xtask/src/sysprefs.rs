// SPDX-License-Identifier: GPL-3.0-or-later
//! Every Koha system preference has a destination in Elzevir (task 0.11).
//!
//! Compares the inventory extracted from Koha (`data/koha-<series>/sysprefs.json`) with the
//! triage (`tools/sysprefs-triage.json`). A Koha upgrade that adds a preference fails CI
//! until someone decides where it goes.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

const TRIAGE: &str = "tools/sysprefs-triage.json";
const DESTINATIONS: [&str; 7] = [
    "config",
    "regra",
    "fixo",
    "interface",
    "plataforma",
    "fora-v1",
    "revisar",
];

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let lock = read(&root.join("koha-lock.json"))?;
    let series = lock
        .get("series")
        .and_then(Value::as_str)
        .ok_or("koha-lock.json has no series")?;
    let inventory = read(&root.join(format!("data/koha-{series}/sysprefs.json")))?;
    let triage = read(&root.join(TRIAGE))?;
    let problems = check(&inventory, &triage)?;
    if problems.is_empty() {
        println!("check-sysprefs: every Koha preference has a destination in {TRIAGE}");
        Ok(())
    } else {
        Err(format!(
            "{TRIAGE} is out of date:\n  {}",
            problems.join("\n  ")
        ))
    }
}

/// Lists preferences without a valid destination and triage entries Koha no longer has.
pub(crate) fn check(inventory: &Value, triage: &Value) -> Result<Vec<String>, String> {
    let known: BTreeSet<&str> = inventory
        .get("sysprefs")
        .and_then(Value::as_array)
        .ok_or("inventory has no sysprefs array")?
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .collect();
    let entries = triage
        .get("preferencias")
        .and_then(Value::as_object)
        .ok_or("triage has no preferencias object")?;

    let mut problems = Vec::new();
    for name in &known {
        match entries
            .get(*name)
            .and_then(|e| e.get("destino"))
            .and_then(Value::as_str)
        {
            None => problems.push(format!("`{name}` has no destination")),
            Some(d) if !DESTINATIONS.contains(&d) => {
                problems.push(format!("`{name}` has unknown destination `{d}`"));
            }
            Some(_) => {}
        }
    }
    for name in entries.keys() {
        if !known.contains(name.as_str()) {
            problems.push(format!("`{name}` is not a Koha preference any more"));
        }
    }
    Ok(problems)
}

fn read(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not JSON: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn missing_unknown_and_stale_entries_are_reported() {
        let inventory =
            json(r#"{ "sysprefs": [ { "name": "A" }, { "name": "B" }, { "name": "C" } ] }"#);
        let triage = json(
            r#"{ "preferencias": {
                "A": { "destino": "config" },
                "B": { "destino": "maybe" },
                "Old": { "destino": "fixo" }
            } }"#,
        );
        let problems = check(&inventory, &triage).unwrap();
        assert_eq!(
            problems,
            [
                "`B` has unknown destination `maybe`",
                "`C` has no destination",
                "`Old` is not a Koha preference any more"
            ]
        );
    }
}
