// SPDX-License-Identifier: GPL-3.0-or-later
//! Every filled field of a focused Work or Instance block
//! (docs/specs/catalogo/listagem.md, items 42 to 45).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use elzevir_app::catalog::{CodeLists, ItemData, NewRecord};
use elzevir_app::export::stored_record;
use elzevir_app::staff::Permission;
use elzevir_domain::catalog::{
    AgentKind, IdentifierKind, InstanceId, NoteKind, SubjectKind, WorkRelationKind,
};
use elzevir_web_templates::{FieldEntry, FieldGroup, FieldValue, I18n, ItemFields, RecordFields};
use serde::Deserialize;

use crate::catalog::signal;
use crate::opac::material_key;
use crate::record_form::{instance_form, work_form};
use crate::staff::current_session;
use crate::{AppState, render};

/// Query string of the fields of a block.
#[derive(Deserialize)]
pub(crate) struct FieldsParams {
    level: Option<String>,
}

/// Which block the fields are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Work,
    Instance,
}

/// `GET /equipe/acervo/instancia/{id}/campos?level=work|instance`: the fields of the
/// Instance, or of its Work, as an HTML fragment for the focused block.
pub(crate) async fn record_fields(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Query(params): Query<FieldsParams>,
) -> Response {
    let Some(session) = current_session(&state, &headers) else {
        return Redirect::to("/equipe/login").into_response();
    };
    let level = match params.level.as_deref() {
        None | Some("instance") => Level::Instance,
        Some("work") => Level::Work,
        Some(_) => return (StatusCode::BAD_REQUEST, "unknown level").into_response(),
    };
    let records = Arc::clone(&state.inner.staff_ports.records);
    let catalog = Arc::clone(&state.inner.opac.catalog);
    let read = tokio::task::spawn_blocking(move || {
        let stored = stored_record(&*records, InstanceId(id)).map_err(drop)?;
        let codes = catalog.codes().map_err(drop)?;
        Ok::<_, ()>(stored.map(|stored| (stored.record, codes)))
    })
    .await;
    let (record, codes) = match read {
        Ok(Ok(Some(read))) => read,
        Ok(Ok(None)) => return (StatusCode::NOT_FOUND, "no such record").into_response(),
        Ok(Err(())) | Err(_) => {
            return (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response();
        }
    };
    let t = &state.inner.i18n;
    if session.identity.can(Permission::Cataloguing) {
        return form(&state, t, level, InstanceId(id), &record).await;
    }
    let page = match level {
        Level::Work => RecordFields {
            t,
            groups: vec![work_group(t, &record)],
            items: Vec::new(),
        },
        Level::Instance => RecordFields {
            t,
            groups: vec![instance_group(t, &record)],
            items: record
                .items
                .iter()
                .map(|item| item_fields(item, &codes))
                .collect(),
        },
    };
    render(&page)
}

fn plain(text: &str) -> FieldValue {
    FieldValue {
        text: text.to_owned(),
        qualifier: None,
    }
}

/// Adds the field when it has a value: the block shows only what is filled.
fn push(fields: &mut Vec<FieldEntry>, label_key: &'static str, values: Vec<FieldValue>) {
    if !values.is_empty() {
        fields.push(FieldEntry { label_key, values });
    }
}

fn joined(parts: impl IntoIterator<Item = String>) -> Option<String> {
    let parts: Vec<String> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn work_group(t: &I18n, record: &NewRecord) -> FieldGroup {
    let mut fields = Vec::new();
    push(
        &mut fields,
        "staff-fields-preferred-title",
        vec![plain(&record.work_title)],
    );
    push(
        &mut fields,
        "staff-fields-material",
        vec![plain(&t.get(material_key(record.material_type)))],
    );
    push(
        &mut fields,
        "staff-fields-contributions",
        record
            .contributions
            .iter()
            .map(|contribution| FieldValue {
                text: contribution.heading.clone(),
                qualifier: joined(
                    [
                        Some(t.get(agent_key(contribution.kind))),
                        contribution.role.clone(),
                        contribution.primary.then(|| t.get("staff-fields-primary")),
                    ]
                    .into_iter()
                    .flatten(),
                ),
            })
            .collect(),
    );
    push(
        &mut fields,
        "staff-fields-classification",
        record
            .classification
            .iter()
            .map(|number| FieldValue {
                text: number.clone(),
                qualifier: record.classification_scheme.clone(),
            })
            .collect(),
    );
    push(
        &mut fields,
        "staff-fields-subjects",
        record
            .subjects
            .iter()
            .map(|subject| FieldValue {
                text: subject.heading.clone(),
                qualifier: joined(
                    [
                        Some(t.get(subject_key(subject.kind))),
                        subject.thesaurus.clone(),
                    ]
                    .into_iter()
                    .flatten(),
                ),
            })
            .collect(),
    );
    push(
        &mut fields,
        "staff-fields-languages",
        record.languages.iter().map(|code| plain(code)).collect(),
    );
    push(
        &mut fields,
        "staff-fields-summary",
        record.summary.iter().map(|text| plain(text)).collect(),
    );
    push(
        &mut fields,
        "staff-fields-audience",
        record.audience.iter().map(|text| plain(text)).collect(),
    );
    push(
        &mut fields,
        "staff-fields-relations",
        record
            .relations
            .iter()
            .map(|relation| FieldValue {
                text: t.get_numbers(
                    "staff-relation-work",
                    &[("id", u64::try_from(relation.work.0).unwrap_or_default())],
                ),
                qualifier: Some(t.get(relation_key(relation.kind))),
            })
            .collect(),
    );
    FieldGroup {
        heading_key: "staff-fields-work",
        fields,
    }
}

fn instance_group(t: &I18n, record: &NewRecord) -> FieldGroup {
    let instance = &record.instance;
    let optional = |value: &Option<String>| value.iter().map(|text| plain(text)).collect();
    let mut fields = Vec::new();
    push(
        &mut fields,
        "staff-fields-title",
        vec![plain(&instance.title)],
    );
    push(
        &mut fields,
        "staff-fields-edition",
        optional(&instance.edition),
    );
    push(&mut fields, "staff-fields-place", optional(&instance.place));
    push(
        &mut fields,
        "staff-fields-publisher",
        optional(&instance.publisher),
    );
    push(&mut fields, "staff-fields-date", optional(&instance.date));
    push(
        &mut fields,
        "staff-fields-extent",
        optional(&instance.extent),
    );
    push(
        &mut fields,
        "staff-fields-series",
        optional(&instance.series),
    );
    if let Some(material) = instance.material_type {
        push(
            &mut fields,
            "staff-fields-material",
            vec![plain(&t.get(material_key(material)))],
        );
    }
    push(
        &mut fields,
        "staff-fields-identifiers",
        instance
            .identifiers
            .iter()
            .map(|identifier| FieldValue {
                text: identifier.value.clone(),
                qualifier: joined(
                    [
                        Some(t.get(identifier_key(identifier.kind))),
                        identifier.qualifier.clone(),
                    ]
                    .into_iter()
                    .flatten(),
                ),
            })
            .collect(),
    );
    for kind in NoteKind::ALL {
        push(
            &mut fields,
            note_key(*kind),
            instance
                .notes
                .iter()
                .filter(|note| note.kind == *kind)
                .map(|note| plain(&note.text))
                .collect(),
        );
    }
    FieldGroup {
        heading_key: "staff-fields-instance",
        fields,
    }
}

/// One Item, with names in place of codes (exemplares.md, item 29).
fn item_fields(item: &ItemData, codes: &CodeLists) -> ItemFields {
    let (signal_class, signal_key) = signal(item.status.signal());
    ItemFields {
        barcode: item.barcode.clone(),
        inventory_number: item.inventory_number.clone(),
        home_branch: codes.branch(&item.home_branch).to_owned(),
        holding_branch: (item.holding_branch != item.home_branch)
            .then(|| codes.branch(&item.holding_branch).to_owned()),
        item_type: codes.item_type(&item.item_type).to_owned(),
        call_number: item.call_number.clone(),
        location: item
            .location
            .as_deref()
            .map(|code| codes.location(code).to_owned()),
        collection: item
            .collection
            .as_deref()
            .map(|code| codes.collection(code).to_owned()),
        copy_number: item.copy_number.clone(),
        acquired_on: item.acquired_on.clone(),
        signal_class,
        signal_key,
    }
}

const fn agent_key(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Person => "staff-agent-person",
        AgentKind::Organization => "staff-agent-organization",
        AgentKind::Meeting => "staff-agent-meeting",
        AgentKind::Family => "staff-agent-family",
    }
}

const fn subject_key(kind: SubjectKind) -> &'static str {
    match kind {
        SubjectKind::Topic => "staff-subject-topic",
        SubjectKind::Place => "staff-subject-place",
        SubjectKind::Agent => "staff-subject-agent",
        SubjectKind::Title => "staff-subject-title",
        SubjectKind::GenreForm => "staff-subject-genre-form",
        SubjectKind::Temporal => "staff-subject-temporal",
    }
}

const fn identifier_key(kind: IdentifierKind) -> &'static str {
    match kind {
        IdentifierKind::Isbn => "staff-identifier-isbn",
        IdentifierKind::Issn => "staff-identifier-issn",
        IdentifierKind::Lccn => "staff-identifier-lccn",
        IdentifierKind::Ean => "staff-identifier-ean",
        IdentifierKind::SystemControlNumber => "staff-identifier-system-control-number",
        IdentifierKind::Doi => "staff-identifier-doi",
        IdentifierKind::Ismn => "staff-identifier-ismn",
    }
}

const fn note_key(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::General => "staff-fields-note-general",
        NoteKind::Contents => "staff-fields-note-contents",
        NoteKind::Local => "staff-fields-note-local",
    }
}

const fn relation_key(kind: WorkRelationKind) -> &'static str {
    match kind {
        WorkRelationKind::TranslationOf => "staff-relation-translation-of",
        WorkRelationKind::AdaptationOf => "staff-relation-adaptation-of",
        WorkRelationKind::PartOf => "staff-relation-part-of",
        WorkRelationKind::SupplementTo => "staff-relation-supplement-to",
        WorkRelationKind::ContinuationOf => "staff-relation-continuation-of",
    }
}

/// The form for a cataloguer (docs/specs/catalogo/edicao.md, item 25), with the versions
/// the edit must cite.
async fn form(
    state: &AppState,
    t: &I18n,
    level: Level,
    instance: InstanceId,
    record: &NewRecord,
) -> Response {
    let catalog = Arc::clone(&state.inner.opac.catalog);
    let Ok(Ok(Some(view))) = tokio::task::spawn_blocking(move || catalog.instance(instance)).await
    else {
        return (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response();
    };
    match level {
        Level::Work => render(&work_form(t, record, view.work.0, view.work_version)),
        Level::Instance => render(&instance_form(t, record, instance.0, view.version)),
    }
}
