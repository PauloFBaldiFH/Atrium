// SPDX-License-Identifier: GPL-3.0-or-later
//! Cleaning MARC values into model text (spec §1, rule 1).

/// Trims spaces; `None` when nothing is left.
pub(super) fn trimmed(value: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(value);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Trims spaces and the trailing ISBD punctuation; `None` when nothing is left.
pub(super) fn clean(value: &[u8]) -> Option<String> {
    let mut text = trimmed(value)?;
    if text
        .chars()
        .all(|c| c.is_ascii_punctuation() || c.is_whitespace())
    {
        return None;
    }
    loop {
        let before = text.len();
        text.truncate(text.trim_end_matches([' ', '/', ':', ';', '=', ',']).len());
        if text.ends_with('.') && !ends_with_initial(&text) && !text.ends_with("...") {
            text.pop();
            text.truncate(text.trim_end().len());
        }
        if text.len() == before {
            break;
        }
    }
    (!text.is_empty()).then_some(text)
}

/// Whether the text ends in a one-letter abbreviation such as `J.`, which keeps its period.
fn ends_with_initial(text: &str) -> bool {
    let mut letters = text.trim_end_matches('.').chars().rev();
    matches!(
        (letters.next(), letters.next()),
        (Some(letter), None | Some(' ' | '.' | '-' | '(')) if letter.is_alphabetic()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbd_punctuation_and_spaces_go() {
        assert_eq!(clean(b" Dom Casmurro /").as_deref(), Some("Dom Casmurro"));
        assert_eq!(
            clean(b"Rio de Janeiro :").as_deref(),
            Some("Rio de Janeiro")
        );
        assert_eq!(clean(b"Garnier,").as_deref(), Some("Garnier"));
        assert_eq!(clean(b"1899.").as_deref(), Some("1899"));
        assert_eq!(clean(b"romance ; ").as_deref(), Some("romance"));
        assert_eq!(
            clean(b"Rio de Janeiro,RJ:").as_deref(),
            Some("Rio de Janeiro,RJ")
        );
        assert_eq!(clean(b" / ").as_deref(), None);
    }

    #[test]
    fn initials_and_ellipses_keep_their_periods() {
        assert_eq!(clean(b"Machado, J.").as_deref(), Some("Machado, J."));
        assert_eq!(clean(b"Lima, A. B.").as_deref(), Some("Lima, A. B."));
        assert_eq!(
            clean(b"E o vento levou...").as_deref(),
            Some("E o vento levou...")
        );
    }
}
