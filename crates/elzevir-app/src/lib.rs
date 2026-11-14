// SPDX-License-Identifier: GPL-3.0-or-later
//! Use cases and ports (repository, index, clock and mail traits). Knows only traits, never concrete storage.

pub mod catalog;
pub mod commands;
pub mod creating;
pub mod editing;
pub mod export;
pub mod import;
pub mod listing;
pub mod marc_mapping;
pub mod matching;
pub mod memory;
pub mod opac;
pub mod parallelism;
pub mod profiles;
pub mod search;
pub mod staff;
pub mod suggestions;
