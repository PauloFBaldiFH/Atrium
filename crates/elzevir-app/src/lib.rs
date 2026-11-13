// SPDX-License-Identifier: GPL-3.0-or-later
//! Use cases and ports (repository, index, clock and mail traits). Knows only traits, never concrete storage.

pub mod catalog;
pub mod commands;
pub mod import;
pub mod marc_mapping;
pub mod memory;
pub mod parallelism;
pub mod search;
pub mod staff;
