// SPDX-License-Identifier: GPL-3.0-or-later
//! A library's opening calendar: weekly closed days, fixed annual holidays and dated
//! entries (single holidays and exceptions). See `docs/specs/circulacao/vencimento.md`.

use std::collections::{BTreeMap, BTreeSet};

use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

/// How many steps a search for an open day may take before giving up.
///
/// Koha: `Koha::Calendar::OPEN_DAYS_SEARCH_MAX_ITERATIONS`.
pub const OPEN_DAY_SEARCH_LIMIT: u32 = 5000;

/// A dated calendar entry, which wins over the weekly and annual rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatedEntry {
    /// Closed on this date only.
    Closed,
    /// Open on this date even if a weekly or annual rule closes it.
    Open,
}

/// A fixed annual holiday, such as 25 December.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AnnualHoliday {
    /// Month, 1 to 12.
    pub month: i8,
    /// Day of the month, 1 to 31.
    pub day: i8,
}

/// When one library is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Calendar {
    /// One bit per weekday, bit 0 = Sunday (Koha's numbering).
    weekly_closed: u8,
    annual: BTreeSet<AnnualHoliday>,
    dated: BTreeMap<Date, DatedEntry>,
}

/// No open day was found within [`OPEN_DAY_SEARCH_LIMIT`] steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no open day within {OPEN_DAY_SEARCH_LIMIT} steps from {from}")]
pub struct NoOpenDay {
    /// Where the search started.
    pub from: Date,
}

/// How far each step of an open-day search moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Push {
    /// One day at a time.
    Daily,
    /// A week at a time, except from a day that is closed every week.
    Weekly,
}

impl Calendar {
    /// A calendar with every day open.
    #[must_use]
    pub fn always_open() -> Self {
        Self::default()
    }

    /// Closes `weekday` every week.
    #[must_use]
    pub fn closed_every(mut self, weekday: Weekday) -> Self {
        self.weekly_closed |= weekday_bit(weekday);
        self
    }

    /// Closes the same day every year.
    #[must_use]
    pub fn closed_annually(mut self, holiday: AnnualHoliday) -> Self {
        self.annual.insert(holiday);
        self
    }

    /// Sets a dated entry; a later entry for the same date replaces the earlier one.
    #[must_use]
    pub fn on(mut self, date: Date, entry: DatedEntry) -> Self {
        self.dated.insert(date, entry);
        self
    }

    fn closed_weekly(&self, weekday: Weekday) -> bool {
        self.weekly_closed & weekday_bit(weekday) != 0
    }

    /// Whether the library is closed on `date`.
    ///
    /// Koha: `Koha::Calendar::is_holiday`
    #[must_use]
    pub fn is_closed(&self, date: Date) -> bool {
        match self.dated.get(&date) {
            Some(DatedEntry::Closed) => true,
            Some(DatedEntry::Open) => false,
            None => {
                self.closed_weekly(date.weekday())
                    || self.annual.contains(&AnnualHoliday {
                        month: date.month(),
                        day: date.day(),
                    })
            }
        }
    }

    /// The step size from `date` during a search.
    ///
    /// Koha: `Koha::Calendar::get_push_amt`
    fn step_from(&self, date: Date, push: Push) -> i32 {
        match push {
            Push::Weekly if !self.closed_weekly(date.weekday()) => 7,
            Push::Weekly | Push::Daily => 1,
        }
    }

    /// Moves `first_step` days forward, then keeps stepping while the day is closed.
    ///
    /// Koha: `Koha::Calendar::next_open_days`
    pub(crate) fn next_open(
        &self,
        from: Date,
        first_step: i32,
        push: Push,
    ) -> Result<Date, NoOpenDay> {
        let no_open_day = NoOpenDay { from };
        let mut date = add_days(from, first_step).ok_or(no_open_day)?;
        let mut steps = 0;
        while self.is_closed(date) && steps < OPEN_DAY_SEARCH_LIMIT {
            date = add_days(date, self.step_from(date, push)).ok_or(no_open_day)?;
            steps += 1;
        }
        if self.is_closed(date) {
            return Err(no_open_day);
        }
        Ok(date)
    }

    /// The step a search would take from `date`; exposed for the due-date rules.
    pub(crate) fn first_step(&self, date: Date, push: Push) -> i32 {
        self.step_from(date, push)
    }
}

fn weekday_bit(weekday: Weekday) -> u8 {
    1_u8 << weekday.to_sunday_zero_offset().unsigned_abs()
}

/// `date` plus `days`, or `None` past the supported range (year 9999).
pub(crate) fn add_days(date: Date, days: i32) -> Option<Date> {
    date.checked_add(days.days()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn weekends_closed() -> Calendar {
        Calendar::always_open()
            .closed_every(Weekday::Saturday)
            .closed_every(Weekday::Sunday)
    }

    #[test]
    fn dated_entries_win_over_weekly_and_annual_rules() {
        let calendar = weekends_closed()
            .closed_annually(AnnualHoliday { month: 11, day: 20 })
            .on(date(2026, 11, 14), DatedEntry::Open)
            .on(date(2026, 11, 20), DatedEntry::Open)
            .on(date(2026, 11, 23), DatedEntry::Closed);
        assert!(
            !calendar.is_closed(date(2026, 11, 14)),
            "Saturday exception"
        );
        assert!(calendar.is_closed(date(2026, 11, 15)), "Sunday");
        assert!(
            !calendar.is_closed(date(2026, 11, 20)),
            "annual holiday exception"
        );
        assert!(
            calendar.is_closed(date(2027, 11, 20)),
            "annual holiday next year"
        );
        assert!(calendar.is_closed(date(2026, 11, 23)), "single holiday");
        assert!(!calendar.is_closed(date(2026, 11, 24)));
    }

    #[test]
    fn next_open_skips_closed_days() {
        let calendar = weekends_closed();
        assert_eq!(
            calendar.next_open(date(2026, 11, 13), 1, Push::Daily),
            Ok(date(2026, 11, 16))
        );
    }

    #[test]
    fn a_calendar_closed_every_day_has_no_open_day() {
        let calendar = [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ]
        .into_iter()
        .fold(Calendar::always_open(), Calendar::closed_every);
        let from = date(2026, 11, 12);
        assert_eq!(
            calendar.next_open(from, 1, Push::Daily),
            Err(NoOpenDay { from })
        );
    }
}
