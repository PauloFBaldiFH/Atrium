// SPDX-License-Identifier: GPL-3.0-or-later
//! Creating records from the staff screen (docs/specs/catalogo/criacao.md, items 5 to 12):
//! the screen sends the same changes as an edit, applied to an empty Work and Instance.

use crate::catalog::{InstanceChange, NewInstance, NewRecord, WorkChange};
use crate::editing::{FieldError, validate_instance_changes, validate_work_changes};

/// Builds a new family (a Work and its first Instance) from the screen's changes. The
/// Instance takes the Work's title when it has none (item 9).
///
/// # Errors
///
/// Every field that cannot be stored as written, and the parts a record cannot exist
/// without: the Work's title and category of material (item 8).
pub fn new_record(
    mut work: Vec<WorkChange>,
    mut instance: Vec<InstanceChange>,
) -> Result<NewRecord, Vec<FieldError>> {
    let mut errors = Vec::new();
    collect(&mut errors, validate_work_changes(&mut work));
    collect(&mut errors, validate_instance_changes(&mut instance));

    let mut work_title = None;
    let mut material_type = None;
    let mut record_parts = RecordParts::default();
    for change in work {
        match change {
            WorkChange::Title(title) => work_title = Some(title),
            WorkChange::MaterialType(material) => material_type = Some(material),
            other => record_parts.apply(other),
        }
    }
    let (instance_title, new_instance) = fold_instance(instance);

    let work_title = work_title.filter(|title| !title.is_empty());
    if work_title.is_none() && !errors.iter().any(|error| error.field == "title") {
        errors.push(required("title", "edit-error-title-empty"));
    }
    if material_type.is_none() {
        errors.push(required("material_type", "edit-error-material-required"));
    }
    let (Some(work_title), Some(material_type), true) =
        (work_title, material_type, errors.is_empty())
    else {
        return Err(errors);
    };
    let instance = NewInstance {
        title: instance_title.unwrap_or_else(|| work_title.clone()),
        ..new_instance
    };
    Ok(NewRecord {
        work_title,
        material_type,
        hub: None,
        relations: Vec::new(),
        classification: record_parts.classification,
        classification_scheme: record_parts.classification_scheme,
        summary: record_parts.summary,
        audience: record_parts.audience,
        languages: record_parts.languages,
        contributions: record_parts.contributions,
        subjects: record_parts.subjects,
        instance,
        items: Vec::new(),
        source: None,
    })
}

/// Builds a new Instance of a stored Work from the screen's changes.
///
/// # Errors
///
/// Every field that cannot be stored as written, and a missing title (item 8).
pub fn new_instance(mut changes: Vec<InstanceChange>) -> Result<NewInstance, Vec<FieldError>> {
    let mut errors = Vec::new();
    collect(&mut errors, validate_instance_changes(&mut changes));
    let (title, instance) = fold_instance(changes);
    match title {
        Some(title) if errors.is_empty() => Ok(NewInstance { title, ..instance }),
        Some(_) => Err(errors),
        None => {
            if !errors.iter().any(|error| error.field == "title") {
                errors.push(required("title", "edit-error-title-empty"));
            }
            Err(errors)
        }
    }
}

/// The Work's parts other than title and category, as a new record holds them.
#[derive(Default)]
struct RecordParts {
    classification: Option<String>,
    classification_scheme: Option<String>,
    summary: Option<String>,
    audience: Option<String>,
    languages: Vec<String>,
    contributions: Vec<crate::catalog::NewContribution>,
    subjects: Vec<crate::catalog::NewSubject>,
}

impl RecordParts {
    fn apply(&mut self, change: WorkChange) {
        match change {
            WorkChange::Classification { number, scheme } => {
                self.classification = number;
                self.classification_scheme = scheme;
            }
            WorkChange::Summary(text) => self.summary = text,
            WorkChange::Audience(text) => self.audience = text,
            WorkChange::Languages(codes) => self.languages = codes,
            WorkChange::Contributions(contributions) => self.contributions = contributions,
            WorkChange::Subjects(subjects) => self.subjects = subjects,
            // Title and category are read by the caller before the rest.
            WorkChange::Title(_) | WorkChange::MaterialType(_) => {}
        }
    }
}

/// The Instance the changes describe, and its title if one was given and is not empty.
/// A later change of the same part wins, as in an edit.
fn fold_instance(changes: Vec<InstanceChange>) -> (Option<String>, NewInstance) {
    let mut title = None;
    let mut instance = NewInstance::default();
    for change in changes {
        match change {
            InstanceChange::Title(text) => title = Some(text),
            InstanceChange::MaterialType(material) => instance.material_type = Some(material),
            InstanceChange::Edition(text) => instance.edition = text,
            InstanceChange::Place(text) => instance.place = text,
            InstanceChange::Publisher(text) => instance.publisher = text,
            InstanceChange::Date(text) => instance.date = text,
            InstanceChange::Extent(text) => instance.extent = text,
            InstanceChange::Series(text) => instance.series = text,
            InstanceChange::Identifiers(identifiers) => instance.identifiers = identifiers,
            InstanceChange::Notes(notes) => instance.notes = notes,
        }
    }
    (title.filter(|title| !title.is_empty()), instance)
}

fn collect(errors: &mut Vec<FieldError>, result: Result<(), Vec<FieldError>>) {
    if let Err(found) = result {
        errors.extend(found);
    }
}

const fn required(field: &'static str, message_key: &'static str) -> FieldError {
    FieldError { field, message_key }
}

#[cfg(test)]
mod tests {
    use elzevir_domain::catalog::{IdentifierKind, MaterialType};

    use super::*;
    use crate::catalog::NewIdentifier;

    #[test]
    fn a_new_family_needs_a_title_and_a_category() {
        let errors = new_record(Vec::new(), Vec::new()).unwrap_err();
        assert_eq!(
            errors,
            [
                required("title", "edit-error-title-empty"),
                required("material_type", "edit-error-material-required"),
            ]
        );
        // A blank title is reported once, by the edit's own validation.
        let errors = new_record(
            vec![
                WorkChange::Title("  ".to_owned()),
                WorkChange::MaterialType(MaterialType::Book),
            ],
            Vec::new(),
        )
        .unwrap_err();
        assert_eq!(errors, [required("title", "edit-error-title-empty")]);
    }

    #[test]
    fn the_instance_takes_the_work_title_when_it_has_none() {
        let record = new_record(
            vec![
                WorkChange::Title(" Dom Casmurro ".to_owned()),
                WorkChange::MaterialType(MaterialType::Book),
                WorkChange::Languages(vec!["por".to_owned()]),
                WorkChange::Summary(Some(String::new())),
            ],
            vec![InstanceChange::Publisher(Some("Garnier".to_owned()))],
        )
        .unwrap();
        assert_eq!(record.work_title, "Dom Casmurro");
        assert_eq!(record.instance.title, "Dom Casmurro");
        assert_eq!(record.instance.publisher.as_deref(), Some("Garnier"));
        assert_eq!(record.languages, ["por"]);
        assert_eq!(record.summary, None);
        assert!(record.source.is_none() && record.items.is_empty());
    }

    #[test]
    fn a_new_instance_needs_a_title_and_valid_identifiers() {
        assert_eq!(
            new_instance(vec![InstanceChange::Edition(Some("2. ed.".to_owned()))]).unwrap_err(),
            [required("title", "edit-error-title-empty")]
        );
        let isbn = |value: &str| NewIdentifier {
            kind: IdentifierKind::Isbn,
            value: value.to_owned(),
            qualifier: None,
        };
        assert_eq!(
            new_instance(vec![
                InstanceChange::Title("Dom Casmurro".to_owned()),
                InstanceChange::Identifiers(vec![isbn("9788535910668")]),
            ])
            .unwrap_err(),
            [required("identifiers", "edit-error-isbn")]
        );
        let instance = new_instance(vec![
            InstanceChange::Title("Dom Casmurro".to_owned()),
            InstanceChange::Identifiers(vec![isbn("978-85-359-1066-7")]),
        ])
        .unwrap();
        assert_eq!(instance.title, "Dom Casmurro");
        assert_eq!(instance.identifiers[0].value, "9788535910667");
    }
}
