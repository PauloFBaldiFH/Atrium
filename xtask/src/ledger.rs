// SPDX-License-Identifier: GPL-3.0-or-later
//! Port ledger v0 (task 0.10): which Koha subs the Atrium code says it reproduces.
//!
//! The convention (CLAUDE.md) is a doc comment `/// Koha: C4::Module::sub` on the Rust item
//! that ports the behaviour. This command lists every Koha sub in `reference/koha` and every
//! such comment, and reports coverage per Koha module.
//!
//! - `cargo xtask ledger`: coverage report (Markdown).
//! - `cargo xtask ledger where <Koha::Name>`: Atrium places that cite a Koha name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A citation of a Koha name in Atrium code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Citation {
    pub(crate) koha_name: String,
    pub(crate) file: PathBuf,
    pub(crate) line: usize,
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let koha = root.join("reference/koha");
    if !koha.join("Koha.pm").is_file() {
        return Err(
            "reference/koha is missing: run `git submodule update --init --depth 1 reference/koha`"
                .to_owned(),
        );
    }
    let subs = koha_subs(&koha)?;
    let citations = atrium_citations(root)?;
    match args {
        [] => {
            print!("{}", report(&subs, &citations));
            Ok(())
        }
        [command, name] if command == "where" => {
            let found: Vec<_> = citations.iter().filter(|c| c.koha_name == *name).collect();
            if found.is_empty() {
                println!("{name}: not cited in Atrium");
            }
            for c in found {
                println!("{}:{}", c.file.display(), c.line);
            }
            Ok(())
        }
        _ => Err("usage: cargo xtask ledger [where <Koha::Name>]".to_owned()),
    }
}

/// Every `Package::sub` defined in `C4/` and `Koha/`.
fn koha_subs(koha: &Path) -> Result<BTreeSet<String>, String> {
    let mut files = Vec::new();
    for dir in ["C4", "Koha"] {
        collect(&koha.join(dir), "pm", &mut files)?;
    }
    let mut subs = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        subs.extend(subs_in(&text));
    }
    Ok(subs)
}

/// Subs declared in one Perl source, qualified by the current `package`.
pub(crate) fn subs_in(perl: &str) -> Vec<String> {
    let mut package = String::new();
    let mut subs = Vec::new();
    for line in perl.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("package ") {
            package.clear();
            package.push_str(
                rest.split(|c: char| c == ';' || c.is_whitespace() || c == '{')
                    .next()
                    .unwrap_or_default(),
            );
        } else if let Some(rest) = line.strip_prefix("sub ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !package.is_empty() {
                subs.push(format!("{package}::{name}"));
            }
        }
    }
    subs
}

/// Every Koha name cited in a `Koha:` doc comment under `crates/`.
fn atrium_citations(root: &Path) -> Result<Vec<Citation>, String> {
    let mut files = Vec::new();
    collect(&root.join("crates"), "rs", &mut files)?;
    let mut citations = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let shown = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        for (index, line) in text.lines().enumerate() {
            for koha_name in cited_in(line) {
                citations.push(Citation {
                    koha_name,
                    file: shown.clone(),
                    line: index + 1,
                });
            }
        }
    }
    Ok(citations)
}

/// Koha names in one line, if it is a doc comment containing `Koha:`.
pub(crate) fn cited_in(line: &str) -> Vec<String> {
    let line = line.trim_start();
    let is_doc = line.starts_with("///") || line.starts_with("//!");
    let Some((_, after)) = line.split_once("Koha:").filter(|_| is_doc) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for start in ["C4::", "Koha::"] {
        let mut rest = after;
        while let Some(at) = rest.find(start) {
            let tail = rest.get(at..).unwrap_or_default();
            let name: String = tail
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            let name = name.trim_end_matches(':').to_owned();
            rest = tail.get(name.len().max(1)..).unwrap_or_default();
            // `Koha::` inside `C4::...` or a longer name was already taken.
            let preceded = after
                .get(..after.len() - tail.len())
                .is_some_and(|before| before.ends_with("::"));
            if !preceded && name.contains("::") {
                names.push(name);
            }
        }
    }
    names
}

/// Coverage report grouped by Koha module (`C4::Circulation`, `Koha::Patron`, ...).
fn report(subs: &BTreeSet<String>, citations: &[Citation]) -> String {
    let cited: BTreeSet<&str> = citations.iter().map(|c| c.koha_name.as_str()).collect();
    let ported: BTreeSet<&str> = cited
        .iter()
        .copied()
        .filter(|n| subs.contains(*n))
        .collect();
    let other: Vec<&str> = cited
        .iter()
        .copied()
        .filter(|n| !subs.contains(*n))
        .collect();

    let mut modules: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for sub in subs {
        let module = sub.rsplit_once("::").map_or(sub.as_str(), |(m, _)| m);
        let entry = modules.entry(module).or_default();
        entry.0 += 1;
        if ported.contains(sub.as_str()) {
            entry.1 += 1;
        }
    }

    let mut lines = vec![
        "# Port ledger".to_owned(),
        String::new(),
        format!(
            "Koha subs: {}. Ported (cited with `/// Koha:`): {}.",
            subs.len(),
            ported.len()
        ),
        String::new(),
        "## Modules with ported subs".to_owned(),
        String::new(),
        "| Module | Ported | Subs |".to_owned(),
        "|---|---|---|".to_owned(),
    ];
    for (module, (total, done)) in modules.iter().filter(|(_, (_, done))| *done > 0) {
        lines.push(format!("| {module} | {done} | {total} |"));
    }
    lines.extend([String::new(), "## Ported subs".to_owned(), String::new()]);
    lines.extend(ported.iter().map(|name| format!("- {name}")));
    if !other.is_empty() {
        lines.extend([
            String::new(),
            "## Cited names that are not subs (variables, modules or typos)".to_owned(),
            String::new(),
        ]);
        lines.extend(other.iter().map(|name| format!("- {name}")));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn collect(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
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
            if path.file_name().is_none_or(|n| n != "target") {
                collect(&path, extension, out)?;
            }
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subs_are_qualified_by_package() {
        let perl = "package C4::Circulation;\nuse strict;\nsub CalcDateDue {\n}\nsub _private { }\npackage Koha::Foo;\nsub bar;\n";
        assert_eq!(
            subs_in(perl),
            [
                "C4::Circulation::CalcDateDue",
                "C4::Circulation::_private",
                "Koha::Foo::bar"
            ]
        );
    }

    #[test]
    fn citations_come_only_from_doc_comments() {
        assert_eq!(
            cited_in("/// Koha: C4::Circulation::CalcDateDue"),
            ["C4::Circulation::CalcDateDue"]
        );
        assert_eq!(
            cited_in("    //! Koha: `Koha::CirculationRules::$RULE_KINDS` in a file."),
            ["Koha::CirculationRules"]
        );
        assert_eq!(
            cited_in("/// Koha: C4::Overdues::CalcFine and C4::Circulation::AddIssue."),
            ["C4::Overdues::CalcFine", "C4::Circulation::AddIssue"]
        );
        assert!(cited_in("// Koha: C4::Circulation::CalcDateDue").is_empty());
        assert!(cited_in("/// Uses C4::Circulation::CalcDateDue").is_empty());
    }
}
