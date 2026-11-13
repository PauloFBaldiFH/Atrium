// SPDX-License-Identifier: GPL-3.0-or-later
//! Matching an imported record with a stored one, and what to do then
//! (docs/specs/catalogo/casamento.md).

use elzevir_domain::catalog::{IdentifierKind, MatchRule, isbn13};
use elzevir_marc::{Limits, RecordRef, Tag};
use serde::{Deserialize, Serialize};

use crate::catalog::NewRecord;
use crate::search::normalized_identifier;

/// One match key of a record: records with the same rule and key are the same record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchKey {
    /// The rule the key belongs to.
    pub rule: MatchRule,
    /// The normalized key.
    pub key: String,
}

/// Every match key of `record`, under every rule, without repeats.
#[must_use]
pub fn match_keys(record: &NewRecord) -> Vec<MatchKey> {
    let mut keys: Vec<MatchKey> = record
        .instance
        .identifiers
        .iter()
        .filter_map(|identifier| identifier_key(identifier.kind, &identifier.value))
        .chain(record.source.as_deref().and_then(koha_biblionumber_key))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The match key of one Instance identifier, if its kind has a rule.
///
/// Koha: `C4::Matcher::_get_match_keys` with the `ISBN` normalization for 020 `$a`; ISSN
/// keys lose hyphens and spaces.
#[must_use]
pub fn identifier_key(kind: IdentifierKind, value: &str) -> Option<MatchKey> {
    let (rule, key) = match kind {
        IdentifierKind::Isbn => (
            MatchRule::Isbn,
            isbn13(value).unwrap_or_else(|| normalized_identifier(kind, value)),
        ),
        IdentifierKind::Issn => (MatchRule::Issn, normalized_identifier(kind, value)),
        _ => return None,
    };
    (!key.is_empty()).then_some(MatchKey { rule, key })
}

/// The Koha `biblionumber` of a MARC21 source record: its first `999 $c`, when it is a
/// number.
#[must_use]
pub fn koha_biblionumber_key(source: &[u8]) -> Option<MatchKey> {
    let record = RecordRef::parse(source, &Limits::default()).ok()?;
    let field = record.fields_with_tag(Tag::new(*b"999")).next()?;
    let value = field.subfield_values(b'c').next()?;
    let key = std::str::from_utf8(value).ok()?.trim();
    (!key.is_empty() && key.bytes().all(|b| b.is_ascii_digit())).then(|| MatchKey {
        rule: MatchRule::KohaBiblionumber,
        key: key.to_owned(),
    })
}

/// What to do with an imported record that matches a stored one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnMatch {
    /// Keep the stored record as it is.
    Ignore,
    /// Store the imported record as a new one anyway.
    CreateNew,
}

/// What to do with an imported record that matches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnNoMatch {
    /// Store it as a new record.
    CreateNew,
    /// Do not store it.
    Ignore,
}

/// What to do with the Items of an imported record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemAction {
    /// Store them whether the record matched or not.
    AlwaysAdd,
    /// Store them only when the record matched (on the matched Instance).
    AddOnlyForMatches,
    /// Store them only when the record did not match.
    AddOnlyForNew,
    /// Never store them.
    Ignore,
}

/// How an import matches records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matching {
    /// The rule that finds the stored record.
    pub rule: MatchRule,
    /// When a stored record matches.
    pub on_match: OnMatch,
    /// When nothing matches.
    pub on_no_match: OnNoMatch,
    /// What happens to the Items.
    pub items: ItemAction,
}

impl Matching {
    /// Matching by `rule` that keeps stored records, creates the ones without a match and
    /// adds every Item: importing the same file again adds nothing but new Items.
    #[must_use]
    pub const fn new(rule: MatchRule) -> Self {
        Self {
            rule,
            on_match: OnMatch::Ignore,
            on_no_match: OnNoMatch::CreateNew,
            items: ItemAction::AlwaysAdd,
        }
    }
}

/// What happens to the record itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordAction {
    /// A new record is stored.
    CreateNew,
    /// Nothing is stored for the record.
    Ignore,
}

/// The decision for one imported record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// What happens to the record.
    pub record: RecordAction,
    /// Whether its Items are stored: on the new record, or on the matched one when the
    /// record is ignored.
    pub add_items: bool,
}

/// Decides what to do with a record, given whether it matched a stored one.
///
/// Koha: `C4::ImportBatch::_get_commit_action` (biblio records).
#[must_use]
pub const fn decide(matching: &Matching, matched: bool) -> Decision {
    if matched {
        Decision {
            record: match matching.on_match {
                OnMatch::Ignore => RecordAction::Ignore,
                OnMatch::CreateNew => RecordAction::CreateNew,
            },
            add_items: matches!(
                matching.items,
                ItemAction::AlwaysAdd | ItemAction::AddOnlyForMatches
            ),
        }
    } else {
        Decision {
            record: match matching.on_no_match {
                OnNoMatch::CreateNew => RecordAction::CreateNew,
                OnNoMatch::Ignore => RecordAction::Ignore,
            },
            add_items: matches!(
                matching.items,
                ItemAction::AlwaysAdd | ItemAction::AddOnlyForNew
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use elzevir_marc::{Field, Record, Subfield};

    use super::*;
    use crate::catalog::{NewIdentifier, NewInstance};

    fn record(identifiers: &[(IdentifierKind, &str)], source: Option<Vec<u8>>) -> NewRecord {
        NewRecord {
            work_title: "Dom Casmurro".to_owned(),
            material_type: elzevir_domain::catalog::MaterialType::Book,
            hub: None,
            relations: Vec::new(),
            classification: None,
            classification_scheme: None,
            summary: None,
            languages: Vec::new(),
            contributions: Vec::new(),
            subjects: Vec::new(),
            instance: NewInstance {
                title: "Dom Casmurro".to_owned(),
                identifiers: identifiers
                    .iter()
                    .map(|(kind, value)| NewIdentifier {
                        kind: *kind,
                        value: (*value).to_owned(),
                        qualifier: None,
                    })
                    .collect(),
                ..NewInstance::default()
            },
            items: Vec::new(),
            source,
        }
    }

    fn marc_with_999(c: &str) -> Vec<u8> {
        let mut marc = Record {
            leader: *b"00000nam a2200000 a 4500",
            fields: Vec::new(),
        };
        marc.fields.push(Field::Data {
            tag: Tag::new(*b"999"),
            indicators: *b"  ",
            subfields: vec![Subfield {
                code: b'c',
                value: c.as_bytes().to_vec(),
            }],
        });
        let mut bytes = Vec::new();
        marc.write_iso2709(&mut bytes).unwrap();
        bytes
    }

    fn key(rule: MatchRule, key: &str) -> MatchKey {
        MatchKey {
            rule,
            key: key.to_owned(),
        }
    }

    #[test]
    fn keys_come_from_identifiers_and_the_koha_biblionumber() {
        let new = record(
            &[
                (IdentifierKind::Isbn, "85-60342-01-X"),
                (IdentifierKind::Isbn, "978-85-60342-01-3"),
                (IdentifierKind::Isbn, "not an isbn"),
                (IdentifierKind::Issn, "0034-7116"),
                (IdentifierKind::SystemControlNumber, "(OCoLC)123"),
            ],
            Some(marc_with_999(" 42 ")),
        );
        assert_eq!(
            match_keys(&new),
            [
                key(MatchRule::Isbn, "9788560342013"),
                key(MatchRule::Isbn, "NOTANISBN"),
                key(MatchRule::Issn, "00347116"),
                key(MatchRule::KohaBiblionumber, "42"),
            ]
        );
        assert!(koha_biblionumber_key(&marc_with_999("12a")).is_none());
        assert!(koha_biblionumber_key(b"garbage").is_none());
        assert!(match_keys(&record(&[], None)).is_empty());
    }

    #[test]
    fn decisions_follow_koha() {
        use ItemAction::{AddOnlyForMatches, AddOnlyForNew, AlwaysAdd, Ignore};
        let with = |items, on_match, on_no_match| Matching {
            rule: MatchRule::Isbn,
            on_match,
            on_no_match,
            items,
        };
        let expected = [
            // items, add when matched, add when new
            (AlwaysAdd, true, true),
            (AddOnlyForMatches, true, false),
            (AddOnlyForNew, false, true),
            (Ignore, false, false),
        ];
        for (items, matched_adds, new_adds) in expected {
            let matching = with(items, OnMatch::Ignore, OnNoMatch::CreateNew);
            assert_eq!(
                decide(&matching, true),
                Decision {
                    record: RecordAction::Ignore,
                    add_items: matched_adds
                },
                "{items:?}"
            );
            assert_eq!(
                decide(&matching, false),
                Decision {
                    record: RecordAction::CreateNew,
                    add_items: new_adds
                },
                "{items:?}"
            );
        }
        let flipped = with(AlwaysAdd, OnMatch::CreateNew, OnNoMatch::Ignore);
        assert_eq!(decide(&flipped, true).record, RecordAction::CreateNew);
        assert_eq!(decide(&flipped, false).record, RecordAction::Ignore);
    }
}
