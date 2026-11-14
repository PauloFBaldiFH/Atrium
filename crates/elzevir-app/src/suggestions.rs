// SPDX-License-Identifier: GPL-3.0-or-later
//! Existing agents and subjects suggested while cataloguing (docs/specs/catalogo/edicao.md,
//! items 33 to 37), so that a name links to the entity already there instead of creating a
//! duplicate.

use elzevir_domain::catalog::{AgentKind, SubjectKind};

use crate::catalog::CatalogReadError;

/// How many suggestions a lookup returns at most.
pub const SUGGESTION_LIMIT: usize = 10;

/// Fewer characters than this suggest nothing.
pub const MIN_PREFIX: usize = 2;

/// An existing agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSuggestion {
    /// Authorized name.
    pub heading: String,
    /// Person, organization, meeting or family.
    pub kind: AgentKind,
}

/// An existing subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectSuggestion {
    /// Heading.
    pub heading: String,
    /// What it names.
    pub kind: SubjectKind,
    /// Thesaurus code, when it has one.
    pub thesaurus: Option<String>,
}

/// Port: looks up existing agents and subjects by the start of their name. The store
/// implements it.
pub trait AuthoritySuggestions {
    /// Agents whose name starts with `prefix`, ignoring ASCII case, in name order.
    ///
    /// # Errors
    ///
    /// When storage fails or holds a value this build does not understand.
    fn agents(&self, prefix: &str, limit: usize) -> Result<Vec<AgentSuggestion>, CatalogReadError>;

    /// Subjects whose heading starts with `prefix`, ignoring ASCII case, in heading order.
    ///
    /// # Errors
    ///
    /// When storage fails or holds a value this build does not understand.
    fn subjects(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<SubjectSuggestion>, CatalogReadError>;
}

/// The prefix to look up, trimmed; `None` when it is too short to suggest anything.
#[must_use]
pub fn lookup_prefix(typed: &str) -> Option<&str> {
    let prefix = typed.trim();
    (prefix.chars().count() >= MIN_PREFIX).then_some(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_or_blank_input_suggests_nothing() {
        assert_eq!(lookup_prefix(" a "), None);
        assert_eq!(lookup_prefix("   "), None);
        assert_eq!(lookup_prefix(" Ma "), Some("Ma"));
        assert_eq!(lookup_prefix("Ão"), Some("Ão"));
    }
}
