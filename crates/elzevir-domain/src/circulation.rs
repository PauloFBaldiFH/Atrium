// SPDX-License-Identifier: GPL-3.0-or-later
//! The circulation state machine of one Item: pure, so the same state and event always
//! give the same result (docs/arquitetura/03-estrategia-reconstrucao.md, §3.5, and
//! `docs/specs/circulacao/transicao.md`).
//!
//! v0 knows two states (available, on loan) and three events (checkout, renewal,
//! checkin). Holds, transfers, lost and withdrawn Items come with their tasks.

use jiff::Zoned;

use crate::due_date::{DueDateContext, DueDateError, calc_date_due};
use crate::fine::{Fine, FineContext, calc_fine};

/// Where an Item is in circulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    /// On the shelf, can be lent.
    Available,
    /// Lent, due at `due`.
    OnLoan {
        /// When it is due.
        due: Zoned,
    },
}

/// Something that happens to an Item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircEvent {
    /// Lent at `at`.
    Checkout {
        /// When.
        at: Zoned,
    },
    /// Renewed at `at`.
    Renew {
        /// When.
        at: Zoned,
    },
    /// Returned at `at`.
    Checkin {
        /// When.
        at: Zoned,
    },
}

/// Where a renewal's new period starts (`ManualRenewalPeriodBase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenewalBase {
    /// From the current due date (`date_due`).
    DueDate,
    /// From the moment of the renewal (`now`).
    Now,
}

/// The rules and settings that apply to the Item, its patron and the library.
#[derive(Debug, Clone, Copy)]
pub struct CircContext<'a> {
    /// What the due date reads.
    pub due_date: DueDateContext<'a>,
    /// What the fine reads.
    pub fine: FineContext<'a>,
    /// `ManualRenewalPeriodBase`.
    pub renewal_base: RenewalBase,
    /// `CalculateFinesOnReturn`.
    pub fines_on_return: bool,
}

/// The result of an accepted event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// The Item's new state.
    pub state: ItemState,
    /// The fine charged by the event, if it computes one (a checkin with fines on return).
    pub fine: Option<Fine>,
}

/// Why an event was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// A checkout of an Item that is not available.
    #[error("the item is not available")]
    NotAvailable,
    /// A renewal or checkin of an Item that is not on loan.
    #[error("the item is not on loan")]
    NotOnLoan,
    /// The due date could not be computed.
    #[error(transparent)]
    DueDate(#[from] DueDateError),
}

/// Applies `event` to an Item in `state`.
///
/// # Errors
///
/// [`Refusal`] when the event does not apply to the state, or no due date can be found.
///
/// Koha: `C4::Circulation::AddIssue`, `C4::Circulation::AddRenewal`, `C4::Circulation::AddReturn`
///
/// Only the due date and the fine; everything else they do comes with later tasks.
pub fn transition(
    state: &ItemState,
    event: &CircEvent,
    context: &CircContext<'_>,
) -> Result<Transition, Refusal> {
    match (state, event) {
        (ItemState::Available, CircEvent::Checkout { at }) => Ok(Transition {
            state: ItemState::OnLoan {
                due: to_minute(&calc_date_due(at, false, &context.due_date)?)?,
            },
            fine: None,
        }),
        (ItemState::OnLoan { due }, CircEvent::Renew { at }) => {
            let start = match context.renewal_base {
                RenewalBase::DueDate => due,
                RenewalBase::Now => at,
            };
            Ok(Transition {
                state: ItemState::OnLoan {
                    due: calc_date_due(start, true, &context.due_date)?,
                },
                fine: None,
            })
        }
        (ItemState::OnLoan { due }, CircEvent::Checkin { at }) => Ok(Transition {
            state: ItemState::Available,
            fine: context
                .fines_on_return
                .then(|| calc_fine(due, at, &context.fine)),
        }),
        (_, CircEvent::Checkout { .. }) => Err(Refusal::NotAvailable),
        (ItemState::Available, CircEvent::Renew { .. } | CircEvent::Checkin { .. }) => {
            Err(Refusal::NotOnLoan)
        }
    }
}

/// `due` without seconds: a checkout's due date is kept to the minute, while a renewal
/// from "now" keeps the seconds of the moment it happened.
///
/// Koha: `C4::Circulation::AddIssue` (`$datedue->truncate( to => 'minute' )`)
fn to_minute(due: &Zoned) -> Result<Zoned, DueDateError> {
    due.with()
        .second(0)
        .subsec_nanosecond(0)
        .build()
        .map_err(|_| DueDateError::OutOfRange)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::Calendar;
    use crate::catalog::Money;
    use crate::due_date::{ConsiderLibraryHours, DaysMode, LengthUnit, LibraryHours, LoanRule};
    use crate::fine::{FineRule, FinesCalendar};
    use jiff::civil::date;

    fn at(day: i8, hour: i8, second: i8) -> Zoned {
        date(2026, 11, day)
            .at(hour, 0, second, 0)
            .in_tz("UTC")
            .unwrap()
    }

    fn run(
        state: &ItemState,
        event: &CircEvent,
        renewal_base: RenewalBase,
        fines_on_return: bool,
    ) -> Result<Transition, Refusal> {
        let loan = LoanRule {
            issue_length: 14,
            renewal_period: Some(7),
            unit: LengthUnit::Days,
            days_mode: DaysMode::Days,
            hard_due_date: None,
        };
        let fine = FineRule {
            unit: LengthUnit::Days,
            grace: 0,
            charge_period: 1,
            charge_at_start: false,
            fine: Some(Money(50)),
            cap: None,
            cap_to_replacement_price: false,
            recall_fine: None,
        };
        let calendar = Calendar::always_open();
        let hours = LibraryHours::default();
        let context = CircContext {
            due_date: DueDateContext {
                rule: &loan,
                calendar: &calendar,
                hours: &hours,
                consider_hours: ConsiderLibraryHours::Ignore,
                return_before_expiry: false,
                patron_expiry: None,
            },
            fine: FineContext {
                rule: &fine,
                calendar: &calendar,
                fines_calendar: FinesCalendar::Ignore,
                include_grace: false,
                replacement_price: None,
                default_replacement_cost: None,
                overdue_recall: false,
            },
            renewal_base,
            fines_on_return,
        };
        transition(state, event, &context)
    }

    fn on_loan(due: Zoned) -> ItemState {
        ItemState::OnLoan { due }
    }

    #[test]
    fn checkout_is_due_at_the_minute() {
        let result = run(
            &ItemState::Available,
            &CircEvent::Checkout { at: at(1, 10, 25) },
            RenewalBase::DueDate,
            true,
        )
        .unwrap();
        let due = date(2026, 11, 15).at(23, 59, 0, 0).in_tz("UTC").unwrap();
        assert_eq!(result.state, on_loan(due));
        assert_eq!(result.fine, None);
    }

    #[test]
    fn renewal_starts_from_the_due_date_or_from_now() {
        let state = on_loan(at(15, 23, 0));
        let renew = CircEvent::Renew { at: at(10, 9, 0) };
        let from_due = run(&state, &renew, RenewalBase::DueDate, false).unwrap();
        let from_now = run(&state, &renew, RenewalBase::Now, false).unwrap();
        let due = |day| date(2026, 11, day).at(23, 59, 0, 0).in_tz("UTC").unwrap();
        assert_eq!(from_due.state, on_loan(due(22)));
        assert_eq!(from_now.state, on_loan(due(17)));
    }

    #[test]
    fn checkin_charges_only_with_fines_on_return() {
        let state = on_loan(at(10, 23, 0));
        let checkin = CircEvent::Checkin { at: at(13, 9, 0) };
        let with = run(&state, &checkin, RenewalBase::DueDate, true).unwrap();
        let without = run(&state, &checkin, RenewalBase::DueDate, false).unwrap();
        assert_eq!(with.state, ItemState::Available);
        assert_eq!(with.fine.map(|fine| fine.amount), Some(Money(150)));
        assert_eq!(without.fine, None);
    }

    #[test]
    fn events_that_do_not_fit_the_state_are_refused() {
        let lent = on_loan(at(10, 23, 0));
        let checkout = CircEvent::Checkout { at: at(1, 10, 0) };
        let checkin = CircEvent::Checkin { at: at(1, 10, 0) };
        assert_eq!(
            run(&lent, &checkout, RenewalBase::DueDate, false),
            Err(Refusal::NotAvailable)
        );
        assert_eq!(
            run(&ItemState::Available, &checkin, RenewalBase::DueDate, false),
            Err(Refusal::NotOnLoan)
        );
    }
}
