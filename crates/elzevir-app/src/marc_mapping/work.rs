// SPDX-License-Identifier: GPL-3.0-or-later
//! The Work: category, titles, classification, languages, contributors, subjects (spec §1.1, §1.2).

use elzevir_domain::catalog::{AgentKind, MaterialType, SubjectKind};

use super::{DataField, MappingError, Marc, text};
use crate::catalog::{NewContribution, NewSubject};

/// What the record says about the Work.
pub(super) struct Work {
    pub(super) title: String,
    pub(super) material_type: MaterialType,
    pub(super) classification: Option<String>,
    pub(super) classification_scheme: Option<String>,
    pub(super) summary: Option<String>,
    pub(super) languages: Vec<String>,
    pub(super) contributions: Vec<NewContribution>,
    pub(super) subjects: Vec<NewSubject>,
}

pub(super) fn map(marc: &Marc<'_>) -> Result<Work, MappingError> {
    let title = marc
        .first_text(b"130", b'a')
        .or_else(|| marc.first_text(b"240", b'a'))
        .or_else(|| marc.first_text(b"245", b'a'))
        .ok_or(MappingError::MissingTitle)?;
    let (classification, classification_scheme) = classification(marc);
    Ok(Work {
        title,
        material_type: material_type(marc.leader_byte(6), marc.leader_byte(7)),
        classification,
        classification_scheme,
        summary: marc.first_text(b"520", b'a'),
        languages: languages(marc),
        contributions: contributions(marc),
        subjects: subjects(marc),
    })
}

/// Leader 06 (type of record) and 07 (bibliographic level) to a category.
pub(super) fn material_type(record_type: u8, level: u8) -> MaterialType {
    match record_type {
        b'a' | b't' if matches!(level, b'b' | b'i' | b's') => MaterialType::Periodical,
        b'e' | b'f' => MaterialType::Map,
        b'c' | b'd' => MaterialType::Score,
        b'i' | b'j' => MaterialType::SoundRecording,
        b'g' => MaterialType::Video,
        b'm' => MaterialType::Digital,
        b'k' | b'r' => MaterialType::VisualMaterial,
        b'o' | b'p' => MaterialType::Mixed,
        _ => MaterialType::Book,
    }
}

fn classification(marc: &Marc<'_>) -> (Option<String>, Option<String>) {
    [(b"082", "ddc"), (b"080", "udc"), (b"050", "lcc")]
        .into_iter()
        .find_map(|(tag, scheme)| {
            marc.first_text(tag, b'a')
                .map(|number| (Some(number), Some(scheme.to_owned())))
        })
        .unwrap_or((None, None))
}

fn languages(marc: &Marc<'_>) -> Vec<String> {
    let mut codes: Vec<String> = marc
        .data(b"041")
        .flat_map(|field| field.values(b'a'))
        .filter_map(text::trimmed)
        .flat_map(|joined| split_codes(&joined))
        .collect();
    if codes.is_empty()
        && let Some(code) = marc
            .control(*b"008")
            .and_then(|fixed| fixed.get(35..38))
            .and_then(|code| std::str::from_utf8(code).ok())
            .filter(|code| code.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        codes.push(code.to_ascii_lowercase());
    }
    codes.dedup();
    codes
}

/// `porfre` → `por`, `fre`; an incomplete tail is dropped (spec §4, case 5).
fn split_codes(joined: &str) -> Vec<String> {
    let letters: Vec<char> = joined.chars().filter(char::is_ascii_alphabetic).collect();
    letters
        .chunks_exact(3)
        .map(|code| code.iter().collect::<String>().to_ascii_lowercase())
        .collect()
}

/// Tags of the contribution fields: main entries first, then added entries.
pub(super) const CONTRIBUTION_TAGS: [&[u8; 3]; 6] =
    [b"100", b"110", b"111", b"700", b"710", b"711"];

/// Tags of the subject fields.
pub(super) const SUBJECT_TAGS: [&[u8; 3]; 8] = [
    b"600", b"610", b"611", b"630", b"648", b"650", b"651", b"655",
];

fn contributions(marc: &Marc<'_>) -> Vec<NewContribution> {
    let (main, added): (Vec<_>, Vec<_>) = marc
        .data_in(&CONTRIBUTION_TAGS)
        .filter_map(|(tag, field)| contribution_of(*tag, field))
        .partition(|contribution| contribution.primary);
    main.into_iter().chain(added).collect()
}

/// The contribution one field describes; `1XX` is the main entry.
pub(super) fn contribution_of(tag: [u8; 3], field: DataField<'_>) -> Option<NewContribution> {
    contribution(tag, field, tag[0] == b'1')
}

fn contribution(tag: [u8; 3], field: DataField<'_>, primary: bool) -> Option<NewContribution> {
    let kind = match (tag.get(1..), field.indicators[0]) {
        (Some(b"00"), b'3') => AgentKind::Family,
        (Some(b"00"), _) => AgentKind::Person,
        (Some(b"10"), _) => AgentKind::Organization,
        _ => AgentKind::Meeting,
    };
    let codes: &[u8] = if kind == AgentKind::Meeting {
        b"abcdnq"
    } else {
        b"abcdq"
    };
    // Join first and clean once, so inner punctuation (`Ginzberg, Eli, 1911-2002`) stays.
    let heading = text::clean(field.raw_joined(codes)?.as_bytes())?;
    let role = field
        .raw(b'4')
        .or_else(|| field.text(b'e').map(|role| role.to_lowercase()));
    Some(NewContribution {
        kind,
        heading,
        role,
        primary,
    })
}

fn subjects(marc: &Marc<'_>) -> Vec<NewSubject> {
    marc.data_in(&SUBJECT_TAGS)
        .filter_map(|(tag, field)| subject_of(*tag, field))
        .collect()
}

/// The subject one field describes.
pub(super) fn subject_of(tag: [u8; 3], field: DataField<'_>) -> Option<NewSubject> {
    let kind = match &tag {
        b"630" => SubjectKind::Title,
        b"648" => SubjectKind::Temporal,
        b"650" => SubjectKind::Topic,
        b"651" => SubjectKind::Place,
        b"655" => SubjectKind::GenreForm,
        _ => SubjectKind::Agent,
    };
    subject(kind, field)
}

fn subject(kind: SubjectKind, field: DataField<'_>) -> Option<NewSubject> {
    let main: &[u8] = if kind == SubjectKind::Agent {
        b"abcdt"
    } else {
        b"a"
    };
    let mut heading = text::clean(field.raw_joined(main)?.as_bytes())?;
    if let Some(subdivisions) = field.joined(b"vxyz", " -- ") {
        heading.push_str(" -- ");
        heading.push_str(&subdivisions);
    }
    let thesaurus = match field.indicators[1] {
        b'0' => Some("lcsh".to_owned()),
        b'1' => Some("lcac".to_owned()),
        b'2' => Some("mesh".to_owned()),
        b'3' => Some("nal".to_owned()),
        b'5' => Some("cash".to_owned()),
        b'6' => Some("rvm".to_owned()),
        b'7' => field.raw(b'2'),
        _ => None,
    };
    Some(NewSubject {
        kind,
        heading,
        thesaurus,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leader_picks_the_category() {
        assert_eq!(material_type(b'a', b'm'), MaterialType::Book);
        assert_eq!(material_type(b'a', b's'), MaterialType::Periodical);
        assert_eq!(material_type(b't', b'm'), MaterialType::Book);
        assert_eq!(material_type(b'e', b'm'), MaterialType::Map);
        assert_eq!(material_type(b'c', b'm'), MaterialType::Score);
        assert_eq!(material_type(b'j', b'm'), MaterialType::SoundRecording);
        assert_eq!(material_type(b'g', b'm'), MaterialType::Video);
        assert_eq!(material_type(b'm', b'm'), MaterialType::Digital);
        assert_eq!(material_type(b'k', b'm'), MaterialType::VisualMaterial);
        assert_eq!(material_type(b'p', b'c'), MaterialType::Mixed);
        assert_eq!(material_type(b'z', b'm'), MaterialType::Book);
    }

    #[test]
    fn joined_language_codes_split_in_threes() {
        assert_eq!(split_codes("porfre"), ["por", "fre"]);
        assert_eq!(split_codes("POR"), ["por"]);
        assert_eq!(split_codes("porfr"), ["por"]);
    }
}
