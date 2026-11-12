// SPDX-License-Identifier: GPL-3.0-or-later
//! Generates character set tables in `atrium-data` (task 1.1).

use std::fmt::Write as _;

use serde_json::Value;

/// `iso5426.rs` from `data/koha-<series>/iso5426.json`.
pub(crate) fn iso5426(json: &Value, from: &str, header: &str) -> Result<String, String> {
    let chars = json
        .get("chars")
        .and_then(Value::as_array)
        .ok_or("iso5426.json has no `chars` array")?;
    let source_file = json
        .pointer("/source/file")
        .and_then(Value::as_str)
        .ok_or("iso5426.json has no source file")?;
    let mut out = header.to_owned();
    writeln!(
        out,
        "//! ISO 5426 to Unicode, generated from {from}.\n//!\n\
         //! Koha: `%chars` in `{source_file}` (used by `C4::Charset::char_decode5426`).\n\n\
         /// `(code, character)`, sorted by code. A code above `0xff` is a diacritic byte\n\
         /// followed by the base byte.\n\
         #[rustfmt::skip]\n\
         pub(crate) static ISO5426: &[(u16, char)] = &["
    )
    .map_err(fmt_error)?;
    let mut previous = None;
    for entry in chars {
        let code = hex(entry, "iso5426")?;
        let code =
            u16::try_from(code).map_err(|_| format!("ISO 5426 code {code:x} is too large"))?;
        if previous.is_some_and(|p| p >= code) {
            return Err(format!("ISO 5426 codes are not sorted at {code:04x}"));
        }
        previous = Some(code);
        let unicode = hex(entry, "unicode")?;
        let character =
            char::from_u32(unicode).ok_or_else(|| format!("U+{unicode:04X} is not a character"))?;
        writeln!(out, "    (0x{code:04x}, {character:?}),").map_err(fmt_error)?;
    }
    out.push_str("];\n");
    Ok(out)
}

fn hex(entry: &Value, key: &str) -> Result<u32, String> {
    let text = entry
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("entry without `{key}`"))?;
    u32::from_str_radix(text, 16).map_err(|e| format!("`{key}` {text} is not hex: {e}"))
}

fn fmt_error(e: std::fmt::Error) -> String {
    format!("formatting generated code failed: {e}")
}
