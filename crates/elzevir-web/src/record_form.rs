// SPDX-License-Identifier: GPL-3.0-or-later
//! The form of a focused Work or Instance, built from its profile
//! (docs/specs/catalogo/edicao.md, items 25 to 32). Each field's `change` and each
//! column's `name` are the edit's own JSON names, so the script sends what the route reads.

use elzevir_app::catalog::NewRecord;
use elzevir_app::profiles::{InstanceField, WorkField, profile_for};
use elzevir_domain::catalog::{AgentKind, IdentifierKind, MaterialType, NoteKind, SubjectKind};
use elzevir_web_templates::{
    ColumnKind, FormCell, FormColumn, FormField, FormInput, FormOption, I18n, RecordForm,
};

use crate::opac::material_key;

/// The serde names of the vocabularies, with their labels. A unit test checks each name
/// against the type's own serde form.
const MATERIALS: &[(MaterialType, &str)] = &[
    (MaterialType::Book, "book"),
    (MaterialType::Periodical, "periodical"),
    (MaterialType::Map, "map"),
    (MaterialType::Score, "score"),
    (MaterialType::SoundRecording, "sound_recording"),
    (MaterialType::Video, "video"),
    (MaterialType::Digital, "digital"),
    (MaterialType::VisualMaterial, "visual_material"),
    (MaterialType::Mixed, "mixed"),
];

const AGENTS: &[(AgentKind, &str, &str)] = &[
    (AgentKind::Person, "person", "staff-agent-person"),
    (
        AgentKind::Organization,
        "organization",
        "staff-agent-organization",
    ),
    (AgentKind::Meeting, "meeting", "staff-agent-meeting"),
    (AgentKind::Family, "family", "staff-agent-family"),
];

const SUBJECTS: &[(SubjectKind, &str, &str)] = &[
    (SubjectKind::Topic, "topic", "staff-subject-topic"),
    (SubjectKind::Place, "place", "staff-subject-place"),
    (SubjectKind::Agent, "agent", "staff-subject-agent"),
    (SubjectKind::Title, "title", "staff-subject-title"),
    (
        SubjectKind::GenreForm,
        "genre_form",
        "staff-subject-genre-form",
    ),
    (SubjectKind::Temporal, "temporal", "staff-subject-temporal"),
];

const IDENTIFIERS: &[(IdentifierKind, &str, &str)] = &[
    (IdentifierKind::Isbn, "isbn", "staff-identifier-isbn"),
    (IdentifierKind::Issn, "issn", "staff-identifier-issn"),
    (IdentifierKind::Lccn, "lccn", "staff-identifier-lccn"),
    (IdentifierKind::Ean, "ean", "staff-identifier-ean"),
    (
        IdentifierKind::SystemControlNumber,
        "system_control_number",
        "staff-identifier-system-control-number",
    ),
    (IdentifierKind::Doi, "doi", "staff-identifier-doi"),
    (IdentifierKind::Ismn, "ismn", "staff-identifier-ismn"),
];

const NOTES: &[(NoteKind, &str, &str)] = &[
    (NoteKind::General, "general", "staff-fields-note-general"),
    (NoteKind::Contents, "contents", "staff-fields-note-contents"),
    (NoteKind::Local, "local", "staff-fields-note-local"),
];

fn name_of<T: PartialEq + Copy>(
    table: &[(T, &'static str, &'static str)],
    value: T,
) -> &'static str {
    table
        .iter()
        .find(|(variant, _, _)| *variant == value)
        .map_or("", |(_, name, _)| name)
}

fn options<T>(t: &I18n, table: &[(T, &'static str, &'static str)]) -> Vec<FormOption> {
    table
        .iter()
        .map(|(_, value, key)| FormOption {
            value,
            label: t.get(key),
        })
        .collect()
}

fn material_options(t: &I18n) -> Vec<FormOption> {
    MATERIALS
        .iter()
        .map(|(material, value)| FormOption {
            value,
            label: t.get(material_key(*material)),
        })
        .collect()
}

fn material_name(material: MaterialType) -> &'static str {
    MATERIALS
        .iter()
        .find(|(variant, _)| *variant == material)
        .map_or("", |(_, name)| name)
}

const fn column(
    name: &'static str,
    label_key: &'static str,
    kind: ColumnKind,
    optional: bool,
) -> FormColumn {
    FormColumn {
        name,
        label_key,
        kind,
        optional,
    }
}

fn cells(columns: &[FormColumn], values: Vec<String>) -> Vec<FormCell> {
    columns
        .iter()
        .cloned()
        .zip(values)
        .map(|(column, value)| FormCell { column, value })
        .collect()
}

fn rows(columns: &[FormColumn], stored: Vec<Vec<String>>, blank: Vec<String>) -> FormInput {
    FormInput::Rows {
        rows: stored.into_iter().map(|row| cells(columns, row)).collect(),
        blank: cells(columns, blank),
    }
}

fn text(value: Option<&str>) -> FormInput {
    FormInput::Text {
        value: value.unwrap_or_default().to_owned(),
        required: false,
    }
}

fn field(
    change: &'static str,
    error: &'static str,
    label_key: &'static str,
    input: FormInput,
) -> (FormField, bool) {
    let filled = match &input {
        FormInput::Text { value, .. }
        | FormInput::LongText { value }
        | FormInput::Select { value, .. } => !value.is_empty(),
        FormInput::Rows { rows, .. } => !rows.is_empty(),
        FormInput::Values { values } => !values.is_empty(),
        FormInput::Group { cells } => cells.iter().any(|cell| !cell.value.is_empty()),
    };
    (
        FormField {
            change,
            error,
            label_key,
            input,
        },
        filled,
    )
}

fn work_field(t: &I18n, record: &NewRecord, part: WorkField) -> (FormField, bool) {
    match part {
        WorkField::Title => field(
            "Title",
            "title",
            "staff-fields-preferred-title",
            FormInput::Text {
                value: record.work_title.clone(),
                required: true,
            },
        ),
        WorkField::MaterialType => field(
            "MaterialType",
            "material_type",
            "staff-fields-material",
            FormInput::Select {
                value: material_name(record.material_type).to_owned(),
                options: material_options(t),
            },
        ),
        WorkField::Contributions => contributions_field(t, record),
        WorkField::Classification => classification_field(record),
        WorkField::Languages => field(
            "Languages",
            "languages",
            "staff-fields-languages",
            FormInput::Values {
                values: record.languages.clone(),
            },
        ),
        WorkField::Subjects => subjects_field(t, record),
        WorkField::Summary => field(
            "Summary",
            "summary",
            "staff-fields-summary",
            FormInput::LongText {
                value: record.summary.clone().unwrap_or_default(),
            },
        ),
        WorkField::Audience => field(
            "Audience",
            "audience",
            "staff-fields-audience",
            text(record.audience.as_deref()),
        ),
    }
}

fn instance_field(
    t: &I18n,
    record: &NewRecord,
    new_identifier: IdentifierKind,
    part: InstanceField,
) -> (FormField, bool) {
    let instance = &record.instance;
    match part {
        InstanceField::Title => field(
            "Title",
            "title",
            "staff-fields-title",
            FormInput::Text {
                value: instance.title.clone(),
                required: true,
            },
        ),
        InstanceField::Edition => field(
            "Edition",
            "edition",
            "staff-fields-edition",
            text(instance.edition.as_deref()),
        ),
        InstanceField::Place => field(
            "Place",
            "place",
            "staff-fields-place",
            text(instance.place.as_deref()),
        ),
        InstanceField::Publisher => field(
            "Publisher",
            "publisher",
            "staff-fields-publisher",
            text(instance.publisher.as_deref()),
        ),
        InstanceField::Date => field(
            "Date",
            "date",
            "staff-fields-date",
            text(instance.date.as_deref()),
        ),
        InstanceField::Extent => field(
            "Extent",
            "extent",
            "staff-fields-extent",
            text(instance.extent.as_deref()),
        ),
        InstanceField::Series => field(
            "Series",
            "series",
            "staff-fields-series",
            text(instance.series.as_deref()),
        ),
        InstanceField::Identifiers => identifiers_field(t, record, new_identifier),
        InstanceField::Notes => notes_field(t, record),
        InstanceField::MaterialType => field(
            "MaterialType",
            "material_type",
            "staff-fields-material",
            FormInput::Select {
                value: instance
                    .material_type
                    .map(material_name)
                    .unwrap_or_default()
                    .to_owned(),
                options: material_options(t),
            },
        ),
    }
}

fn split(fields: impl Iterator<Item = (FormField, bool)>) -> (Vec<FormField>, Vec<FormField>) {
    let (filled, empty): (Vec<_>, Vec<_>) = fields.partition(|(_, filled)| *filled);
    (
        filled.into_iter().map(|(field, _)| field).collect(),
        empty.into_iter().map(|(field, _)| field).collect(),
    )
}

/// The form of the Work of `record`, read at `version`.
pub(crate) fn work_form<'a>(
    t: &'a I18n,
    record: &NewRecord,
    work: i64,
    version: i64,
) -> RecordForm<'a> {
    let profile = profile_for(
        record
            .instance
            .material_type
            .unwrap_or(record.material_type),
    );
    let (filled, empty) = split(profile.work.iter().map(|part| work_field(t, record, *part)));
    RecordForm {
        t,
        endpoint: format!("/equipe/acervo/obra/{work}"),
        version,
        filled,
        empty,
    }
}

/// The form of the Instance of `record`, read at `version`.
pub(crate) fn instance_form<'a>(
    t: &'a I18n,
    record: &NewRecord,
    instance: i64,
    version: i64,
) -> RecordForm<'a> {
    let profile = profile_for(
        record
            .instance
            .material_type
            .unwrap_or(record.material_type),
    );
    let (filled, empty) = split(
        profile
            .instance
            .iter()
            .map(|part| instance_field(t, record, profile.identifier, *part)),
    );
    RecordForm {
        t,
        endpoint: format!("/equipe/acervo/instancia/{instance}"),
        version,
        filled,
        empty,
    }
}

fn contributions_field(t: &I18n, record: &NewRecord) -> (FormField, bool) {
    let columns = [
        column("heading", "edit-column-name", ColumnKind::Text, false),
        column(
            "kind",
            "edit-column-kind",
            ColumnKind::Select(options(t, AGENTS)),
            false,
        ),
        column("role", "edit-column-role", ColumnKind::Text, true),
        column("primary", "staff-fields-primary", ColumnKind::Check, false),
    ];
    let stored = record
        .contributions
        .iter()
        .map(|contribution| {
            vec![
                contribution.heading.clone(),
                name_of(AGENTS, contribution.kind).to_owned(),
                contribution.role.clone().unwrap_or_default(),
                contribution.primary.to_string(),
            ]
        })
        .collect();
    let blank = vec![
        String::new(),
        "person".to_owned(),
        String::new(),
        "false".to_owned(),
    ];
    field(
        "Contributions",
        "contributions",
        "staff-fields-contributions",
        rows(&columns, stored, blank),
    )
}

fn classification_field(record: &NewRecord) -> (FormField, bool) {
    let columns = [
        column("number", "edit-column-number", ColumnKind::Text, true),
        column("scheme", "edit-column-scheme", ColumnKind::Text, true),
    ];
    let values = vec![
        record.classification.clone().unwrap_or_default(),
        record.classification_scheme.clone().unwrap_or_default(),
    ];
    field(
        "Classification",
        "classification",
        "staff-fields-classification",
        FormInput::Group {
            cells: cells(&columns, values),
        },
    )
}

fn subjects_field(t: &I18n, record: &NewRecord) -> (FormField, bool) {
    let columns = [
        column("heading", "edit-column-heading", ColumnKind::Text, false),
        column(
            "kind",
            "edit-column-kind",
            ColumnKind::Select(options(t, SUBJECTS)),
            false,
        ),
        column("thesaurus", "edit-column-thesaurus", ColumnKind::Text, true),
    ];
    let stored = record
        .subjects
        .iter()
        .map(|subject| {
            vec![
                subject.heading.clone(),
                name_of(SUBJECTS, subject.kind).to_owned(),
                subject.thesaurus.clone().unwrap_or_default(),
            ]
        })
        .collect();
    let blank = vec![String::new(), "topic".to_owned(), String::new()];
    field(
        "Subjects",
        "subjects",
        "staff-fields-subjects",
        rows(&columns, stored, blank),
    )
}

fn identifiers_field(
    t: &I18n,
    record: &NewRecord,
    new_identifier: IdentifierKind,
) -> (FormField, bool) {
    let columns = [
        column(
            "kind",
            "edit-column-kind",
            ColumnKind::Select(options(t, IDENTIFIERS)),
            false,
        ),
        column("value", "edit-column-value", ColumnKind::Text, false),
        column("qualifier", "edit-column-qualifier", ColumnKind::Text, true),
    ];
    let stored = record
        .instance
        .identifiers
        .iter()
        .map(|identifier| {
            vec![
                name_of(IDENTIFIERS, identifier.kind).to_owned(),
                identifier.value.clone(),
                identifier.qualifier.clone().unwrap_or_default(),
            ]
        })
        .collect();
    let blank = vec![
        name_of(IDENTIFIERS, new_identifier).to_owned(),
        String::new(),
        String::new(),
    ];
    field(
        "Identifiers",
        "identifiers",
        "staff-fields-identifiers",
        rows(&columns, stored, blank),
    )
}

fn notes_field(t: &I18n, record: &NewRecord) -> (FormField, bool) {
    let columns = [
        column(
            "kind",
            "edit-column-kind",
            ColumnKind::Select(options(t, NOTES)),
            false,
        ),
        column("text", "edit-column-text", ColumnKind::Text, false),
    ];
    let stored = record
        .instance
        .notes
        .iter()
        .map(|note| vec![name_of(NOTES, note.kind).to_owned(), note.text.clone()])
        .collect();
    let blank = vec!["general".to_owned(), String::new()];
    field(
        "Notes",
        "notes",
        "edit-field-notes",
        rows(&columns, stored, blank),
    )
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde::de::value::{Error, StrDeserializer};

    use super::*;

    fn parses<'de, T: Deserialize<'de> + PartialEq + std::fmt::Debug>(name: &'de str, expected: T) {
        let parsed = T::deserialize(StrDeserializer::<Error>::new(name));
        assert_eq!(parsed.ok(), Some(expected), "{name}");
    }

    /// Every message key the staff catalog's Rust code names exists in the English messages.
    #[test]
    fn every_key_named_in_code_exists() {
        let t = I18n::english().unwrap();
        let sources = [
            include_str!("record_form.rs"),
            include_str!("record_fields.rs"),
            include_str!("../../elzevir-app/src/editing.rs"),
        ];
        let mut missing = Vec::new();
        for source in sources {
            for chunk in source.split('"').skip(1).step_by(2) {
                let is_key = ["staff-", "edit-", "material-"]
                    .iter()
                    .any(|prefix| chunk.starts_with(prefix))
                    && !chunk.contains(' ');
                if is_key && !t.has(chunk) {
                    missing.push(chunk.to_owned());
                }
            }
        }
        assert!(missing.is_empty(), "missing Fluent keys: {missing:?}");
    }

    #[test]
    fn every_name_is_the_serde_name_of_its_variant() {
        for (material, name) in MATERIALS {
            parses(name, *material);
        }
        assert_eq!(MATERIALS.len(), MaterialType::ALL.len());
        for (kind, name, _) in AGENTS {
            parses(name, *kind);
        }
        assert_eq!(AGENTS.len(), AgentKind::ALL.len());
        for (kind, name, _) in SUBJECTS {
            parses(name, *kind);
        }
        assert_eq!(SUBJECTS.len(), SubjectKind::ALL.len());
        for (kind, name, _) in IDENTIFIERS {
            parses(name, *kind);
        }
        assert_eq!(IDENTIFIERS.len(), IdentifierKind::ALL.len());
        for (kind, name, _) in NOTES {
            parses(name, *kind);
        }
        assert_eq!(NOTES.len(), NoteKind::ALL.len());
    }
}
