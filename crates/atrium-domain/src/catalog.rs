// SPDX-License-Identifier: GPL-3.0-or-later
//! The bibliographic model: Work, Instance, Item, Agent, Subject
//! (docs/specs/catalogo/modelo-bibliografico.md, ADR 0001).
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
            SourceFormat::ALL,
            SourceFormat::code,
            SourceFormat::from_code,
        );
    }
}
