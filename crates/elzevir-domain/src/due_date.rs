// SPDX-License-Identifier: GPL-3.0-or-later
//! Due date of a loan counted in days. See `docs/specs/circulacao/vencimento.md`.

use jiff::Zoned;
use jiff::civil::{Date, Time};

use crate::calendar::{Calendar, NoOpenDay, Push, add_days};

/// How a loan period is counted against the calendar (`daysmode` rule or `useDaysMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaysMode {
    /// Calendar days; closed days are ignored.
    Days,
    /// Only open days count.
    Calendar,
    /// Calendar days; a due date on a closed day moves to the next open day.
    Datedue,
    /// Like `Datedue`, but a period in whole weeks moves a week at a time.
    Dayweek,
}

impl DaysMode {
    /// The mode for Koha's value, or `None` if Koha does not define it.
    #[must_use]
    pub fn from_koha(value: &str) -> Option<Self> {
        match value {
            "Days" => Some(Self::Days),
            "Calendar" => Some(Self::Calendar),
            "Datedue" => Some(Self::Datedue),
            "Dayweek" => Some(Self::Dayweek),
            _ => None,
        }
    }
}

/// A loan period counted in days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanDays(pub u16);

/// The due date could not be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DueDateError {
    /// The calendar has no open day to land on.
    #[error(transparent)]
    NoOpenDay(#[from] NoOpenDay),
    /// The due date falls outside the supported range of dates.
    #[error("due date out of range")]
    OutOfRange,
}

/// When a loan of `length` that starts at `start` is due, in `start`'s time zone.
///
/// The due time is always 23:59, as in Koha for periods in days.
///
/// # Errors
///
/// [`DueDateError::NoOpenDay`] when the calendar has no open day to land on.
///
/// Koha: `C4::Circulation::CalcDateDue` (days only; hours, hard due dates and
/// `ReturnBeforeExpiry` are out of scope in v0).
pub fn due_date(
    start: &Zoned,
    length: LoanDays,
    mode: DaysMode,
    calendar: &Calendar,
) -> Result<Zoned, DueDateError> {
    let day = due_day(start.date(), length, mode, calendar)?;
    day.to_datetime(Time::constant(23, 59, 0, 0))
        .to_zoned(start.time_zone().clone())
        .map_err(|_| DueDateError::OutOfRange)
}

/// The calendar day a loan is due.
///
/// Koha: `Koha::Calendar::addDays`
fn due_day(
    start: Date,
    LoanDays(days): LoanDays,
    mode: DaysMode,
    calendar: &Calendar,
) -> Result<Date, DueDateError> {
    match mode {
        DaysMode::Days => add_days(start, i32::from(days)).ok_or(DueDateError::OutOfRange),
        DaysMode::Calendar => {
            let mut day = start;
            for _ in 0..days {
                day = calendar.next_open(day, 1, Push::Daily)?;
            }
            Ok(day)
        }
        DaysMode::Datedue | DaysMode::Dayweek => {
            let day = add_days(start, i32::from(days)).ok_or(DueDateError::OutOfRange)?;
            if !calendar.is_closed(day) {
                return Ok(day);
            }
            let push = if mode == DaysMode::Dayweek && days % 7 == 0 {
                Push::Weekly
            } else {
                Push::Daily
            };
            let first_step = calendar.first_step(day, push);
            Ok(calendar.next_open(day, first_step, push)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{AnnualHoliday, DatedEntry};
    use jiff::civil::{Weekday, date};

    /// Thursday 12 November 2026, 10:30 in Palotina.
    fn thursday() -> Zoned {
        date(2026, 11, 12)
            .at(10, 30, 0, 0)
            .in_tz("America/Sao_Paulo")
            .unwrap()
    }

    fn library() -> Calendar {
        Calendar::always_open()
            .closed_every(Weekday::Saturday)
            .closed_every(Weekday::Sunday)
            .closed_annually(AnnualHoliday { month: 11, day: 20 })
    }

    fn due(days: u16, mode: DaysMode, calendar: &Calendar) -> String {
        due_date(&thursday(), LoanDays(days), mode, calendar)
            .unwrap()
            .strftime("%F %H:%M %:z")
            .to_string()
    }

    #[test]
    fn days_mode_ignores_the_calendar() {
        assert_eq!(
            due(2, DaysMode::Days, &library()),
            "2026-11-14 23:59 -03:00"
        );
    }

    #[test]
    fn zero_days_is_due_today() {
        assert_eq!(
            due(0, DaysMode::Datedue, &library()),
            "2026-11-12 23:59 -03:00"
        );
    }

    #[test]
    fn datedue_moves_a_closed_due_day_to_the_next_open_day() {
        assert_eq!(
            due(14, DaysMode::Datedue, &library()),
            "2026-11-26 23:59 -03:00"
        );
        assert_eq!(
            due(2, DaysMode::Datedue, &library()),
            "2026-11-16 23:59 -03:00"
        );
        assert_eq!(
            due(8, DaysMode::Datedue, &library()),
            "2026-11-23 23:59 -03:00"
        );
    }

    #[test]
    fn calendar_counts_only_open_days() {
        assert_eq!(
            due(2, DaysMode::Calendar, &library()),
            "2026-11-16 23:59 -03:00"
        );
        assert_eq!(
            due(6, DaysMode::Calendar, &library()),
            "2026-11-23 23:59 -03:00"
        );
    }

    #[test]
    fn dayweek_keeps_the_weekday_for_whole_weeks() {
        let calendar = library().on(date(2026, 11, 26), DatedEntry::Closed);
        assert_eq!(
            due(14, DaysMode::Dayweek, &calendar),
            "2026-12-03 23:59 -03:00"
        );
        assert_eq!(
            due(14, DaysMode::Datedue, &calendar),
            "2026-11-27 23:59 -03:00"
        );
    }

    #[test]
    fn dayweek_moves_one_day_from_a_weekly_closed_day() {
        let start = date(2026, 11, 14)
            .at(10, 0, 0, 0)
            .in_tz("America/Sao_Paulo")
            .unwrap();
        let due = due_date(&start, LoanDays(14), DaysMode::Dayweek, &library()).unwrap();
        assert_eq!(due.date(), date(2026, 11, 30));
    }

    #[test]
    fn an_exception_opens_a_weekly_closed_day() {
        let calendar = library().on(date(2026, 11, 14), DatedEntry::Open);
        assert_eq!(
            due(2, DaysMode::Datedue, &calendar),
            "2026-11-14 23:59 -03:00"
        );
    }

    #[test]
    fn koha_daysmode_names() {
        assert_eq!(DaysMode::from_koha("Datedue"), Some(DaysMode::Datedue));
        assert_eq!(DaysMode::from_koha("datedue"), None);
    }
}
