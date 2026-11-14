// SPDX-License-Identifier: GPL-3.0-or-later
//! Cataloguing profiles: which parts of the model a category of material asks for, in
//! which order (UI spec §4.2; docs/specs/catalogo/edicao.md, items 25 to 27). Each field
//! names a part of the model, never a MARC tag.

use elzevir_domain::catalog::{IdentifierKind, MaterialType};

/// A part of a Work that a profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkField {
    /// Preferred title.
    Title,
    /// Category of material.
    MaterialType,
    /// Contributors with their roles.
    Contributions,
    /// Classification number and scheme.
    Classification,
    /// Language codes.
    Languages,
    /// Subjects.
    Subjects,
    /// Summary.
    Summary,
    /// Target audience.
    Audience,
}

/// A part of an Instance that a profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceField {
    /// Title as on the publication.
    Title,
    /// Edition statement.
    Edition,
    /// Place of publication.
    Place,
    /// Publisher.
    Publisher,
    /// Date of publication.
    Date,
    /// Physical description.
    Extent,
    /// Series statement.
    Series,
    /// Identifiers.
    Identifiers,
    /// Notes.
    Notes,
    /// Category of material.
    MaterialType,
}

/// One profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// The category it was written for.
    pub material: MaterialType,
    /// The Work's fields, in order.
    pub work: &'static [WorkField],
    /// The Instance's fields, in order.
    pub instance: &'static [InstanceField],
    /// The kind a new identifier starts as.
    pub identifier: IdentifierKind,
}

const WORK: &[WorkField] = &[
    WorkField::Title,
    WorkField::MaterialType,
    WorkField::Contributions,
    WorkField::Classification,
    WorkField::Languages,
    WorkField::Subjects,
    WorkField::Summary,
    WorkField::Audience,
];

const INSTANCE: &[InstanceField] = &[
    InstanceField::Title,
    InstanceField::Edition,
    InstanceField::Place,
    InstanceField::Publisher,
    InstanceField::Date,
    InstanceField::Extent,
    InstanceField::Series,
    InstanceField::Identifiers,
    InstanceField::Notes,
    InstanceField::MaterialType,
];

/// Books (item 26).
pub const BOOK: Profile = Profile {
    material: MaterialType::Book,
    work: WORK,
    instance: INSTANCE,
    identifier: IdentifierKind::Isbn,
};

/// Periodicals: a new identifier is an ISSN. Frequency and numbering wait for the model.
pub const PERIODICAL: Profile = Profile {
    material: MaterialType::Periodical,
    work: WORK,
    instance: INSTANCE,
    identifier: IdentifierKind::Issn,
};

/// The profile of a category; categories without their own use the book profile.
#[must_use]
pub const fn profile_for(material: MaterialType) -> &'static Profile {
    match material {
        MaterialType::Periodical => &PERIODICAL,
        _ => &BOOK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_profile_lists_a_field_once_and_has_between_12_and_20_fields() {
        for profile in [&BOOK, &PERIODICAL] {
            let total = profile.work.len() + profile.instance.len();
            assert!(
                (12..=20).contains(&total),
                "{:?} has {total}",
                profile.material
            );
            for (index, field) in profile.work.iter().enumerate() {
                assert!(!profile.work[index + 1..].contains(field));
            }
            for (index, field) in profile.instance.iter().enumerate() {
                assert!(!profile.instance[index + 1..].contains(field));
            }
        }
    }

    #[test]
    fn periodicals_start_new_identifiers_as_issn() {
        assert_eq!(
            profile_for(MaterialType::Periodical).identifier,
            IdentifierKind::Issn
        );
        assert_eq!(profile_for(MaterialType::Map).material, MaterialType::Book);
    }
}
