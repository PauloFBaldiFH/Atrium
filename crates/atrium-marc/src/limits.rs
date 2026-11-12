// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit input limits (CLAUDE.md: every external input has size limits).

/// Bounds applied to every record read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest record accepted, in bytes. ISO 2709 cannot declare more than 99 999.
    pub max_record_len: usize,
    /// Most fields (directory entries) accepted in one record.
    pub max_fields: usize,
}

impl Limits {
    /// The largest record length ISO 2709 can express (five decimal digits).
    pub const ISO_2709_MAX_RECORD_LEN: usize = 99_999;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_record_len: Self::ISO_2709_MAX_RECORD_LEN,
            max_fields: 5_000,
        }
    }
}
