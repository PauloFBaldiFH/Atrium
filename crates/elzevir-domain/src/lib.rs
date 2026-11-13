// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure library domain: circulation, holds, fines and calendars. No I/O: time, rules and calendars are passed in.

#![forbid(unsafe_code)]

pub mod calendar;
pub mod catalog;
pub mod circulation;
pub mod due_date;
pub mod fine;
