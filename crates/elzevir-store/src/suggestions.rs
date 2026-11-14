// SPDX-License-Identifier: GPL-3.0-or-later
//! Agents and subjects by the start of their name (docs/specs/catalogo/edicao.md, items 33
//! to 37).

use elzevir_app::catalog::CatalogReadError;
use elzevir_app::suggestions::{AgentSuggestion, AuthoritySuggestions, SubjectSuggestion};
use elzevir_domain::catalog::{AgentKind, SubjectKind};
use rusqlite::{Connection, params};

use crate::ElzevirStore;
use crate::catalog::ReadFailure;

impl AuthoritySuggestions for ElzevirStore {
    fn agents(&self, prefix: &str, limit: usize) -> Result<Vec<AgentSuggestion>, CatalogReadError> {
        self.readers()
            .read(|c| agents(c, prefix, limit))
            .map_err(|e| CatalogReadError(e.to_string()))
    }

    fn subjects(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<SubjectSuggestion>, CatalogReadError> {
        self.readers()
            .read(|c| subjects(c, prefix, limit))
            .map_err(|e| CatalogReadError(e.to_string()))
    }
}

/// `prefix` as a `LIKE` pattern: its own `%`, `_` and `\\` match only themselves.
fn starts_with(prefix: &str) -> String {
    let mut pattern = String::with_capacity(prefix.len() + 1);
    for character in prefix.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

fn limit_of(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn agents(c: &Connection, prefix: &str, limit: usize) -> Result<Vec<AgentSuggestion>, ReadFailure> {
    // LIKE ignores ASCII case, and the NOCASE index of migration 10 serves it.
    let mut statement = c.prepare_cached(
        "SELECT heading, kind FROM agents WHERE heading LIKE ?1 ESCAPE '\\'
         ORDER BY heading COLLATE NOCASE, kind LIMIT ?2",
    )?;
    let rows = statement.query_map(params![starts_with(prefix), limit_of(limit)], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?))
    })?;
    rows.map(|row| {
        let (heading, kind) = row?;
        let kind = AgentKind::from_code(kind).ok_or(ReadFailure::UnknownCode {
            what: "agent kind",
            code: kind,
        })?;
        Ok(AgentSuggestion { heading, kind })
    })
    .collect()
}

fn subjects(
    c: &Connection,
    prefix: &str,
    limit: usize,
) -> Result<Vec<SubjectSuggestion>, ReadFailure> {
    let mut statement = c.prepare_cached(
        "SELECT subjects.heading, subjects.kind, terms.code FROM subjects
         LEFT JOIN terms ON terms.id = subjects.thesaurus
         WHERE subjects.heading LIKE ?1 ESCAPE '\\'
         ORDER BY subjects.heading COLLATE NOCASE, subjects.kind LIMIT ?2",
    )?;
    let rows = statement.query_map(params![starts_with(prefix), limit_of(limit)], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u8>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    rows.map(|row| {
        let (heading, kind, thesaurus) = row?;
        let kind = SubjectKind::from_code(kind).ok_or(ReadFailure::UnknownCode {
            what: "subject kind",
            code: kind,
        })?;
        Ok(SubjectSuggestion {
            heading,
            kind,
            thesaurus,
        })
    })
    .collect()
}
