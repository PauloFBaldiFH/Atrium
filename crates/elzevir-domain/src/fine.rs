// SPDX-License-Identifier: GPL-3.0-or-later
//! The overdue fine of a loan. See `docs/specs/circulacao/multa.md`.

use jiff::Zoned;

use crate::calendar::Calendar;
use crate::catalog::Money;
use crate::due_date::LengthUnit;

/// The fine part of a circulation rule, already resolved for the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FineRule {
    /// `lengthunit`: fines are counted in the loan's unit.
    pub unit: LengthUnit,
    /// `firstremind`: units of grace.
    pub grace: u32,
    /// `chargeperiod`: units per charge; `0` charges nothing.
    pub charge_period: u32,
    /// `chargeperiod_charge_at = 1`: charge at the start of each period, not at its end.
    pub charge_at_start: bool,
    /// `fine` per period; `None` when unset or empty.
    pub fine: Option<Money>,
    /// `overduefinescap`; only a positive value caps.
    pub cap: Option<Money>,
    /// `cap_fine_to_replacement_price`.
    pub cap_to_replacement_price: bool,
    /// `recall_overdue_fine` per period, used while the record has an overdue recall.
    pub recall_fine: Option<Money>,
}

/// How closed days count (`finesCalendar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinesCalendar {
    /// Every day counts (`ignoreCalendar`).
    Ignore,
    /// Closed days do not count (`noFinesWhenClosed`).
    NoFinesWhenClosed,
}

/// Everything `calc_fine` reads besides the dates.
#[derive(Debug, Clone, Copy)]
pub struct FineContext<'a> {
    /// The circulation rule.
    pub rule: &'a FineRule,
    /// The calendar of the library whose rules apply.
    pub calendar: &'a Calendar,
    /// `finesCalendar`.
    pub fines_calendar: FinesCalendar,
    /// `FinesIncludeGracePeriod`: once past the grace, charge the grace units too.
    pub include_grace: bool,
    /// The Item's replacement price.
    pub replacement_price: Option<Money>,
    /// The item type's default replacement cost when `useDefaultReplacementCost` is on.
    pub default_replacement_cost: Option<Money>,
    /// Whether the record has an overdue recall.
    pub overdue_recall: bool,
}

/// A computed fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fine {
    /// What is owed.
    pub amount: Money,
    /// Overdue units past the grace (may be negative).
    pub units_minus_grace: i64,
    /// Overdue units.
    pub chargeable_units: i64,
}

impl Fine {
    const NONE: Self = Self {
        amount: Money(0),
        units_minus_grace: 0,
        chargeable_units: 0,
    };
}

/// The fine for a loan due at `due` and returned (or counted) at `end`.
///
/// Koha: `C4::Overdues::CalcFine`
#[must_use]
pub fn calc_fine(due: &Zoned, end: &Zoned, context: &FineContext<'_>) -> Fine {
    if due >= end {
        return Fine::NONE;
    }
    let rule = context.rule;
    let chargeable_units = chargeable_units(rule.unit, due, end, context);
    let units_minus_grace = chargeable_units - i64::from(rule.grace);
    let mut amount = 0_i64;
    if rule.charge_period > 0 && units_minus_grace > 0 {
        let units = if context.include_grace {
            chargeable_units
        } else {
            units_minus_grace
        };
        let period = i64::from(rule.charge_period);
        // `units` is positive here, so integer division is Koha's floor.
        let periods = if rule.charge_at_start {
            (units + period - 1) / period
        } else {
            units / period
        };
        // With an overdue recall Koha charges the recall fine, or nothing at all when the
        // rule has none: the normal fine only applies without recalls.
        let per_period = if context.overdue_recall {
            rule.recall_fine
        } else {
            rule.fine
        };
        if let Some(Money(per_period)) = per_period {
            amount = periods.saturating_mul(per_period);
        }
    }
    if let Some(Money(cap)) = rule.cap
        && cap > 0
        && amount > cap
    {
        amount = cap;
    }
    let replacement_price = match context.replacement_price {
        Some(Money(price)) if price != 0 => Some(price),
        _ => context.default_replacement_cost.map(|Money(cost)| cost),
    };
    if rule.cap_to_replacement_price
        && let Some(price) = replacement_price
        && price != 0
        && amount > price
    {
        amount = price;
    }
    Fine {
        amount: Money(amount),
        units_minus_grace,
        chargeable_units,
    }
}

/// Overdue units between `due` and `end`.
///
/// Koha: `C4::Overdues::get_chargeable_units`, `Koha::Calendar::days_between` and
/// `hours_between`.
fn chargeable_units(unit: LengthUnit, due: &Zoned, end: &Zoned, context: &FineContext<'_>) -> i64 {
    let closed = || match context.fines_calendar {
        FinesCalendar::Ignore => 0,
        FinesCalendar::NoFinesWhenClosed => context.calendar.closed_days(due.date(), end.date()),
    };
    match unit {
        LengthUnit::Days => {
            // Koha counts calendar dates, not 24-hour spans.
            let days = (end.date() - due.date()).get_days();
            i64::from(days) - closed()
        }
        LengthUnit::Hours => {
            // Koha's `delta_ms` keeps minutes and seconds apart, on wall-clock times, and
            // takes away 24 hours for each closed day.
            let seconds = end.datetime().duration_since(due.datetime()).as_secs();
            let minutes = seconds / 60 - closed() * 24 * 60;
            let hours = minutes / 60;
            // Koha: less than an hour late, but with seconds over the minute, is one hour;
            // a whole number of minutes under an hour is none.
            if hours == 0 && seconds % 60 > 0 {
                1
            } else {
                hours
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn rule() -> FineRule {
        FineRule {
            unit: LengthUnit::Days,
            grace: 0,
            charge_period: 1,
            charge_at_start: false,
            fine: Some(Money(100)),
            cap: None,
            cap_to_replacement_price: false,
            recall_fine: None,
        }
    }

    fn fine(rule: &FineRule, due: (i8, i8, i8), end: (i8, i8, i8)) -> Fine {
        let calendar = Calendar::always_open();
        let context = FineContext {
            rule,
            calendar: &calendar,
            fines_calendar: FinesCalendar::Ignore,
            include_grace: false,
            replacement_price: None,
            default_replacement_cost: None,
            overdue_recall: false,
        };
        let at = |(d, h, m): (i8, i8, i8)| date(2026, 11, d).at(h, m, 0, 0).in_tz("UTC").unwrap();
        calc_fine(&at(due), &at(end), &context)
    }

    #[test]
    fn nothing_is_owed_before_the_due_date() {
        assert_eq!(fine(&rule(), (10, 23, 59), (10, 23, 59)), Fine::NONE);
        assert_eq!(fine(&rule(), (10, 23, 59), (9, 10, 0)), Fine::NONE);
    }

    #[test]
    fn days_count_dates_and_grace_comes_off() {
        let mut graced = rule();
        graced.grace = 2;
        assert_eq!(fine(&rule(), (10, 23, 59), (11, 0, 1)).amount, Money(100));
        let late = fine(&graced, (10, 23, 59), (15, 9, 0));
        assert_eq!(
            (late.amount, late.units_minus_grace, late.chargeable_units),
            (Money(300), 3, 5)
        );
    }

    #[test]
    fn charge_periods_round_down_or_up() {
        let mut weekly = rule();
        weekly.charge_period = 7;
        assert_eq!(fine(&weekly, (1, 23, 59), (11, 9, 0)).amount, Money(100));
        weekly.charge_at_start = true;
        assert_eq!(fine(&weekly, (1, 23, 59), (11, 9, 0)).amount, Money(200));
    }

    #[test]
    fn hours_under_one_count_only_with_stray_seconds() {
        let mut hourly = rule();
        hourly.unit = LengthUnit::Hours;
        assert_eq!(fine(&hourly, (10, 10, 0), (10, 10, 30)).chargeable_units, 0);
        assert_eq!(fine(&hourly, (10, 10, 0), (10, 12, 30)).chargeable_units, 2);
    }
}
