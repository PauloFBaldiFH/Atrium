// SPDX-License-Identifier: GPL-3.0-or-later
//! The blank forms that create a family or an Instance, with what carries over from the
//! previous record (docs/specs/catalogo/criacao.md, items 13 to 19).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use elzevir_app::catalog::{NewInstance, NewRecord};
use elzevir_app::export::stored_record;
use elzevir_app::profiles::profile_for;
use elzevir_app::staff::Permission;
use elzevir_domain::catalog::{InstanceId, MaterialType};
use elzevir_web_templates::{FormField, FormPart, I18n, NewRecordForm};
use serde::Deserialize;

use crate::record_form::{instance_field, material_named, work_field};
use crate::staff::current_session;
use crate::{AppState, render};

/// The parts of a Work that carry over to the next new family (item 17).
const CARRIED_WORK: &[&str] = &["MaterialType", "Languages", "Subjects"];
/// The parts of an Instance that carry over to the next new family (item 17).
const CARRIED_INSTANCE: &[&str] = &["Publisher", "Place", "Date"];

/// Query string of a new family's form.
#[derive(Deserialize)]
pub(crate) struct NewFamilyParams {
    /// The category of material of the session, by its serde name.
    material: Option<String>,
    /// The Instance created before, whose values carry over.
    from: Option<i64>,
}

/// `GET /equipe/acervo/nova?material=&from=`: the form of a new family (items 14 and 17).
pub(crate) async fn new_family(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<NewFamilyParams>,
) -> Response {
    if let Some(refused) = refused(&state, &headers) {
        return refused;
    }
    let previous = match params.from {
        Some(id) => match read(&state, InstanceId(id)).await {
            Ok(record) => record,
            Err(response) => return response,
        },
        None => None,
    };
    let t = &state.inner.i18n;
    let (start, inherited) = if let Some(previous) = &previous {
        (carried_over(previous), true)
    } else {
        let material = params
            .material
            .as_deref()
            .and_then(material_named)
            .unwrap_or(MaterialType::Book);
        (blank(material), false)
    };
    render(&family_form(t, &start, inherited))
}

/// `GET /equipe/acervo/instancia/{id}/nova`: the form of a new Instance of the Work of
/// Instance `{id}`, with its title and category (item 13).
pub(crate) async fn new_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Some(refused) = refused(&state, &headers) {
        return refused;
    }
    let source = match read(&state, InstanceId(id)).await {
        Ok(Some(record)) => record,
        Ok(None) => return (StatusCode::NOT_FOUND, "no such record").into_response(),
        Err(response) => return response,
    };
    let catalog = Arc::clone(&state.inner.opac.catalog);
    let Ok(Ok(Some(view))) =
        tokio::task::spawn_blocking(move || catalog.instance(InstanceId(id))).await
    else {
        return (StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response();
    };
    let mut start = blank(source.material_type);
    start.instance.title.clone_from(&source.instance.title);
    start.instance.material_type = source.instance.material_type;
    let t = &state.inner.i18n;
    let profile = profile_for(start.instance.material_type.unwrap_or(start.material_type));
    let fields = profile
        .instance
        .iter()
        .map(|part| instance_field(t, &start, profile.identifier, *part).0)
        .collect();
    render(&NewRecordForm {
        t,
        endpoint: format!("/equipe/acervo/obra/{}/instancias", view.work.0),
        create: "instance",
        heading_key: "new-instance-heading",
        parts: vec![FormPart {
            name: "instance",
            heading_key: "new-instance-part",
            fields,
        }],
    })
}

/// The answer when the request has no session (to the login) or no cataloguing
/// permission (403); `None` when it may have the form.
fn refused(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let Some(session) = current_session(state, headers) else {
        return Some(Redirect::to("/equipe/login").into_response());
    };
    (!session.identity.can(Permission::Cataloguing)).then(|| StatusCode::FORBIDDEN.into_response())
}

async fn read(state: &AppState, instance: InstanceId) -> Result<Option<NewRecord>, Response> {
    let records = Arc::clone(&state.inner.staff_ports.records);
    match tokio::task::spawn_blocking(move || stored_record(&*records, instance)).await {
        Ok(Ok(stored)) => Ok(stored.map(|stored| stored.record)),
        Ok(Err(_)) | Err(_) => {
            Err((StatusCode::SERVICE_UNAVAILABLE, "catalog unavailable").into_response())
        }
    }
}

fn blank(material_type: MaterialType) -> NewRecord {
    NewRecord {
        work_title: String::new(),
        material_type,
        hub: None,
        relations: Vec::new(),
        classification: None,
        classification_scheme: None,
        summary: None,
        audience: None,
        languages: Vec::new(),
        contributions: Vec::new(),
        subjects: Vec::new(),
        instance: NewInstance::default(),
        items: Vec::new(),
        source: None,
    }
}

/// What the next family starts with after `previous` (item 17).
fn carried_over(previous: &NewRecord) -> NewRecord {
    let mut start = blank(previous.material_type);
    start.languages.clone_from(&previous.languages);
    start.subjects.clone_from(&previous.subjects);
    start.instance.material_type = previous.instance.material_type;
    start
        .instance
        .publisher
        .clone_from(&previous.instance.publisher);
    start.instance.place.clone_from(&previous.instance.place);
    start.instance.date.clone_from(&previous.instance.date);
    start
}

/// Marks a filled field that carried over, until the cataloguer confirms it.
fn mark(carried: &[&str], (mut field, filled): (FormField, bool), inherited: bool) -> FormField {
    field.inherited = inherited && filled && carried.contains(&field.change);
    field
}

fn family_form<'a>(t: &'a I18n, start: &NewRecord, inherited: bool) -> NewRecordForm<'a> {
    let profile = profile_for(start.instance.material_type.unwrap_or(start.material_type));
    let work = profile
        .work
        .iter()
        .map(|part| mark(CARRIED_WORK, work_field(t, start, *part), inherited))
        .collect();
    // The Instance's title is left empty: it takes the Work's (item 9).
    let instance = profile
        .instance
        .iter()
        .map(|part| {
            mark(
                CARRIED_INSTANCE,
                instance_field(t, start, profile.identifier, *part),
                inherited,
            )
        })
        .collect();
    NewRecordForm {
        t,
        endpoint: "/equipe/acervo/nova".to_owned(),
        create: "family",
        heading_key: "new-family-heading",
        parts: vec![
            FormPart {
                name: "work",
                heading_key: "new-work-part",
                fields: work,
            },
            FormPart {
                name: "instance",
                heading_key: "new-instance-part",
                fields: instance,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carried_over_parts_are_the_ones_the_form_names() {
        // Each name is a field change of the profile, or the mark would never show.
        let t = I18n::english().unwrap();
        let start = blank(MaterialType::Book);
        let form = family_form(&t, &start, false);
        let changes = |name: &str| -> Vec<&str> {
            form.parts
                .iter()
                .find(|part| part.name == name)
                .unwrap()
                .fields
                .iter()
                .map(|field| field.change)
                .collect()
        };
        for carried in CARRIED_WORK {
            assert!(changes("work").contains(carried), "{carried}");
        }
        for carried in CARRIED_INSTANCE {
            assert!(changes("instance").contains(carried), "{carried}");
        }
    }
}
