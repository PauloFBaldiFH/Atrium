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

/// The ISO code (final escape byte) of the East Asian character set, which gets its own file.
const EACC: &str = "31";

/// Rows per line in the EACC table, which has about 15,700 characters.
const EACC_ROWS_PER_LINE: usize = 8;

/// `marc8.rs` and `marc8_eacc.rs` from `data/loc/marc8.json`.
pub(crate) fn marc8(
    json: &Value,
    from: &str,
    header: &str,
) -> Result<[(String, String); 2], String> {
    let sets = json
        .get("character_sets")
        .and_then(Value::as_array)
        .ok_or("marc8.json has no `character_sets` array")?;
    let sha256 = json
        .pointer("/source/sha256")
        .and_then(Value::as_str)
        .ok_or("marc8.json has no source hash")?;
    let mut main = header.to_owned();
    writeln!(
        main,
        "//! MARC-8 to Unicode, generated from {from}\n\
         //! (Library of Congress `codetables.xml`, SHA-256 {sha256}).\n\n\
         use crate::Marc8Row;\n\n\
         /// Every graphic set by ISO code (final escape byte), each sorted by MARC-8 code.\n\
         pub(crate) static SETS: &[(u8, &[Marc8Row])] = &["
    )
    .map_err(fmt_error)?;
    let mut statics = String::new();
    let mut eacc = header.to_owned();
    let mut ordered = Vec::new();
    for set in sets {
        let iso = set
            .get("iso_code")
            .and_then(Value::as_str)
            .ok_or("character set without `iso_code`")?;
        let code = u8::from_str_radix(iso, 16).map_err(|e| format!("ISO code {iso}: {e}"))?;
        ordered.push((code, iso, set));
    }
    ordered.sort_by_key(|&(code, _, _)| code);
    for pair in ordered.windows(2) {
        if let [(a, iso, _), (b, _, _)] = pair
            && a == b
        {
            return Err(format!("character set {iso} appears twice"));
        }
    }
    for (_, iso, set) in ordered {
        let name = set
            .get("name")
            .and_then(Value::as_str)
            .ok_or("character set without `name`")?;
        let rows = rows(set, iso)?;
        let ident = format!("SET_{iso}");
        if iso == EACC {
            writeln!(main, "    (0x{iso}, super::marc8_eacc::{ident}),").map_err(fmt_error)?;
            writeln!(
                eacc,
                "//! MARC-8 {name}, generated from {from}.\n\n\
                 use crate::Marc8Row;\n\n\
                 /// {name}, sorted by MARC-8 code.\n\
                 #[rustfmt::skip]\n\
                 pub(crate) static {ident}: &[Marc8Row] = &["
            )
            .map_err(fmt_error)?;
            for line in rows.chunks(EACC_ROWS_PER_LINE) {
                writeln!(eacc, "    {}", line.join(" ")).map_err(fmt_error)?;
            }
            eacc.push_str("];\n");
        } else {
            writeln!(main, "    (0x{iso}, {ident}),").map_err(fmt_error)?;
            writeln!(
                statics,
                "\n/// {name}, sorted by MARC-8 code.\n\
                 #[rustfmt::skip]\n\
                 static {ident}: &[Marc8Row] = &["
            )
            .map_err(fmt_error)?;
            for row in rows {
                writeln!(statics, "    {row}").map_err(fmt_error)?;
            }
            statics.push_str("];\n");
        }
    }
    main.push_str("];\n");
    main.push_str(&statics);
    Ok([
        ("marc8.rs".to_owned(), main),
        ("marc8_eacc.rs".to_owned(), eacc),
    ])
}

/// One `(marc, unicode, alternative, combining),` row per code, sorted by MARC-8 code.
fn rows(set: &Value, iso: &str) -> Result<Vec<String>, String> {
    let codes = set
        .get("codes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("character set {iso} has no `codes`"))?;
    let mut parsed = Vec::with_capacity(codes.len());
    for code in codes {
        let marc = hex(code, "marc")?;
        let unicode = optional_char(code, "ucs")?;
        let alternative = optional_char(code, "alt")?;
        let combining = code
            .get("combining")
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("code {marc:x} in set {iso} without `combining`"))?;
        parsed.push((marc, unicode, alternative, combining));
    }
    parsed.sort_by_key(|&(marc, ..)| marc);
    for pair in parsed.windows(2) {
        if let [(a, ..), (b, ..)] = pair
            && a == b
        {
            return Err(format!("code {a:x} appears twice in set {iso}"));
        }
    }
    Ok(parsed
        .into_iter()
        .map(|(marc, unicode, alternative, combining)| {
            format!(
                "({}, {}, {}, {combining}),",
                literal(marc),
                option_char(unicode),
                option_char(alternative)
            )
        })
        .collect())
}

/// A hex literal that satisfies `clippy::unreadable_literal` (EACC codes have three bytes).
fn literal(code: u32) -> String {
    if code > 0xffff {
        format!("0x{:x}_{:04x}", code >> 16, code & 0xffff)
    } else {
        format!("0x{code:x}")
    }
}

/// `Some('…')` or `None`. Anything beyond printable ASCII is written as a `\u{…}` escape, so the
/// generated source stays ASCII (clippy rejects non-NFC literals, and some tables map
/// codes to characters that NFC would rewrite).
fn option_char(character: Option<char>) -> String {
    match character {
        None => "None".to_owned(),
        Some(c) if c.is_ascii_graphic() && c != '\'' && c != '\\' => format!("Some('{c}')"),
        Some(c) => format!("Some('\\u{{{:x}}}')", u32::from(c)),
    }
}

fn optional_char(entry: &Value, key: &str) -> Result<Option<char>, String> {
    if entry.get(key).is_none_or(Value::is_null) {
        return Ok(None);
    }
    let value = hex(entry, key)?;
    char::from_u32(value)
        .map(Some)
        .ok_or_else(|| format!("U+{value:04X} is not a character"))
}
