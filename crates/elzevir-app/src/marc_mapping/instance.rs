// SPDX-License-Identifier: GPL-3.0-or-later
//! The Instance: title, publication, extent, series, identifiers, category (spec §1.1, §1.3).

use elzevir_domain::catalog::{IdentifierKind, MaterialType};

use super::{DataField, Marc, text};
use crate::catalog::{NewIdentifier, NewInstance};

pub(super) fn map(marc: &Marc<'_>, work_material_type: MaterialType) -> NewInstance {
    let publication = marc
        .data(b"264")
        .find(|field| field.indicators[1] == b'1')
        .or_else(|| marc.first(b"260"));
    NewInstance {
        title: marc.first(b"245").map(title).unwrap_or_default(),
        material_type: electronic(marc, work_material_type).then_some(MaterialType::Digital),
        edition: marc.first_text(b"250", b'a'),
        place: publication.and_then(|field| field.text(b'a')),
        publisher: publication.and_then(|field| field.text(b'b')),
        date: publication.and_then(|field| field.text(b'c')),
        extent: marc
            .first(b"300")
            .and_then(|field| field.raw_joined(b"abc")),
        series: series(marc),
        identifiers: identifiers(marc),
    }
}

/// `245`: `$a`, `$n`, `$p` in field order, then ` : ` and `$b`.
fn title(field: DataField<'_>) -> String {
    let main = field.joined(b"anp", " ").unwrap_or_default();
    match field.text(b'b') {
        Some(rest) if !main.is_empty() => format!("{main} : {rest}"),
        Some(rest) => rest,
        None => main,
    }
}

/// Whether the record describes an electronic resource while its Work is not `Digital`.
fn electronic(marc: &Marc<'_>, work: MaterialType) -> bool {
    if work == MaterialType::Digital {
        return false;
    }
    let physical = marc.control(*b"007").and_then(|value| value.first()) == Some(&b'c');
    let form_applies = matches!(
        work,
        MaterialType::Book | MaterialType::Periodical | MaterialType::Score | MaterialType::Mixed
    );
    let form = marc
        .control(*b"008")
        .and_then(|value| value.get(23))
        .copied();
    physical || (form_applies && matches!(form, Some(b'o' | b'q' | b's')))
}

fn series(marc: &Marc<'_>) -> Option<String> {
    if let Some(field) = marc.first(b"490")
        && let Some(name) = field.text(b'a')
    {
        return Some(match field.text(b'v') {
            Some(number) => format!("{name} ; {number}"),
            None => name,
        });
    }
    marc.first_text(b"830", b'a')
        .or_else(|| marc.first_text(b"440", b'a'))
}

/// Tags of the identifier fields.
pub(super) const IDENTIFIER_TAGS: [&[u8; 3]; 5] = [b"010", b"020", b"022", b"024", b"035"];

fn identifiers(marc: &Marc<'_>) -> Vec<NewIdentifier> {
    marc.data_in(&IDENTIFIER_TAGS)
        .filter_map(|(tag, field)| identifier_of(*tag, field))
        .collect()
}

/// The identifier one field holds, if it is one the model knows.
pub(super) fn identifier_of(tag: [u8; 3], field: DataField<'_>) -> Option<NewIdentifier> {
    let kind = match &tag {
        b"010" => IdentifierKind::Lccn,
        b"020" => IdentifierKind::Isbn,
        b"022" => IdentifierKind::Issn,
        b"035" => IdentifierKind::SystemControlNumber,
        _ => standard_number_kind(field)?,
    };
    let written = field.raw(b'a')?;
    let (value, in_parentheses) = split_qualifier(&written);
    let qualifier = in_parentheses.or_else(|| field.raw(b'q'));
    Some(NewIdentifier {
        kind,
        value,
        qualifier,
    })
}

/// `024`: the first indicator (and `$2`) says which standard number it is.
fn standard_number_kind(field: DataField<'_>) -> Option<IdentifierKind> {
    match field.indicators[0] {
        b'7' if field
            .raw(b'2')
            .is_some_and(|source| source.eq_ignore_ascii_case("doi")) =>
        {
            Some(IdentifierKind::Doi)
        }
        b'2' => Some(IdentifierKind::Ismn),
        b'3' => Some(IdentifierKind::Ean),
        _ => None,
    }
}

/// `8535910662 (broch.)` → value `8535910662`, qualifier `(broch.)`.
fn split_qualifier(written: &str) -> (String, Option<String>) {
    match written.split_once(" (") {
        Some((value, rest)) => (
            value.trim().to_owned(),
            text::trimmed(format!("({rest}").as_bytes()),
        ),
        None => (written.to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn_qualifiers_in_parentheses_are_split_off() {
        assert_eq!(
            split_qualifier("8535910662 (broch.)"),
            ("8535910662".to_owned(), Some("(broch.)".to_owned()))
        );
        assert_eq!(
            split_qualifier("8535910662"),
            ("8535910662".to_owned(), None)
        );
    }
}
