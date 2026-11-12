// SPDX-License-Identifier: GPL-3.0-or-later
//! MARC field tags.

use std::fmt;

/// A three-character field tag such as `245`. Stored as bytes, never as a `String`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag([u8; 3]);

impl Tag {
    /// The leader is not a field; this tag only names it in errors and mappings.
    pub const LEADER: Self = Self(*b"LDR");

    /// Builds a tag from its three bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    /// The tag's bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 3] {
        &self.0
    }

    /// Control fields (`001` to `009`) carry a single value, without indicators or
    /// subfields. In both MARC21 and UNIMARC they are the tags starting with `00`.
    #[must_use]
    pub const fn is_control(&self) -> bool {
        matches!(self.0, [b'0', b'0', _])
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in &self.0 {
            let shown = if byte.is_ascii_graphic() {
                char::from(byte)
            } else {
                '?'
            };
            write!(f, "{shown}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_tags_start_with_two_zeros() {
        assert!(Tag::new(*b"001").is_control());
        assert!(Tag::new(*b"008").is_control());
        assert!(!Tag::new(*b"010").is_control());
        assert!(!Tag::new(*b"245").is_control());
    }

    #[test]
    fn display_never_emits_control_characters() {
        assert_eq!(Tag::new(*b"245").to_string(), "245");
        assert_eq!(Tag::new([b'2', 0x00, b'5']).to_string(), "2?5");
    }
}
