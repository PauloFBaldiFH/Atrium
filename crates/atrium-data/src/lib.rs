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

/// One tag of a MARC bibliographic framework: what the cataloguing editor shows and checks.
///
/// Koha: a row of `marc_tag_structure` with its `marc_subfield_structure` rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarcFieldDefinition {
    /// Three-digit tag, for example `245`.
    pub tag: &'static str,
    /// Label in the staff interface.
    pub label: &'static str,
    /// Label in the OPAC.
    pub label_opac: &'static str,
    /// Whether the tag may appear more than once in a record.
    pub repeatable: bool,
    /// Whether every record must have it.
    pub mandatory: bool,
    /// Authorised value category for the tag as a whole, rarely used.
    pub authorised_value: Option<&'static str>,
    /// Subfields, letters before digits.
    pub subfields: &'static [MarcSubfieldDefinition],
}

/// One subfield of a framework tag. Control fields have a single subfield `@`.
///
/// Koha: a row of `marc_subfield_structure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarcSubfieldDefinition {
    /// Subfield code; `@` stands for the whole control field.
    pub code: char,
    /// Label in the staff interface.
    pub label: &'static str,
    /// Label in the OPAC.
    pub label_opac: &'static str,
    /// Whether it may appear more than once in a field.
    pub repeatable: bool,
    /// Whether the field must have it.
    pub mandatory: bool,
    /// The Koha database column it fills, for example `biblio.title`.
    pub kohafield: Option<&'static str>,
    /// Editor tab; `-1` means ignored.
    pub tab: Option<i8>,
    /// Koha's visibility code (which interfaces show it).
    pub hidden: Option<i8>,
    /// Authorised value category whose values it takes.
    pub authorised_value: Option<&'static str>,
    /// Authority type it links to.
    pub authtypecode: Option<&'static str>,
    /// Koha plugin that fills it in the editor.
    pub value_builder: Option<&'static str>,
    /// Whether the value is a URL.
    pub is_url: bool,
    /// Maximum length of the value.
    pub max_length: u16,
}

/// Every tag of Koha's default MARC21 bibliographic framework, sorted by tag.
pub fn marc21_framework() -> impl Iterator<Item = &'static MarcFieldDefinition> {
    MARC21_HUNDREDS.iter().flat_map(|fields| fields.iter())
}

/// The default MARC21 framework's definition of `tag`, if it defines it.
#[must_use]
pub fn marc21_field(tag: &str) -> Option<&'static MarcFieldDefinition> {
    let hundred = tag.as_bytes().first()?.checked_sub(b'0')?;
    let fields = MARC21_HUNDREDS.get(usize::from(hundred))?;
    let at = fields.binary_search_by(|field| field.tag.cmp(tag)).ok()?;
    fields.get(at)
}

impl MarcFieldDefinition {
    /// The definition of subfield `code`, if the framework defines it.
    #[must_use]
    pub fn subfield(&self, code: char) -> Option<&'static MarcSubfieldDefinition> {
        self.subfields.iter().find(|subfield| subfield.code == code)
    }
}

static MARC21_HUNDREDS: [&[MarcFieldDefinition]; 10] = [
    generated::marc21_framework_0xx::FIELDS,
    generated::marc21_framework_1xx::FIELDS,
    generated::marc21_framework_2xx::FIELDS,
    generated::marc21_framework_3xx::FIELDS,
    generated::marc21_framework_4xx::FIELDS,
    generated::marc21_framework_5xx::FIELDS,
    generated::marc21_framework_6xx::FIELDS,
    generated::marc21_framework_7xx::FIELDS,
    generated::marc21_framework_8xx::FIELDS,
    generated::marc21_framework_9xx::FIELDS,
];

/// The Unicode character for an ISO 5426 code: a single byte, or a diacritic byte followed
/// by its base byte (`0xc141` is capital A with grave).
///
/// Koha: the `%chars` table of `C4::Charset::char_decode5426`.
#[must_use]
pub fn iso5426_char(code: u16) -> Option<char> {
    let table = generated::iso5426::ISO5426;
    let at = table.binary_search_by_key(&code, |&(c, _)| c).ok()?;
    table.get(at).map(|&(_, character)| character)
}
