// SPDX-License-Identifier: GPL-3.0-or-later
//! Tables generated from the pinned Koha reference (circulation rule kinds, MARC frameworks, vocabularies).
//!
//! The types in this file are written by hand. The tables live in `src/generated/`, which
//! `cargo xtask codegen` writes from `koha-lock.json` and `data/koha-<series>/`; never edit
//! them by hand. CI runs `cargo xtask codegen --check` to keep them in sync.

mod generated;

pub use generated::circulation_rule_kinds::RuleKind;
pub use generated::koha_version::KOHA_REFERENCE;

/// The Koha release whose behaviour Atrium reproduces, as pinned in `koha-lock.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KohaReference {
    /// `$VERSION` from `Koha.pm`, for example `26.05.05.000`.
    pub version: &'static str,
    /// Git tag of the release in the upstream repository.
    pub tag: &'static str,
    /// Full commit hash the tag points to.
    pub commit: &'static str,
}

/// One dimension a circulation rule can be scoped by.
///
/// Koha: the `scope` arrays of `Koha::CirculationRules::$RULE_KINDS`, where these are
/// spelled `branchcode`, `categorycode` and `itemtype`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleScope {
    /// The library (branch) the rule applies to.
    Branch,
    /// The patron category the rule applies to.
    PatronCategory,
    /// The item type the rule applies to.
    ItemType,
}

/// Static facts about one kind of circulation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleKindInfo {
    /// The rule name as Koha stores it in `circulation_rules.rule_name`.
    pub koha_name: &'static str,
    /// The dimensions this rule is scoped by, in Koha's order.
    pub scope: &'static [RuleScope],
    /// Whether an empty value is kept as empty (`true`) or stored as "no value" (`false`).
    ///
    /// Koha: `Koha::CirculationRules::set_rule` defaults this to true.
    pub can_be_blank: bool,
    /// Whether the value is an amount of money.
    ///
    /// Koha: `Koha::CirculationRules::set_rule` defaults this to false.
    pub is_monetary: bool,
}

impl RuleKind {
    /// The rule name as Koha stores it.
    #[must_use]
    pub const fn koha_name(self) -> &'static str {
        self.info().koha_name
    }

    /// Whether this rule is scoped by `dimension`.
    #[must_use]
    pub fn is_scoped_by(self, dimension: RuleScope) -> bool {
        self.info().scope.contains(&dimension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_koha_name() {
        for kind in RuleKind::ALL {
            assert_eq!(RuleKind::from_koha_name(kind.koha_name()), Some(kind));
        }
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(RuleKind::from_koha_name("no_such_rule"), None);
        assert_eq!(RuleKind::from_koha_name(""), None);
    }

    #[test]
    fn koha_names_are_unique_and_sorted() {
        let names: Vec<_> = RuleKind::ALL.iter().map(|k| k.koha_name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn every_kind_is_scoped_by_branch() {
        for kind in RuleKind::ALL {
            assert!(kind.is_scoped_by(RuleScope::Branch), "{}", kind.koha_name());
        }
    }

    /// Spot checks against `reference/koha/Koha/CirculationRules.pm`, which doc 02
    /// cites as examples of rules with their own scope.
    #[test]
    fn known_scopes_match_koha() {
        assert_eq!(
            RuleKind::MaxHolds.info().scope,
            [RuleScope::Branch, RuleScope::PatronCategory]
        );
        assert_eq!(
            RuleKind::Holdallowed.info().scope,
            [RuleScope::Branch, RuleScope::ItemType]
        );
        assert!(!RuleKind::Holdallowed.info().can_be_blank);
        assert!(RuleKind::Fine.info().is_monetary);
        assert!(!RuleKind::Issuelength.info().is_monetary);
    }

    #[test]
    fn reference_is_the_pinned_stable_release() {
        assert_eq!(KOHA_REFERENCE.version, "26.05.05.000");
        assert_eq!(KOHA_REFERENCE.commit.len(), 40);
    }
}
