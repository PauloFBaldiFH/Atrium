// SPDX-License-Identifier: GPL-3.0-or-later
//! ISO 5426 to UTF-8, as `C4::Charset::char_decode5426` does it.

use std::borrow::Cow;

/// Decodes an ISO 5426 value (the legacy UNIMARC character set).
///
/// Koha: `C4::Charset::char_decode5426`, quirks included: `0x00`, `0x01` and `0x0F` are
/// removed, unknown bytes become the Unicode character with the same number, and only the
/// first `ESC x`, `0x0E` and `ESC [` are removed afterwards.
#[must_use]
pub fn iso5426_to_utf8(bytes: &[u8]) -> Cow<'_, str> {
    let plain = |b: &u8| (0x02..=0x7f).contains(b) && !matches!(*b, 0x0e | 0x0f | 0x1b);
    if bytes.iter().all(plain)
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return Cow::Borrowed(text);
    }
    let mut text = String::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        match byte {
            0x00 | 0x01 | 0x0f => {}
            0x02..=0x7f => text.push(char::from(byte)),
            0xc0..=0xdf => {
                let pair = bytes
                    .get(index)
                    .and_then(|&next| elzevir_data::iso5426_char(u16::from_be_bytes([byte, next])));
                if let Some(character) = pair {
                    text.push(character);
                    index += 1;
                } else {
                    text.push(single(byte));
                }
            }
            _ => text.push(single(byte)),
        }
    }
    remove_first_escape_pair(&mut text);
    remove_first(&mut text, "\u{0e}");
    remove_first(&mut text, "\u{1b}[");
    Cow::Owned(text)
}

/// The table's character for one byte, or the Unicode character with the same number.
fn single(byte: u8) -> char {
    elzevir_data::iso5426_char(u16::from(byte)).unwrap_or(char::from(byte))
}

/// Perl's `s/\x1b.//`: the first ESC followed by any character except a line feed.
fn remove_first_escape_pair(text: &mut String) {
    let mut chars = text.char_indices().peekable();
    while let Some((at, character)) = chars.next() {
        if character == '\u{1b}'
            && let Some(&(_, next)) = chars.peek()
            && next != '\n'
        {
            text.replace_range(at..at + 1 + next.len_utf8(), "");
            return;
        }
    }
}

fn remove_first(text: &mut String, pattern: &str) {
    if let Some(at) = text.find(pattern) {
        text.replace_range(at..at + pattern.len(), "");
    }
}
