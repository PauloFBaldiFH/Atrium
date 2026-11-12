// SPDX-License-Identifier: GPL-3.0-or-later
//! Store limits. Defaults follow docs/arquitetura/05-definicoes-de-implementacao.md, §§1-2.

use std::num::NonZeroUsize;

/// Limits for one institution's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreConfig {
    /// Commands that may wait in the writer queue before [`crate::SubmitError::Busy`].
    pub queue_capacity: NonZeroUsize,
    /// Most commands the writer commits in one transaction.
    pub max_batch: NonZeroUsize,
    /// Read-only connections in the pool.
    pub readers: NonZeroUsize,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            queue_capacity: NonZeroUsize::new(1024).unwrap_or(NonZeroUsize::MIN),
            max_batch: NonZeroUsize::new(256).unwrap_or(NonZeroUsize::MIN),
            readers: NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN),
        }
    }
}
