// SPDX-License-Identifier: GPL-3.0-or-later
//! Generates the MARC21 default framework tables in `elzevir-data` from
//! `data/koha-<series>/marc21_framework.json` (task 1.1).
//!
//! One file per hundred of tags keeps each under the 3,000-line limit. Rows are written one
//! per line and kept out of rustfmt, which would spread every row over a dozen lines.

use std::fmt::Write as _;

use serde_json::Value;

/// A generated file name and its source.
pub(crate) struct File {
    pub(crate) name: String,
    pub(crate) source: String,
}

/// The ten `marc21_framework_<n>xx.rs` files.
pub(crate) fn files(
    json: &Value,
    from: &str,
    header: &dyn Fn(&str) -> String,
) -> Result<Vec<File>, String> {
    let fields = json
        .get("fields")
        .and_then(Value::as_array)
        .ok_or("marc21_framework.json has no `fields` array")?;
    let source_file = json
        .pointer("/source/file")
        .and_then(Value::as_str)
        .ok_or("marc21_framework.json has no source file")?;
    let mut files = Vec::new();
    for hundred in '0'..='9' {
        let mut out = header(from);
        writeln!(
            out,
            "//! MARC21 default framework, tags {hundred}xx.\n//!\n//! Koha: `{source_file}`\n\n\
             use crate::{{MarcFieldDefinition, MarcSubfieldDefinition}};\n\n\
             /// Tags {hundred}00 to {hundred}99, sorted by tag.\n\
             #[rustfmt::skip]\n\
             pub(crate) static FIELDS: &[MarcFieldDefinition] = &["
        )
        .map_err(fmt_error)?;
        for field in fields {
            let tag = text(field, "tag")?.ok_or("field without tag")?;
            if !tag.starts_with(hundred) {
                continue;
            }
            field_row(&mut out, tag, field)?;
        }
        out.push_str("];\n");
        files.push(File {
            name: format!("marc21_framework_{hundred}xx.rs"),
            source: out,
        });
    }
    Ok(files)
}

fn field_row(out: &mut String, tag: &str, field: &Value) -> Result<(), String> {
    writeln!(
        out,
        "    MarcFieldDefinition {{ tag: {tag:?}, label: {:?}, label_opac: {:?}, repeatable: {}, mandatory: {}, authorised_value: {}, subfields: &[",
        text(field, "label")?.unwrap_or_default(),
        text(field, "label_opac")?.unwrap_or_default(),
        flag(field, "repeatable")?,
        flag(field, "mandatory")?,
        option_str(text(field, "authorised_value")?),
    )
    .map_err(fmt_error)?;
    let subfields = field
        .get("subfields")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("tag {tag} has no subfields array"))?;
    for subfield in subfields {
        let code =
            text(subfield, "code")?.ok_or_else(|| format!("tag {tag}: subfield without code"))?;
        let mut chars = code.chars();
        let (Some(code), None) = (chars.next(), chars.next()) else {
            return Err(format!(
                "tag {tag}: subfield code `{code}` is not one character"
            ));
        };
        writeln!(
            out,
            "        MarcSubfieldDefinition {{ code: {code:?}, label: {:?}, label_opac: {:?}, repeatable: {}, mandatory: {}, kohafield: {}, tab: {}, hidden: {}, authorised_value: {}, authtypecode: {}, value_builder: {}, is_url: {}, max_length: {} }},",
            text(subfield, "label")?.unwrap_or_default(),
            text(subfield, "label_opac")?.unwrap_or_default(),
            flag(subfield, "repeatable")?,
            flag(subfield, "mandatory")?,
            option_str(text(subfield, "kohafield")?),
            option_i8(subfield, "tab")?,
            option_i8(subfield, "hidden")?,
            option_str(text(subfield, "authorised_value")?),
            option_str(text(subfield, "authtypecode")?),
            option_str(text(subfield, "value_builder")?),
            flag(subfield, "isurl")?,
            subfield
                .get("maxlength")
                .and_then(Value::as_u64)
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| format!("tag {tag}${code}: bad maxlength"))?,
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
        Some(other) => Err(format!("`{key}` is not a string: {other}")),
    }
}

fn flag(json: &Value, key: &str) -> Result<bool, String> {
    json.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("`{key}` is not a boolean"))
}

fn option_str(value: Option<&str>) -> String {
    value.map_or_else(|| "None".to_owned(), |s| format!("Some({s:?})"))
}

fn option_i8(json: &Value, key: &str) -> Result<String, String> {
    match json.get(key) {
        None | Some(Value::Null) => Ok("None".to_owned()),
        Some(value) => value
            .as_i64()
            .and_then(|n| i8::try_from(n).ok())
            .map(|n| format!("Some({n})"))
            .ok_or_else(|| format!("`{key}` is not a small integer: {value}")),
    }
}

fn fmt_error(e: std::fmt::Error) -> String {
    format!("formatting generated code failed: {e}")
}
