// SPDX-License-Identifier: GPL-3.0-or-later
//! File formats: scenario (YAML) and oracle recording (JSON).

use serde::{Deserialize, Serialize};

/// What a scenario does. Field names follow Koha so the recorder needs no alias table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Unique id, also the file name.
    pub id: String,
    /// The spec the scenario proves.
    pub spec: String,
    /// `required` or `free` (doc 04, §2).
    pub parity: String,
    /// The world before the steps.
    pub given: Given,
    /// Steps, in order.
    #[serde(with = "serde_norway::with::singleton_map_recursive")]
    pub when: Vec<Step>,
    /// What to compare.
    #[serde(with = "serde_norway::with::singleton_map_recursive")]
    pub then: Vec<Expected>,
}

/// The world before the steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Given {
    /// The patron who borrows; the recorder uses the one authorised example account.
    pub patron: PatronGiven,
    /// The item to lend; the recorder picks an available one that matches.
    pub item: ItemGiven,
}

/// The borrowing patron.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatronGiven {
    /// Koha `categorycode`.
    pub category: String,
}

/// The lent item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemGiven {
    /// Koha `itemtype`.
    pub itemtype: String,
    /// Koha `branchcode` of the item's home library.
    pub branch: String,
}

/// One step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Lend the item to the patron.
    Checkout(Checkout),
}

/// A checkout at a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkout {
    /// Koha `branchcode` where the checkout happens.
    pub branch: String,
}

/// One comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Expected {
    /// Compare the loan's due date with the recording.
    Loan {
        /// Always `recorded` in v0.
        due: String,
    },
}

/// What the oracle did for one scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    /// Koha version that produced it; must match `koha-lock.json`.
    pub koha_version: String,
    /// The scenario as recorded; a different scenario needs a new recording.
    pub scenario: Scenario,
    /// Effective inputs Koha used.
    pub inputs: RecordedInputs,
    /// Koha's outputs, one per step.
    pub outputs: Vec<RecordedOutput>,
}

/// Effective inputs, captured from the oracle at recording time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedInputs {
    /// Resolved circulation rule for the checkout's library, category and item type.
    pub rule: RuleSnapshot,
    /// System preference `useDaysMode`.
    pub use_days_mode: String,
    /// System preference `ReturnBeforeExpiry`.
    pub return_before_expiry: String,
    /// System preference `ConsiderLibraryHoursInCirculation`.
    pub consider_library_hours: String,
    /// The checkout library's calendar.
    pub calendar: CalendarSnapshot,
}

/// The circulation rule values that `CalcDateDue` reads, as Koha returns them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSnapshot {
    /// `issuelength`.
    pub issuelength: Option<String>,
    /// `lengthunit`.
    pub lengthunit: Option<String>,
    /// `daysmode`.
    pub daysmode: Option<String>,
    /// `hardduedate`.
    pub hardduedate: Option<String>,
}

/// A library calendar as the staff calendar screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSnapshot {
    /// Weekdays closed every week, 0 = Sunday.
    pub weekly_closed: Vec<u8>,
    /// Fixed annual holidays as `[month, day]`.
    pub annual: Vec<[u8; 2]>,
    /// Single closed dates, `YYYY-MM-DD`.
    pub closed: Vec<String>,
    /// Exception dates (open despite a repeating holiday), `YYYY-MM-DD`.
    pub open: Vec<String>,
}

/// Koha's result for one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordedOutput {
    /// A loan was created.
    Loan(LoanRecorded),
}

/// A loan as Koha returned it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoanRecorded {
    /// When the checkout happened, RFC 3339 with offset.
    pub checkout_date: String,
    /// When it is due, RFC 3339 with offset.
    pub due_date: String,
}
