// SPDX-License-Identifier: GPL-3.0-or-later
//! The generated MARC21 framework matches facts checked by hand in Koha's YAML.

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
