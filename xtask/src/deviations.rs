// SPDX-License-Identifier: GPL-3.0-or-later
//! Validates `docs/desvios-conhecidos.md` (task 0.12), the list of intentional differences
//! from Koha that the scenario executor accepts.

use std::collections::BTreeMap;
use std::path::Path;

const FILE: &str = "docs/desvios-conhecidos.md";
const AREAS: [&str; 6] = ["circulacao", "multas", "reservas", "marc", "sip2", "z3950"];
const REQUIRED: [&str; 6] = ["id", "area", "koha", "scenarios", "decided_by", "date"];

/// One parsed deviation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Deviation {
    pub(crate) id: String,
    pub(crate) fields: BTreeMap<String, String>,
}

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let text =
        std::fs::read_to_string(root.join(FILE)).map_err(|e| format!("cannot read {FILE}: {e}"))?;
    let deviations = parse(&text)?;
    println!(
        "check-deviations: {} deviations in {FILE}",
        deviations.len()
    );
    Ok(())
}

/// Parses every `## DEV-NNNN: ...` section after the `## Desvios` heading.
pub(crate) fn parse(text: &str) -> Result<Vec<Deviation>, String> {
    let Some((_, list)) = text.split_once("\n## Desvios\n") else {
        return Err(format!("{FILE} has no `## Desvios` section"));
    };
    let mut deviations = Vec::new();
    for section in list.split("\n## ").skip(1) {
        let (title, body) = section.split_once('\n').unwrap_or((section, ""));
        let title_id = title.split(':').next().unwrap_or_default().trim();
        let deviation = parse_section(title_id, body)?;
        let expected = format!("DEV-{:04}", deviations.len() + 1);
        if deviation.id != expected {
            return Err(format!(
                "`{}` is out of order; expected `{expected}`",
                deviation.id
            ));
        }
        deviations.push(deviation);
    }
    Ok(deviations)
}

fn parse_section(title_id: &str, body: &str) -> Result<Deviation, String> {
    let blocks: Vec<&str> = body.split("```deviation\n").skip(1).collect();
    let [block] = blocks.as_slice() else {
        return Err(format!(
            "`{title_id}` must have exactly one ```deviation block"
        ));
    };
    let content = block
        .split_once("```")
        .map(|(content, _)| content)
        .ok_or(format!("`{title_id}`: unterminated ```deviation block"))?;
    let mut fields = BTreeMap::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or(format!("`{title_id}`: `{line}` is not `key: value`"))?;
        if fields
            .insert(key.trim().to_owned(), value.trim().to_owned())
            .is_some()
        {
            return Err(format!("`{title_id}`: key `{}` appears twice", key.trim()));
        }
    }
    for key in REQUIRED {
        if fields.get(key).is_none_or(String::is_empty) {
            return Err(format!("`{title_id}`: missing `{key}`"));
        }
    }
    if let Some(extra) = fields.keys().find(|k| !REQUIRED.contains(&k.as_str())) {
        return Err(format!("`{title_id}`: unknown key `{extra}`"));
    }
    let id = fields.get("id").cloned().unwrap_or_default();
    if id != title_id {
        return Err(format!("title says `{title_id}` but id is `{id}`"));
    }
    let area = fields.get("area").map(String::as_str).unwrap_or_default();
    if !AREAS.contains(&area) {
        return Err(format!("`{id}`: area `{area}` is not one of {AREAS:?}"));
    }
    let date = fields.get("date").map(String::as_str).unwrap_or_default();
    if !is_iso_date(date) {
        return Err(format!("`{id}`: date `{date}` is not AAAA-MM-DD"));
    }
    Ok(Deviation { id, fields })
}

fn is_iso_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && [y, m, d].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(sections: &str) -> String {
        format!("# Desvios\n\n## Formato\n\nignored ```deviation\nx\n```\n\n## Desvios\n{sections}")
    }

    const GOOD: &str = "\n## DEV-0001: Exemplo\n\n```deviation\nid: DEV-0001\narea: circulacao\nkoha: C4::Circulation::CalcDateDue\nscenarios: scenarios/circulacao/x.yaml\ndecided_by: Paulo\ndate: 2026-11-12\n```\n\nMotivo.\n";

    #[test]
    fn repository_file_is_valid() {
        let text = include_str!("../../docs/desvios-conhecidos.md");
        assert!(parse(text).is_ok());
    }

    #[test]
    fn a_complete_entry_is_accepted() {
        let parsed = parse(&doc(GOOD)).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].fields["koha"], "C4::Circulation::CalcDateDue");
    }

    #[test]
    fn missing_keys_wrong_area_and_bad_order_are_rejected() {
        let missing = GOOD.replace("decided_by: Paulo\n", "");
        assert!(
            parse(&doc(&missing))
                .unwrap_err()
                .contains("missing `decided_by`")
        );

        let area = GOOD.replace("area: circulacao", "area: catalogo");
        assert!(parse(&doc(&area)).unwrap_err().contains("area `catalogo`"));

        let order = GOOD.replace("DEV-0001", "DEV-0002");
        assert!(parse(&doc(&order)).unwrap_err().contains("out of order"));

        let date = GOOD.replace("2026-11-12", "12/11/2026");
        assert!(parse(&doc(&date)).unwrap_err().contains("not AAAA-MM-DD"));
    }
}
