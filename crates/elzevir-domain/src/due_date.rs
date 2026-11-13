// SPDX-License-Identifier: GPL-3.0-or-later
//! Due date of a loan or a renewal. See `docs/specs/circulacao/vencimento.md`.

use jiff::civil::{Date, Time};
use jiff::{ToSpan, Zoned};

use crate::calendar::{Calendar, NoOpenDay, Push};

/// How a loan period is counted against the calendar (`daysmode` rule or `useDaysMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaysMode {
    /// Calendar days; closed days are ignored.
    Days,
    /// Only open days count.
    Calendar,
    /// Calendar days; a due date on a closed day moves to the next open day.
    Datedue,
    /// Like `Datedue`, but searches for an open day move a week at a time.
    Dayweek,
    /// A value Koha does not define (`noDays` in its own tests). Koha adds the period like
    /// `Days`, but still applies the calendar where it only checks for "not `Days`".
    Other,
}

impl DaysMode {
    /// The mode for Koha's value; values Koha does not define are [`DaysMode::Other`].
    #[must_use]
    pub fn from_koha(value: &str) -> Self {
        match value {
            "Days" => Self::Days,
            "Calendar" => Self::Calendar,
            "Datedue" => Self::Datedue,
            "Dayweek" => Self::Dayweek,
            _ => Self::Other,
        }
    }

    /// How searches for an open day step in this mode.
    ///
    /// Koha: `Koha::Calendar::get_push_amt`
    const fn push(self) -> Push {
        match self {
            Self::Dayweek => Push::Weekly,
            Self::Days | Self::Calendar | Self::Datedue | Self::Other => Push::Daily,
        }
    }
}

/// The unit of a loan period (`lengthunit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    /// Whole days, due at 23:59.
    Days,
    /// Hours from the checkout.
    Hours,
}

/// What a hard due date does to the computed one (`hardduedatecompare`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardDueDateCompare {
    /// A ceiling: a later due date becomes the hard one (Koha `-1`).
    Before,
    /// Every loan is due on the hard date (Koha `0`).
    Exactly,
    /// A floor: an earlier due date becomes the hard one (Koha `1`).
    After,
}

/// A hard due date, at 23:59 of its day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardDueDate {
    /// The day.
    pub date: Date,
    /// How it limits the computed due date.
    pub compare: HardDueDateCompare,
}

/// The loan-period part of a circulation rule, already resolved for the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanRule {
    /// `issuelength`.
    pub issue_length: u32,
    /// `renewalperiod`; `None` when unset or empty, so renewals use `issue_length`.
    pub renewal_period: Option<u32>,
    /// `lengthunit`.
    pub unit: LengthUnit,
    /// The effective `daysmode` (the rule's, else `useDaysMode`).
    pub days_mode: DaysMode,
    /// `hardduedate` with `hardduedatecompare`.
    pub hard_due_date: Option<HardDueDate>,
}

/// One weekday of a library's opening hours (`library_hours`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpeningHours {
    /// Opening time, if set.
    pub open: Option<Time>,
    /// Closing time, if set.
    pub close: Option<Time>,
}

/// A library's opening hours by weekday, Sunday first; `None` when the day has no row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LibraryHours(pub [Option<OpeningHours>; 7]);

impl LibraryHours {
    fn on(&self, date: Date) -> Option<OpeningHours> {
        let index = usize::from(date.weekday().to_sunday_zero_offset().unsigned_abs());
        self.0.get(index).copied().flatten()
    }
}

/// How hourly loans meet the library's closing time (`ConsiderLibraryHoursInCirculation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsiderLibraryHours {
    /// Opening hours do not matter.
    Ignore,
    /// A loan that would end after closing ends at closing time.
    Close,
    /// A loan that would end after closing ends at the next opening.
    Open,
}

/// Everything `calc_date_due` reads besides the start.
#[derive(Debug, Clone, Copy)]
pub struct DueDateContext<'a> {
    /// The circulation rule.
    pub rule: &'a LoanRule,
    /// The library's calendar.
    pub calendar: &'a Calendar,
    /// The library's opening hours.
    pub hours: &'a LibraryHours,
    /// `ConsiderLibraryHoursInCirculation`.
    pub consider_hours: ConsiderLibraryHours,
    /// `ReturnBeforeExpiry`.
    pub return_before_expiry: bool,
    /// The patron's `dateexpiry`.
    pub patron_expiry: Option<Date>,
}

/// Fixed hour an hourly loan moved off a closed day is due at.
///
/// Koha: `$return_by_hour` in `Koha::Calendar::addDuration` ("should be set as a branch
/// parameter").
const RETURN_BY_HOUR: i8 = 10;

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

/// When a loan (or, with `renewal`, a renewal) that starts at `start` is due, in `start`'s
/// time zone.
///
/// # Errors
///
/// [`DueDateError::NoOpenDay`] when the calendar has no open day to land on.
///
/// Koha: `C4::Circulation::CalcDateDue`
pub fn calc_date_due(
    start: &Zoned,
    renewal: bool,
    context: &DueDateContext<'_>,
) -> Result<Zoned, DueDateError> {
    let rule = context.rule;
    let length = match (renewal, rule.renewal_period) {
        (true, Some(period)) => period,
        _ => rule.issue_length,
    };
    let mut due = match rule.unit {
        LengthUnit::Days => {
            let day = due_day(start.date(), length, rule.days_mode, context.calendar)?;
            at(&with_date(start, day)?, 23, 59)?
        }
        LengthUnit::Hours => hourly_due(start, length, context)?,
    };
    if let Some(hard) = rule.hard_due_date {
        due = apply_hard_due_date(due, hard)?;
    }
    if context.return_before_expiry {
        due = before_expiry(due, context)?;
    }
    Ok(due)
}

/// The calendar day a loan in days is due.
///
/// Koha: `Koha::Calendar::addDays`
fn due_day(
    start: Date,
    days: u32,
    mode: DaysMode,
    calendar: &Calendar,
) -> Result<Date, DueDateError> {
    let days = i32::try_from(days).map_err(|_| DueDateError::OutOfRange)?;
    let push = mode.push();
    match mode {
        DaysMode::Calendar => {
            let mut day = start;
            for _ in 0..days {
                day = calendar.next_open(day, 1, push)?;
            }
            Ok(day)
        }
        DaysMode::Days | DaysMode::Other => plus_days(start, days),
        DaysMode::Datedue | DaysMode::Dayweek => {
            let day = plus_days(start, days)?;
            if !calendar.is_closed(day) {
                return Ok(day);
            }
            let first_step = if days % 7 == 0 {
                calendar.step_from(day, push)
            } else {
                1
            };
            Ok(calendar.next_open(day, first_step, push)?)
        }
    }
}

/// When an hourly loan is due.
///
/// Koha: `C4::Circulation::CalcDateDue` (the `hours` branches) and
/// `Koha::Calendar::addHours`.
fn hourly_due(
    start: &Zoned,
    hours: u32,
    context: &DueDateContext<'_>,
) -> Result<Zoned, DueDateError> {
    let mode = context.rule.days_mode;
    let today = context.hours.on(start.date());
    let tomorrow = context.hours.on(start
        .date()
        .tomorrow()
        .map_err(|_| DueDateError::OutOfRange)?);
    let span = i64::from(hours).hours();
    // Koha ignores opening hours when either day has no row, or a time is missing.
    let limits = match (context.consider_hours, today, tomorrow) {
        (ConsiderLibraryHours::Ignore, _, _) | (_, None, _) | (_, _, None) => None,
        (consider, Some(today), Some(tomorrow)) => match (today.close, tomorrow.open) {
            (Some(close), Some(open)) => Some((consider, close, open)),
            _ => None,
        },
    };
    let crosses_closing = match limits {
        Some((_, close, _)) => {
            let without_hours = start
                .checked_add(span)
                .map_err(|_| DueDateError::OutOfRange)?;
            without_hours > at(start, close.hour(), close.minute())?
        }
        None => false,
    };
    let limits = limits.filter(|_| crosses_closing);
    if mode == DaysMode::Days {
        return match limits {
            Some((ConsiderLibraryHours::Close, close, _)) => {
                at(start, close.hour(), close.minute())
            }
            Some((_, _, open)) => {
                let next = with_date(
                    start,
                    start
                        .date()
                        .tomorrow()
                        .map_err(|_| DueDateError::OutOfRange)?,
                )?;
                at(&next, open.hour(), open.minute())
            }
            None => start
                .checked_add(span)
                .map_err(|_| DueDateError::OutOfRange),
        };
    }
    match limits {
        // Koha sets the closing time and adds nothing, so the calendar is not consulted.
        Some((ConsiderLibraryHours::Close, close, _)) => at(start, close.hour(), close.minute()),
        Some((_, _, open)) => add_hours(&at(start, open.hour(), open.minute())?, 24, context),
        None => add_hours(start, i64::from(hours), context),
    }
}

/// `from` plus `hours`; a closed day moves to the next open day at 10 o'clock.
///
/// Koha: `Koha::Calendar::addHours`
fn add_hours(
    from: &Zoned,
    hours: i64,
    context: &DueDateContext<'_>,
) -> Result<Zoned, DueDateError> {
    let due = from
        .checked_add(hours.hours())
        .map_err(|_| DueDateError::OutOfRange)?;
    let mode = context.rule.days_mode;
    if mode == DaysMode::Days || !context.calendar.is_closed(due.date()) {
        return Ok(due);
    }
    let day = context.calendar.next_open(due.date(), 1, mode.push())?;
    let moved = with_date(&due, day)?;
    moved
        .with()
        .hour(RETURN_BY_HOUR)
        .build()
        .map_err(|_| DueDateError::OutOfRange)
}

/// Koha: the hard due date part of `C4::Circulation::CalcDateDue`.
fn apply_hard_due_date(due: Zoned, hard: HardDueDate) -> Result<Zoned, DueDateError> {
    let hard_due = at(&with_date(&due, hard.date)?, 23, 59)?;
    let applies = match hard.compare {
        HardDueDateCompare::Exactly => true,
        HardDueDateCompare::Before => hard_due < due,
        HardDueDateCompare::After => hard_due > due,
    };
    Ok(if applies { hard_due } else { due })
}

/// No later than the patron's expiry, and not on a closed day.
///
/// Koha: the `ReturnBeforeExpiry` part of `C4::Circulation::CalcDateDue`.
fn before_expiry(due: Zoned, context: &DueDateContext<'_>) -> Result<Zoned, DueDateError> {
    let mut due = due;
    if let Some(expiry) = context.patron_expiry {
        // Koha compares wall-clock times ("floating"), not instants.
        let limit = expiry.at(23, 59, 0, 0);
        if due.datetime() > limit {
            due = limit
                .to_zoned(due.time_zone().clone())
                .map_err(|_| DueDateError::OutOfRange)?;
        }
    }
    let mode = context.rule.days_mode;
    if mode != DaysMode::Days && context.calendar.is_closed(due.date()) {
        let day = context.calendar.previous_open(due.date(), 1, mode.push())?;
        due = with_date(&due, day)?;
    }
    Ok(due)
}

fn plus_days(date: Date, days: i32) -> Result<Date, DueDateError> {
    date.checked_add(days.days())
        .map_err(|_| DueDateError::OutOfRange)
}

/// `zoned` on another day, at the same wall-clock time.
fn with_date(zoned: &Zoned, date: Date) -> Result<Zoned, DueDateError> {
    date.to_datetime(zoned.time())
        .to_zoned(zoned.time_zone().clone())
        .map_err(|_| DueDateError::OutOfRange)
}

/// `zoned` at `hour:minute` of its day.
fn at(zoned: &Zoned, hour: i8, minute: i8) -> Result<Zoned, DueDateError> {
    zoned
        .with()
        .hour(hour)
        .minute(minute)
        .build()
        .map_err(|_| DueDateError::OutOfRange)
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

    fn rule(days: u32, mode: DaysMode) -> LoanRule {
        LoanRule {
            issue_length: days,
            renewal_period: None,
            unit: LengthUnit::Days,
            days_mode: mode,
            hard_due_date: None,
        }
    }

    fn due_at(start: &Zoned, rule: &LoanRule, calendar: &Calendar) -> Zoned {
        let hours = LibraryHours::default();
        let context = DueDateContext {
            rule,
            calendar,
            hours: &hours,
            consider_hours: ConsiderLibraryHours::Ignore,
            return_before_expiry: false,
            patron_expiry: None,
        };
        calc_date_due(start, false, &context).unwrap()
    }

    fn due(days: u32, mode: DaysMode, calendar: &Calendar) -> String {
        due_at(&thursday(), &rule(days, mode), calendar)
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
    fn dayweek_steps_a_week_after_the_first_day_even_for_other_periods() {
        // 10 days from Thursday 12 is Sunday 22 (closed weekly): one day to Monday 23,
        // closed by a single holiday, so the search then moves a week to Monday 30.
        let calendar = library().on(date(2026, 11, 23), DatedEntry::Closed);
        assert_eq!(
            due(10, DaysMode::Dayweek, &calendar),
            "2026-11-30 23:59 -03:00"
        );
        assert_eq!(
            due(10, DaysMode::Datedue, &calendar),
            "2026-11-24 23:59 -03:00"
        );
    }

    #[test]
    fn dayweek_moves_one_day_from_a_weekly_closed_day() {
        let start = date(2026, 11, 14)
            .at(10, 0, 0, 0)
            .in_tz("America/Sao_Paulo")
            .unwrap();
        let due = due_at(&start, &rule(14, DaysMode::Dayweek), &library());
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
    fn hard_due_dates_cap_floor_or_fix() {
        let mut capped = rule(14, DaysMode::Days);
        let hard = |compare| HardDueDate {
            date: date(2026, 11, 20),
            compare,
        };
        capped.hard_due_date = Some(hard(HardDueDateCompare::Before));
        assert_eq!(
            due_at(&thursday(), &capped, &library()).date(),
            date(2026, 11, 20)
        );
        capped.hard_due_date = Some(hard(HardDueDateCompare::After));
        assert_eq!(
            due_at(&thursday(), &capped, &library()).date(),
            date(2026, 11, 26)
        );
        let mut short = rule(2, DaysMode::Days);
        short.hard_due_date = Some(hard(HardDueDateCompare::After));
        assert_eq!(
            due_at(&thursday(), &short, &library()).date(),
            date(2026, 11, 20)
        );
        short.hard_due_date = Some(hard(HardDueDateCompare::Exactly));
        assert_eq!(
            due_at(&thursday(), &short, &library()).date(),
            date(2026, 11, 20)
        );
    }

    #[test]
    fn koha_daysmode_names() {
        assert_eq!(DaysMode::from_koha("Datedue"), DaysMode::Datedue);
        assert_eq!(DaysMode::from_koha("datedue"), DaysMode::Other);
        assert_eq!(DaysMode::from_koha("noDays"), DaysMode::Other);
    }
}
