// SPDX-License-Identifier: GPL-3.0-or-later
//! Generates Koha's default MARC21 search fields in `elzevir-data` from
//! `data/koha-<series>/search_fields.json` (task 1.1). UNIMARC mappings are left out until
//! UNIMARC import exists.

use std::fmt::Write as _;

use serde_json::Value;

/// `search_fields.rs`.
pub(crate) fn generate(json: &Value, from: &str, header: &str) -> Result<String, String> {
    let source_file = json
        .pointer("/source/file")
        .and_then(Value::as_str)
        .ok_or("search_fields.json has no source file")?;
    let mut out = header.to_owned();
    writeln!(
        out,
        "//! Koha's default search fields with their MARC21 mappings, generated from {from}.\n//!\n\
         //! Koha: `{source_file}`\n\n\
         use crate::{{SearchField, SearchFieldKind, SearchMapping}};"
    )
    .map_err(fmt_error)?;
    for (index, constant) in [("biblios", "BIBLIOS"), ("authorities", "AUTHORITIES")] {
        let fields = json
            .pointer(&format!("/indexes/{index}"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("search_fields.json has no `{index}` index"))?;
        writeln!(
            out,
            "\n/// Search fields of the `{index}` index, sorted by name.\n\
             #[rustfmt::skip]\n\
             pub(crate) static {constant}: &[SearchField] = &["
        )
        .map_err(fmt_error)?;
        for field in fields {
            field_row(&mut out, field)?;
        }
        out.push_str("];\n");
    }
    Ok(out)
}

fn field_row(out: &mut String, field: &Value) -> Result<(), String> {
    let name = text(field, "name")?.ok_or("search field without name")?;
    let kind = match text(field, "type")? {
        None => "Default",
        Some("string") => "String",
        Some("number") => "Number",
        Some("date") => "Date",
        Some("boolean") => "Boolean",
        Some("callnumber") => "CallNumber",
        Some("isbn") => "Isbn",
        Some("stdno") => "StandardNumber",
        Some("sum") => "Sum",
        Some("geo_point") => "GeoPoint",
        Some(other) => return Err(format!("search field {name}: unknown type `{other}`")),
    };
    let facet_order = match field.get("facet_order") {
        None | Some(Value::Null) => "None".to_owned(),
        Some(value) => format!(
            "Some({})",
            value
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| format!("search field {name}: bad facet_order"))?
        ),
    };
    writeln!(
        out,
        "    SearchField {{ name: {name:?}, label: {:?}, kind: SearchFieldKind::{kind}, opac: {}, staff_client: {}, mandatory: {}, facet_order: {facet_order}, mappings: &[",
        text(field, "label")?.unwrap_or(name),
        flag(field, "opac")?,
        flag(field, "staff_client")?,
        flag(field, "mandatory")?,
    )
    .map_err(fmt_error)?;
    let mappings = field
        .get("mappings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("search field {name} has no mappings"))?;
    for mapping in mappings {
        if text(mapping, "marc_type")? != Some("marc21") {
            continue;
        }
        writeln!(
            out,
            "        SearchMapping {{ marc_field: {:?}, facet: {}, sort: {}, suggestible: {} }},",
            text(mapping, "marc_field")?
                .ok_or_else(|| format!("{name}: mapping without marc_field"))?,
            flag(mapping, "facet")?,
            flag(mapping, "sort")?,
            flag(mapping, "suggestible")?,
        )
        .map_err(fmt_error)?;
    }
    out.push_str("    ] },\n");
    Ok(())
}

fn text<'a>(json: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match json.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Err(format!("`{key}` is a number ({n}), expected text")),
        Some(other) => Err(format!("`{key}` is not a string: {other}")),
    }
}

fn flag(json: &Value, key: &str) -> Result<bool, String> {
    json.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("`{key}` is not a boolean"))
}

fn fmt_error(e: std::fmt::Error) -> String {
    format!("formatting generated code failed: {e}")
}
