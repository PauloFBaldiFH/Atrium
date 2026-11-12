// SPDX-License-Identifier: GPL-3.0-or-later
//! The generated Koha tables match facts checked by hand in the Koha sources.

use atrium_data::{marc21_field, marc21_framework};

#[test]
fn has_every_default_tag_sorted() {
    let tags: Vec<&str> = marc21_framework().map(|field| field.tag).collect();
    assert_eq!(tags.len(), 356);
    assert!(tags.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(tags.first(), Some(&"000"));
    assert_eq!(tags.last(), Some(&"999"));
}

#[test]
fn title_statement() {
    let title = marc21_field("245").unwrap();
    assert_eq!(title.label, "TITLE STATEMENT");
    assert!(!title.repeatable);
    let a = title.subfield('a').unwrap();
    assert_eq!(a.kohafield, Some("biblio.title"));
    assert_eq!(
        title.subfield('b').unwrap().kohafield,
        Some("biblio.subtitle")
    );
}

#[test]
fn items_field_and_control_fields() {
    let items = marc21_field("952").unwrap();
    assert!(items.repeatable);
    assert_eq!(
        items.subfield('a').unwrap().kohafield,
        Some("items.homebranch")
    );
    let leader = marc21_field("000").unwrap();
    assert_eq!(leader.subfield('@').unwrap().max_length, 24);
    assert_eq!(
        marc21_field("008")
            .unwrap()
            .subfield('@')
            .unwrap()
            .max_length,
        40
    );
    assert_eq!(
        marc21_field("245")
            .unwrap()
            .subfield('a')
            .unwrap()
            .max_length,
        9999
    );
}

#[test]
fn unknown_tags_are_none() {
    assert!(marc21_field("").is_none());
    assert!(marc21_field("x45").is_none());
    assert!(marc21_field("2450").is_none());
}

#[test]
fn iso5426_table() {
    use atrium_data::iso5426_char;
    assert_eq!(iso5426_char(0xc141), Some('À'));
    assert_eq!(iso5426_char(0xc161), Some('à'));
    assert_eq!(iso5426_char(0x0097), Some('<'));
    assert_eq!(iso5426_char(0x0041), None);
}

#[test]
fn search_fields() {
    use atrium_data::{SearchFieldKind, authority_search_fields, biblio_search_fields};
    assert_eq!(biblio_search_fields().len(), 155);
    assert_eq!(authority_search_fields().len(), 30);
    let title = biblio_search_fields()
        .iter()
        .find(|field| field.name == "title")
        .unwrap();
    assert_eq!(title.kind, SearchFieldKind::String);
    assert!(title.mappings.iter().any(|m| m.marc_field == "245abp"));
    let holding = biblio_search_fields()
        .iter()
        .find(|field| field.name == "holdingbranch")
        .unwrap();
    assert_eq!(holding.facet_order, Some(8));
    assert_eq!(holding.mappings.len(), 1);
    assert!(holding.mappings[0].facet);
}
