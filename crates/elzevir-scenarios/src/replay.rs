// SPDX-License-Identifier: GPL-3.0-or-later
//! Recomputes a recording's outputs with the Elzevir domain.

use elzevir_domain::calendar::{AnnualHoliday, Calendar, DatedEntry};
use elzevir_domain::due_date::{DaysMode, DueDateError, LoanDays, due_date};
use jiff::Timestamp;
use jiff::civil::{Date, Weekday};
use jiff::fmt::temporal::DateTimeParser;
use jiff::tz::TimeZone;

use crate::model::{
    CalendarSnapshot, Expected, LoanRecorded, RecordedInputs, RecordedOutput, Recording, Scenario,
};

/// Why a scenario does not match its recording.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    /// The scenario changed after it was recorded.
    #[error("the recording is for a different version of the scenario; record it again")]
    Stale,
    /// The recording was made with another Koha version.
    #[error("recorded with Koha {recorded}, but koha-lock.json pins {pinned}")]
    KohaVersion {
        /// In the recording.
        recorded: String,
        /// In `koha-lock.json`.
        pinned: String,
    },
    /// Steps, expectations and outputs do not line up.
    #[error("the scenario has {steps} steps but the recording has {outputs} outputs")]
    Shape {
        /// Steps in the scenario.
        steps: usize,
        /// Outputs in the recording.
        outputs: usize,
    },
    /// The recorded inputs use something outside the v0 scope of the spec.
    #[error("not supported yet: {0}")]
    Unsupported(String),
    /// A recorded value could not be read.
    #[error("invalid recorded value: {0}")]
    Invalid(String),
    /// Elzevir could not compute the value.
    #[error(transparent)]
    DueDate(#[from] DueDateError),
    /// Elzevir and Koha disagree.
    #[error("Elzevir says {elzevir}, Koha said {koha}")]
    Mismatch {
        /// Elzevir's value.
        elzevir: String,
        /// Koha's value.
        koha: String,
    },
}

/// Checks that `recording` belongs to `scenario` and that Elzevir reproduces its outputs.
///
/// # Errors
///
/// The first problem found; see [`CheckError`].
pub fn check(
    scenario: &Scenario,
    recording: &Recording,
    pinned_koha: &str,
) -> Result<(), CheckError> {
    if recording.scenario != *scenario {
        return Err(CheckError::Stale);
    }
    if recording.koha_version != pinned_koha {
        return Err(CheckError::KohaVersion {
            recorded: recording.koha_version.clone(),
            pinned: pinned_koha.to_owned(),
        });
    }
    if scenario.when.len() != recording.outputs.len()
        || scenario.then.len() != recording.outputs.len()
    {
        return Err(CheckError::Shape {
            steps: scenario.when.len(),
            outputs: recording.outputs.len(),
        });
    }
    for (expected, output) in scenario.then.iter().zip(&recording.outputs) {
        match (expected, output) {
            (Expected::Loan { due }, RecordedOutput::Loan(loan)) => {
                if due != "recorded" {
                    return Err(CheckError::Unsupported(format!("fixed due date `{due}`")));
                }
                check_loan(&recording.inputs, loan)?;
            }
        }
    }
    Ok(())
}

fn check_loan(inputs: &RecordedInputs, loan: &LoanRecorded) -> Result<(), CheckError> {
    let (length, mode) = loan_rule(inputs)?;
    let calendar = calendar(&inputs.calendar)?;
    let start = parse_instant(&loan.checkout_date)?;
    let koha_due = parse_instant(&loan.due_date)?;
    let elzevir_due = due_date(&start, length, mode, &calendar)?;
    if elzevir_due.timestamp() != koha_due.timestamp() || elzevir_due.offset() != koha_due.offset()
    {
        return Err(CheckError::Mismatch {
            elzevir: elzevir_due.strftime("%FT%T%:z").to_string(),
            koha: loan.due_date.clone(),
        });
    }
    Ok(())
}

/// Loan length and days mode, refusing inputs outside the v0 spec.
///
/// Koha: `Koha::CirculationRules::get_effective_daysmode`
fn loan_rule(inputs: &RecordedInputs) -> Result<(LoanDays, DaysMode), CheckError> {
    let rule = &inputs.rule;
    if rule.lengthunit.as_deref().unwrap_or("days") != "days" {
        return Err(CheckError::Unsupported("loan length in hours".to_owned()));
    }
    if rule.hardduedate.as_deref().is_some_and(|d| !d.is_empty()) {
        return Err(CheckError::Unsupported("hard due date".to_owned()));
    }
    if inputs.return_before_expiry != "0" {
        return Err(CheckError::Unsupported("ReturnBeforeExpiry".to_owned()));
    }
    let days = rule
        .issuelength
        .as_deref()
        .unwrap_or_default()
        .parse::<u16>()
        .map_err(|_| CheckError::Invalid(format!("issuelength {:?}", rule.issuelength)))?;
    let mode_name = rule
        .daysmode
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or(&inputs.use_days_mode);
    let mode = DaysMode::from_koha(mode_name)
        .ok_or_else(|| CheckError::Invalid(format!("daysmode {mode_name:?}")))?;
    Ok((LoanDays(days), mode))
}

fn calendar(snapshot: &CalendarSnapshot) -> Result<Calendar, CheckError> {
    let mut calendar = Calendar::always_open();
    for &day in &snapshot.weekly_closed {
        let weekday = Weekday::from_sunday_zero_offset(i8::try_from(day).unwrap_or(i8::MAX))
            .map_err(|_| CheckError::Invalid(format!("weekday {day}")))?;
        calendar = calendar.closed_every(weekday);
    }
    for &[month, day] in &snapshot.annual {
        let (Ok(month), Ok(day)) = (i8::try_from(month), i8::try_from(day)) else {
            return Err(CheckError::Invalid(format!("annual holiday {month}/{day}")));
        };
        calendar = calendar.closed_annually(AnnualHoliday { month, day });
    }
    for (dates, entry) in [
        (&snapshot.closed, DatedEntry::Closed),
        (&snapshot.open, DatedEntry::Open),
    ] {
        for date in dates {
            let date: Date = date
                .parse()
                .map_err(|_| CheckError::Invalid(format!("date {date}")))?;
            calendar = calendar.on(date, entry);
        }
    }
    Ok(calendar)
}

/// An RFC 3339 instant kept in its own fixed offset, as Koha wrote it.
fn parse_instant(text: &str) -> Result<jiff::Zoned, CheckError> {
    let invalid = || CheckError::Invalid(format!("instant {text}"));
    let parsed = DateTimeParser::new()
        .parse_pieces(text)
        .map_err(|_| invalid())?;
    let offset = parsed
        .offset()
        .map(|o| o.to_numeric_offset())
        .ok_or_else(invalid)?;
    let timestamp: Timestamp = text.parse().map_err(|_| invalid())?;
    Ok(timestamp.to_zoned(TimeZone::fixed(offset)))
}
