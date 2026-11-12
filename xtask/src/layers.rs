// SPDX-License-Identifier: GPL-3.0-or-later
//! Layer check: compares the workspace dependency graph with `tools/layers.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Allowed Atrium dependencies per workspace crate.
pub(crate) type Rules = BTreeMap<String, BTreeSet<String>>;

/// Workspace crates and the workspace crates each one depends on.
pub(crate) type Graph = BTreeMap<String, BTreeSet<String>>;

/// Loads the rules and the real graph, and fails with every violation found.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    let rules_text = std::fs::read_to_string(root.join("tools/layers.json"))
        .map_err(|e| format!("cannot read tools/layers.json: {e}"))?;
    let rules = parse_rules(&rules_text)?;
    let graph = workspace_graph(root)?;
    let violations = violations(&rules, &graph);
    if violations.is_empty() {
        println!(
            "check-layers: {} crates follow tools/layers.json",
            graph.len()
        );
        Ok(())
    } else {
        Err(format!("layer violations:\n  {}", violations.join("\n  ")))
    }
}

/// Parses the `crates` object of `tools/layers.json`.
pub(crate) fn parse_rules(text: &str) -> Result<Rules, String> {
    let json: Value =
        serde_json::from_str(text).map_err(|e| format!("tools/layers.json is not JSON: {e}"))?;
    let crates = json
        .get("crates")
        .and_then(Value::as_object)
        .ok_or("tools/layers.json has no `crates` object")?;
    crates
        .iter()
        .map(|(name, allowed)| {
            let allowed = allowed
                .as_array()
                .ok_or(format!("`{name}` must map to an array of crate names"))?
                .iter()
                .map(|dep| {
                    dep.as_str()
                        .map(str::to_owned)
                        .ok_or(format!("`{name}` lists a non-string dependency"))
                })
                .collect::<Result<BTreeSet<_>, _>>()?;
            Ok((name.clone(), allowed))
        })
        .collect()
}

/// Reads the workspace dependency graph from `cargo metadata`.
fn workspace_graph(root: &Path) -> Result<Graph, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cargo metadata returned invalid JSON: {e}"))?;
    graph_from_metadata(&json)
}

/// Builds the graph from `cargo metadata --no-deps` output, keeping only
/// dependencies that are themselves workspace members.
pub(crate) fn graph_from_metadata(json: &Value) -> Result<Graph, String> {
    let packages = json
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("cargo metadata has no `packages` array")?;
    let names: BTreeSet<&str> = packages
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .collect();
    let mut graph = Graph::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .ok_or("package without a name")?;
        let deps = package
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|deps| {
                deps.iter()
                    .filter_map(|d| d.get("name").and_then(Value::as_str))
                    .filter(|d| names.contains(d))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        graph.insert(name.to_owned(), deps);
    }
    Ok(graph)
}

/// Lists every crate missing from the rules and every forbidden edge.
pub(crate) fn violations(rules: &Rules, graph: &Graph) -> Vec<String> {
    let mut found = Vec::new();
    for (krate, deps) in graph {
        let Some(allowed) = rules.get(krate) else {
            found.push(format!("`{krate}` is not listed in tools/layers.json"));
            continue;
        };
        for dep in deps.difference(allowed) {
            found.push(format!("`{krate}` must not depend on `{dep}`"));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    const RULES: &str = r#"{ "crates": {
        "atrium-domain": [],
        "atrium-app": ["atrium-domain"],
        "atrium-web": ["atrium-app"]
    } }"#;

    #[test]
    fn allowed_graph_has_no_violations() {
        let rules = parse_rules(RULES).unwrap();
        let graph = Graph::from([
            ("atrium-domain".to_owned(), set(&[])),
            ("atrium-app".to_owned(), set(&["atrium-domain"])),
            ("atrium-web".to_owned(), set(&["atrium-app"])),
        ]);
        assert!(violations(&rules, &graph).is_empty());
    }

    #[test]
    fn forbidden_dependency_is_reported() {
        let rules = parse_rules(RULES).unwrap();
        let graph = Graph::from([("atrium-domain".to_owned(), set(&["atrium-web"]))]);
        assert_eq!(
            violations(&rules, &graph),
            ["`atrium-domain` must not depend on `atrium-web`"]
        );
    }

    #[test]
    fn unlisted_crate_is_reported() {
        let rules = parse_rules(RULES).unwrap();
        let graph = Graph::from([("atrium-new".to_owned(), set(&[]))]);
        assert_eq!(
            violations(&rules, &graph),
            ["`atrium-new` is not listed in tools/layers.json"]
        );
    }

    #[test]
    fn metadata_keeps_only_workspace_dependencies() {
        let json: Value = serde_json::from_str(
            r#"{ "packages": [
                { "name": "atrium-app", "dependencies": [
                    { "name": "atrium-domain" }, { "name": "serde" } ] },
                { "name": "atrium-domain", "dependencies": [] }
            ] }"#,
        )
        .unwrap();
        let graph = graph_from_metadata(&json).unwrap();
        assert_eq!(graph["atrium-app"], set(&["atrium-domain"]));
    }

    #[test]
    fn repository_layers_file_is_valid() {
        let text = include_str!("../../tools/layers.json");
        let rules = parse_rules(text).unwrap();
        assert!(rules.contains_key("atrium-domain"));
    }
}
