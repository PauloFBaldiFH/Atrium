// SPDX-License-Identifier: GPL-3.0-or-later
//! The bibliographic model: Hub, Work, Instance, Item, Agent, Subject
//! (docs/specs/catalogo/modelo-bibliografico.md, ADR 0001 and 0002).
//!
//! This module holds the identities and the fixed vocabularies. Every fixed vocabulary
//! has a stable number, which is what the database stores; never renumber a variant.

/// Declares an `i64` row identifier newtype.
macro_rules! row_id {
    ($(#[$doc:meta] $name:ident),+ $(,)?) => {$(
        #[$doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
            serde::Serialize, serde::Deserialize,
        )]
        pub struct $name(pub i64);
    )+};
}

row_id! {
    /// A Hub: groups Works that share an origin (translations, adaptations, uniform title).
    HubId,
    /// A Work: the intellectual content.
    WorkId,
    /// An Instance: one published edition of a Work.
    InstanceId,
    /// An Item: one physical copy of an Instance.
    ItemId,
    /// An Agent: a person, organization, meeting or family.
    AgentId,
    /// A Subject heading.
    SubjectId,
    /// A library branch (Koha `branches`).
    BranchId,
    /// An item type (Koha `itemtypes`).
    ItemTypeId,
    /// A code in one of the [`Vocabulary`] lists.
    TermId,
}

/// Declares a fixed vocabulary with a stable storage number per variant.
macro_rules! stable_enum {
    (
        $(#[$doc:meta])* $name:ident {
            $($(#[$variant_doc:meta])* $variant:ident = $code:literal,)+
        }
    ) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
            serde::Serialize, serde::Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$variant_doc])* $variant,)+
        }

        impl $name {
            /// Every variant, in storage-number order.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The number stored in the database.
            #[must_use]
            pub const fn code(self) -> u8 {
                match self {
                    $(Self::$variant => $code,)+
                }
            }

            /// The variant stored as `code`; `None` for a number this build does not know.
            #[must_use]
            pub const fn from_code(code: u8) -> Option<Self> {
                match code {
                    $($code => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

stable_enum! {
    /// What kind of entity an Agent is (BIBFRAME `Person`, `Organization`, `Meeting`,
    /// `Family`; MARC21 X00, X10, X11 and X00 with first indicator 3).
    AgentKind {
        /// A person.
        Person = 1,
        /// An organization (corporate body).
        Organization = 2,
        /// A meeting or conference.
        Meeting = 3,
        /// A family.
        Family = 4,
    }
}

stable_enum! {
    /// What a Subject heading names (MARC21 6XX tags).
    SubjectKind {
        /// A topic (650).
        Topic = 1,
        /// A geographic name (651).
        Place = 2,
        /// A person, organization, meeting or family as subject (600, 610, 611).
        Agent = 3,
        /// A uniform title as subject (630).
        Title = 4,
        /// A genre or form (655).
        GenreForm = 5,
        /// A chronological term (648).
        Temporal = 6,
    }
}

stable_enum! {
    /// Kinds of Instance identifier.
    IdentifierKind {
        /// ISBN (MARC21 020).
        Isbn = 1,
        /// ISSN (MARC21 022).
        Issn = 2,
        /// Library of Congress control number (MARC21 010).
        Lccn = 3,
        /// EAN or other standard number (MARC21 024).
        Ean = 4,
        /// Control number from another system (MARC21 035).
        SystemControlNumber = 5,
        /// Digital Object Identifier (MARC21 024, first indicator 7, source `doi`).
        Doi = 6,
        /// International Standard Music Number (MARC21 024, first indicator 2).
        Ismn = 7,
    }
}

stable_enum! {
    /// The vocabularies kept in the shared `terms` table. Branches and item types have
    /// tables of their own because circulation rules point at them.
    Vocabulary {
        /// Language codes (MARC21 language codes, as in 008/35-37 and 041).
        Language = 1,
        /// Relator terms and codes for contributions (MARC21 $e and $4).
        Relator = 2,
        /// Shelving locations (Koha authorised values `LOC`).
        Location = 3,
        /// Collections (Koha authorised values `CCODE`).
        Collection = 4,
        /// Classification schemes (Koha `class_sources`, such as `ddc`).
        ClassificationScheme = 5,
        /// Subject thesauri (MARC21 6XX second indicator and $2).
        Thesaurus = 6,
    }
}

stable_enum! {
    /// How a preserved source record is encoded.
    SourceFormat {
        /// ISO 2709 (MARC21), UTF-8.
        Marc21Iso2709 = 1,
    }
}

stable_enum! {
    /// The category of material of a Work or an Instance. It picks the icon and the
    /// cataloguing profile (the 12 to 20 fields the editor shows for that category).
    MaterialType {
        /// Books and other printed or manuscript text.
        Book = 1,
        /// Periodicals, newspapers and other continuing resources.
        Periodical = 2,
        /// Maps, atlases and other cartographic material.
        Map = 3,
        /// Notated music.
        Score = 4,
        /// Sound recordings, musical or spoken.
        SoundRecording = 5,
        /// Films, videos and other moving images.
        Video = 6,
        /// Electronic resources: e-books, files, websites, software.
        Digital = 7,
        /// Still images, objects and other visual material.
        VisualMaterial = 8,
        /// Kits and mixed materials.
        Mixed = 9,
    }
}

stable_enum! {
    /// How one Work relates to another. Stored one way only: the Work that holds the
    /// row is the translation, the adaptation, the part or the continuation.
    WorkRelationKind {
        /// This Work is a translation of the related Work.
        TranslationOf = 1,
        /// This Work is an adaptation of the related Work.
        AdaptationOf = 2,
        /// This Work is part of the related Work.
        PartOf = 3,
        /// This Work is a supplement to the related Work.
        SupplementTo = 4,
        /// This Work continues the related Work.
        ContinuationOf = 5,
    }
}

stable_enum! {
    /// A rule for finding a stored record equal to one being imported (Koha's default
    /// matching rules, docs/specs/catalogo/casamento.md).
    MatchRule {
        /// Same ISBN, normalized to ISBN-13.
        Isbn = 1,
        /// Same ISSN.
        Issn = 2,
        /// Same Koha `biblionumber` (`999 $c` of the source record).
        KohaBiblionumber = 3,
    }
}

/// The ISBN as a match key: ISBN-13 digits when `text` holds a valid ISBN-10 or ISBN-13,
/// otherwise `None`. Only digits and `X` count, so hyphens, spaces and stray words are
/// ignored.
///
/// Koha: `Koha::Util::Normalize::ISBN` (`C4::Koha::NormalizeISBN` with `ISBN-13` and no
/// hyphens). Koha also checks the group and publisher ranges; this checks the digit.
#[must_use]
pub fn isbn13(text: &str) -> Option<String> {
    let symbols: Vec<u32> = text
        .chars()
        .filter_map(|c| match c {
            'X' | 'x' => Some(10),
            _ => c.to_digit(10),
        })
        .collect();
    match symbols.as_slice() {
        [first9 @ .., check] if symbols.len() == 10 => {
            if first9.contains(&10) || isbn10_check(first9) != *check {
                return None;
            }
            let mut digits = vec![9, 7, 8];
            digits.extend_from_slice(first9);
            let check = isbn13_check(&digits);
            digits.push(check);
            Some(digits_to_string(&digits))
        }
        [first12 @ .., check] if symbols.len() == 13 => {
            let prefix_ok = first12.starts_with(&[9, 7, 8]) || first12.starts_with(&[9, 7, 9]);
            (prefix_ok && !symbols.contains(&10) && isbn13_check(first12) == *check)
                .then(|| digits_to_string(&symbols))
        }
        _ => None,
    }
}

/// Check digit of an ISBN-10 from its first nine digits; 10 stands for `X`.
fn isbn10_check(digits: &[u32]) -> u32 {
    let sum: u32 = digits.iter().zip((2..=10).rev()).map(|(d, w)| d * w).sum();
    (11 - sum % 11) % 11
}

/// Check digit of an ISBN-13 (EAN-13) from its first twelve digits.
fn isbn13_check(digits: &[u32]) -> u32 {
    let sum: u32 = digits
        .iter()
        .zip([1, 3].into_iter().cycle())
        .map(|(d, w)| d * w)
        .sum();
    (10 - sum % 10) % 10
}

fn digits_to_string(digits: &[u32]) -> String {
    digits
        .iter()
        .filter_map(|&d| char::from_digit(d, 10))
        .collect()
}

/// An amount of money in cents. Never a float.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Money(pub i64);

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trips<T: Copy + PartialEq + std::fmt::Debug>(
        all: &[T],
        code: fn(T) -> u8,
        from: fn(u8) -> Option<T>,
    ) {
        for &variant in all {
            assert_eq!(from(code(variant)), Some(variant));
        }
        let mut codes: Vec<u8> = all.iter().map(|&v| code(v)).collect();
        codes.dedup();
        assert_eq!(codes.len(), all.len(), "codes must be unique and ascending");
        assert_eq!(from(0), None);
        assert_eq!(from(u8::MAX), None);
    }

    #[test]
    fn stable_codes_round_trip_and_reject_unknown_numbers() {
        round_trips(AgentKind::ALL, AgentKind::code, AgentKind::from_code);
        round_trips(SubjectKind::ALL, SubjectKind::code, SubjectKind::from_code);
        round_trips(
            IdentifierKind::ALL,
            IdentifierKind::code,
            IdentifierKind::from_code,
        );
        round_trips(Vocabulary::ALL, Vocabulary::code, Vocabulary::from_code);
        round_trips(
            MaterialType::ALL,
            MaterialType::code,
            MaterialType::from_code,
        );
        round_trips(
            WorkRelationKind::ALL,
            WorkRelationKind::code,
            WorkRelationKind::from_code,
        );
        round_trips(
            SourceFormat::ALL,
            SourceFormat::code,
            SourceFormat::from_code,
        );
        round_trips(MatchRule::ALL, MatchRule::code, MatchRule::from_code);
    }

    #[test]
    fn isbns_become_isbn13() {
        // ISBN-10 with and without hyphens, and with X as check digit.
        assert_eq!(isbn13("85-60342-01-X").as_deref(), Some("9788560342013"));
        assert_eq!(isbn13("856034201x").as_deref(), Some("9788560342013"));
        assert_eq!(isbn13("0-8044-2957-X").as_deref(), Some("9780804429573"));
        assert_eq!(isbn13("080442957x").as_deref(), Some("9780804429573"));
        // ISBN-13 stays, hyphens and spaces go.
        assert_eq!(
            isbn13("978-85-60342-01-3").as_deref(),
            Some("9788560342013")
        );
        assert_eq!(
            isbn13("979 10 90636 07 1").as_deref(),
            Some("9791090636071")
        );
        // Wrong check digit, wrong prefix, wrong length, X inside.
        assert_eq!(isbn13("8560342013"), None);
        assert_eq!(isbn13("9778560342013"), None);
        assert_eq!(isbn13("978856034201"), None);
        assert_eq!(isbn13("85603X2016"), None);
        assert_eq!(isbn13(""), None);
    }
}
