// SPDX-License-Identifier: GPL-3.0-or-later
//! Global allocator (mimalloc) and, with the `alloc-count` feature, allocation counters.
//!
//! Counting builds wrap the system allocator instead of mimalloc, because the counting
//! wrapper can only be built in a `static` around it on stable Rust. Allocation counts and
//! sizes are what the program requests, so they do not depend on the allocator underneath.

use atrium_app::memory::{MemoryProbe, MemoryReport};

#[cfg(not(feature = "alloc-count"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "alloc-count")]
#[global_allocator]
static GLOBAL: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

/// Reads the global allocator's state.
pub(crate) struct AllocatorProbe;

impl MemoryProbe for AllocatorProbe {
    fn report(&self) -> MemoryReport {
        MemoryReport {
            allocator: if cfg!(feature = "alloc-count") {
                "system (counting)"
            } else {
                "mimalloc"
            },
            counters: counters(),
        }
    }
}

#[cfg(not(feature = "alloc-count"))]
fn counters() -> Option<atrium_app::memory::AllocationCounters> {
    None
}

#[cfg(feature = "alloc-count")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "same signature as the non-counting build, which returns None"
)]
fn counters() -> Option<atrium_app::memory::AllocationCounters> {
    let stats = GLOBAL.stats();
    let to_u64 = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let allocated = i64::try_from(stats.bytes_allocated).unwrap_or(i64::MAX);
    let freed = i64::try_from(stats.bytes_deallocated).unwrap_or(i64::MAX);
    let reallocated = i64::try_from(stats.bytes_reallocated.unsigned_abs())
        .unwrap_or(i64::MAX)
        .saturating_mul(if stats.bytes_reallocated < 0 { -1 } else { 1 });
    Some(atrium_app::memory::AllocationCounters {
        allocations: to_u64(stats.allocations),
        deallocations: to_u64(stats.deallocations),
        reallocations: to_u64(stats.reallocations),
        live_bytes: allocated.saturating_sub(freed).saturating_add(reallocated),
    })
}
