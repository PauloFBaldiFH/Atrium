// SPDX-License-Identifier: GPL-3.0-or-later
//! Tables generated from the pinned Koha reference (circulation rule kinds, MARC frameworks, vocabularies)
//! and from the Library of Congress MARC-8 code tables.
//!
//! The types in this file are written by hand. The tables live in `src/generated/`, which
//! `cargo xtask codegen` writes from `koha-lock.json`, `data/koha-<series>/` and `data/loc/`; never edit
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

/// A generated MARC-8 row: `(code, unicode, alternative, combining)`.
pub(crate) type Marc8Row = (u32, Option<char>, Option<char>, bool);

/// One character of a MARC-8 graphic set, as the Library of Congress code tables define it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marc8Character {
    /// The preferred Unicode character. `None` only for the second half of the two-part
    /// ligature and double tilde, which have no Unicode counterpart of their own.
    pub unicode: Option<char>,
    /// The alternative Unicode mapping the tables give (for example the half-mark
    /// `U+FE21` for the ligature's second half, or the geta mark `U+3013` for EACC codes
    /// whose preferred mapping is a private-use character).
    pub alternative: Option<char>,
    /// Whether the character is a combining mark. MARC-8 writes combining marks
    /// before their base character; Unicode writes them after.
    pub combining: bool,
}

/// The character for `code` in the MARC-8 graphic set whose ISO code (the final byte
/// of its escape sequence) is `set`, for example `0x45` for Extended Latin (ANSEL) or
/// `0x31` for EACC. `code` is written as the code tables write it: one byte for most
/// sets, three bytes for EACC (`0x213021`). Most sets list their G0 form (`0x21`–`0x7e`),
/// but ANSEL, Extended Cyrillic and Extended Arabic list their G1 form (`0xa1`–`0xfe`,
/// so `0xe1` is ANSEL's combining grave), and ANSEL adds the C1 controls `0x88`, `0x89`,
/// `0x8d` and `0x8e`. Mapping bytes read from a record to these codes is the decoder's job.
///
/// Source: the Library of Congress MARC-8 code tables (`data/loc/marc8.json`).
#[must_use]
pub fn marc8_char(set: u8, code: u32) -> Option<Marc8Character> {
    let sets = generated::marc8::SETS;
    let at = sets.binary_search_by_key(&set, |&(iso, _)| iso).ok()?;
    let (_, rows) = sets.get(at)?;
    let row = rows.binary_search_by_key(&code, |&(marc, ..)| marc).ok()?;
    rows.get(row)
        .map(|&(_, unicode, alternative, combining)| Marc8Character {
            unicode,
            alternative,
            combining,
        })
}

/// How a search field's values are indexed.
///
/// Koha: the `type` of a search field in `mappings.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchFieldKind {
    /// No type: full-text with Koha's default analysis.
    Default,
    /// Text kept as written (exact match, facets).
    String,
    /// A number.
    Number,
    /// A date.
    Date,
    /// True or false.
    Boolean,
    /// A call number, sorted by its classification order.
    CallNumber,
    /// An ISBN, matched in its 10- and 13-digit forms.
    Isbn,
    /// A standard number (ISSN, control numbers).
    StandardNumber,
    /// A number summed over the record's items.
    Sum,
    /// Latitude or longitude.
    GeoPoint,
}

/// One search field and the MARC21 data that feeds it.
///
/// Koha: a `search_fields` row with its `search_marc_to_field` mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchField {
    /// Name used in queries, for example `title`.
    pub name: &'static str,
    /// Label in the interface.
    pub label: &'static str,
    /// How values are indexed.
    pub kind: SearchFieldKind,
    /// Searchable in the OPAC.
    pub opac: bool,
    /// Searchable in the staff interface.
    pub staff_client: bool,
    /// Koha refuses to delete it.
    pub mandatory: bool,
    /// Position among the facets, if it is one.
    pub facet_order: Option<u8>,
    /// MARC21 sources, possibly none.
    pub mappings: &'static [SearchMapping],
}

/// One MARC21 source of a search field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMapping {
    /// Koha's field spec: a tag, subfield codes, and optionally positions, as in
    /// `245abp`, `100(abcd)` or `008_/35-37`.
    pub marc_field: &'static str,
    /// Feeds the facet.
    pub facet: bool,
    /// Feeds sorting.
    pub sort: bool,
    /// Feeds search suggestions.
    pub suggestible: bool,
}

/// Koha's default search fields for bibliographic records, sorted by name.
#[must_use]
pub fn biblio_search_fields() -> &'static [SearchField] {
    generated::search_fields::BIBLIOS
}

/// Koha's default search fields for authority records, sorted by name.
#[must_use]
pub fn authority_search_fields() -> &'static [SearchField] {
    generated::search_fields::AUTHORITIES
}
