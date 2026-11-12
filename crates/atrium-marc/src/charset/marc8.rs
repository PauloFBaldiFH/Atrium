// SPDX-License-Identifier: GPL-3.0-or-later
//! MARC-8 to UTF-8, as `MARC::Charset::marc8_to_utf8` does it (with `ignore_errors`).

use std::borrow::Cow;

/// Escape byte that starts a character set designation.
const ESCAPE: u8 = 0x1b;
/// Final bytes of the sets this decoder names.
const BASIC_LATIN: u8 = 0x42;
const EXTENDED_LATIN: u8 = 0x45;
const CJK: u8 = 0x31;
const ASCII_DEFAULT: u8 = 0x73;
const GREEK_SYMBOLS: u8 = 0x67;
const SUBSCRIPTS: u8 = 0x62;
const SUPERSCRIPTS: u8 = 0x70;

/// The second halves of the two-part ligature and double tilde. `MARC::Charset` drops
/// them, because the first half already maps to the Unicode double diacritic.
const DROPPED_RIGHT_HALVES: [u8; 2] = [0xec, 0xfb];

/// EACC codes that `MARC::Charset` adds to the Library of Congress tables
/// (`etc/additional-iii-characters.xml`, non-standard codes written by the III ILS).
const III_ADDITIONS: [(u32, char); 6] = [
    (0x21_203d, '\u{2026}'),
    (0x21_2040, '\u{201c}'),
    (0x7f_2014, '\u{2014}'),
    (0x7f_2019, '\u{2019}'),
    (0x7f_2020, '\u{201d}'),
    (0x7f_2122, '\u{2122}'),
];

/// The result of decoding one MARC-8 value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marc8Text<'a> {
    /// The decoded text. Borrowed when the input was plain ASCII.
    pub text: Cow<'a, str>,
    /// Byte offsets that had no mapping and were skipped. Koha treats any of these as a
    /// failed MARC-8 conversion.
    pub unmapped: Vec<usize>,
}

/// Decodes a MARC-8 value.
///
/// Koha: `MARC::Charset::marc8_to_utf8`, called by `C4::Charset::_marc_marc8_to_utf8`
/// with errors ignored. See docs/specs/marc/conversao-de-caracteres.md for its quirks,
/// which are reproduced here (trailing diacritics are lost, `ESC $` always designates G0).
#[must_use]
pub fn marc8_to_utf8(bytes: &[u8]) -> Marc8Text<'_> {
    if bytes
        .iter()
        .all(|&b| (0x20..=0x7e).contains(&b) || is_whitespace(b))
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return Marc8Text {
            text: Cow::Borrowed(text),
            unmapped: Vec::new(),
        };
    }
    let mut decoder = Decoder {
        g0: ASCII_DEFAULT,
        g1: EXTENDED_LATIN,
        text: String::with_capacity(bytes.len()),
        combining: String::new(),
        unmapped: Vec::new(),
    };
    decoder.run(bytes);
    Marc8Text {
        text: Cow::Owned(decoder.text),
        unmapped: decoder.unmapped,
    }
}

struct Decoder {
    g0: u8,
    g1: u8,
    text: String,
    combining: String,
    unmapped: Vec<usize>,
}

impl Decoder {
    fn run(&mut self, bytes: &[u8]) {
        let mut index = 0;
        while let Some(&byte) = bytes.get(index) {
            if is_whitespace(byte) {
                self.text.push(char::from(byte));
                index += 1;
                continue;
            }
            let after_escape = self.escape(bytes, index);
            if after_escape > index {
                index = after_escape;
                continue;
            }
            if let Some(consumed) = self.character(bytes, index) {
                index += consumed;
            } else {
                self.unmapped.push(index);
                index += 1;
            }
        }
        // Diacritics still waiting for a base character are lost, as in MARC::Charset.
    }

    /// Koha: `MARC::Charset::_process_escape`. Returns `index` when there is no escape.
    fn escape(&mut self, bytes: &[u8], index: usize) -> usize {
        if bytes.get(index) != Some(&ESCAPE) {
            return index;
        }
        let Some(&first) = bytes.get(index + 1) else {
            return index;
        };
        if matches!(
            first,
            GREEK_SYMBOLS | SUBSCRIPTS | SUPERSCRIPTS | ASCII_DEFAULT
        ) {
            self.g0 = first;
            return index + 2;
        }
        let Some(&second) = bytes.get(index + 2) else {
            return index;
        };
        match first {
            // `$` is tested before `$,`, `$)` and `$-`, so those multibyte forms always
            // designate G0 with their second byte as the final byte.
            b'(' | b',' | b'$' => self.g0 = second,
            b')' | b'-' => self.g1 = second,
            _ => return index,
        }
        index + 3
    }

    /// Looks the character up in G0, then G1. Returns the bytes consumed, or `None`
    /// when neither set maps it.
    fn character(&mut self, bytes: &[u8], index: usize) -> Option<usize> {
        for set in [self.g0, self.g1] {
            let found = if set == CJK {
                bytes
                    .get(index..index + 3)
                    .and_then(|chunk| cjk(chunk).map(|c| (c, false, 3)))
            } else {
                bytes.get(index).and_then(|&byte| self.single(byte))
            };
            let Some((character, dropped, consumed)) = found else {
                continue;
            };
            if character.combining {
                if !dropped {
                    self.combining.push_str(&character.text);
                }
            } else {
                self.text.push_str(&character.text);
                self.text.push_str(&self.combining);
                self.combining.clear();
            }
            return Some(consumed);
        }
        None
    }

    /// A one-byte character, looked up by the range it falls in.
    fn single(&self, byte: u8) -> Option<(Found, bool, usize)> {
        let (set, code) = match byte {
            0x21..=0x7e => (self.g0, byte),
            0xa1..=0xfe => (self.g1, byte - 0x80),
            0x88..=0x8e => (EXTENDED_LATIN, byte),
            0x1b..=0x1f => (BASIC_LATIN, byte),
            _ => return None,
        };
        let set = if set == ASCII_DEFAULT {
            BASIC_LATIN
        } else {
            set
        };
        let code = table_code(set, code);
        let entry = atrium_data::marc8_char(set, u32::from(code))?;
        let dropped = set == EXTENDED_LATIN && DROPPED_RIGHT_HALVES.contains(&code);
        let text = entry.unicode.or(entry.alternative)?;
        Some((
            Found {
                text: String::from(text),
                combining: entry.combining,
            },
            dropped,
            1,
        ))
    }
}

struct Found {
    text: String,
    combining: bool,
}

/// `MARC::Charset` stores the G1-default sets (Extended Latin, Extended Cyrillic and
/// Extended Arabic) by their G0 code; the Library of Congress tables list them by G1 code.
fn table_code(set: u8, code: u8) -> u8 {
    if matches!(set, 0x45 | 0x51 | 0x34) && (0x21..=0x7e).contains(&code) {
        code + 0x80
    } else {
        code
    }
}

fn cjk(chunk: &[u8]) -> Option<Found> {
    let code = chunk
        .iter()
        .fold(0_u32, |code, &byte| (code << 8) | u32::from(byte));
    let character = III_ADDITIONS
        .iter()
        .find(|(addition, _)| *addition == code)
        .map(|&(_, character)| character)
        .or_else(|| {
            let entry = atrium_data::marc8_char(CJK, code)?;
            entry.unicode.or(entry.alternative)
        })?;
    Some(Found {
        text: String::from(character),
        combining: false,
    })
}

/// Perl's `\s` on a byte string: space, tab, line feed, vertical tab, form feed, carriage
/// return.
fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}
