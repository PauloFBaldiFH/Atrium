// SPDX-License-Identifier: GPL-3.0-or-later
//! Memory measurement port (task 0.7). The binary decides the allocator and whether
//! allocations are counted; the web layer only shows the report.

use serde::Serialize;

/// What the process knows about its own memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryReport {
    /// Global allocator in use, for example `mimalloc`.
    pub allocator: &'static str,
    /// Allocation counters, present only in builds with the `alloc-count` feature.
    pub counters: Option<AllocationCounters>,
}

/// Cumulative allocator counters since process start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AllocationCounters {
    /// Number of allocations.
    pub allocations: u64,
    /// Number of deallocations.
    pub deallocations: u64,
    /// Number of reallocations.
    pub reallocations: u64,
    /// Bytes currently allocated (allocated minus freed).
    pub live_bytes: i64,
}

/// Port: reads the current memory report.
pub trait MemoryProbe: Send + Sync {
    /// The current report.
    fn report(&self) -> MemoryReport;
}
