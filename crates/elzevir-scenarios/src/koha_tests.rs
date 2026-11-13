// SPDX-License-Identifier: GPL-3.0-or-later
//! Scenarios converted from Koha's own tests (`scenarios/<area>/<name>.t.yaml`).
//!
//! A Koha `.t` file fixes every input and asserts Koha's output, so it is an oracle that
//! needs no recording: each `is()` becomes a case with its inputs spelled out and the
//! expected value copied from the test. Cases run through the pure `transition()`.

use std::path::Path;

use elzevir_domain::calendar::{AnnualHoliday, Calendar, DatedEntry};
use elzevir_domain::catalog::Money;
use elzevir_domain::circulation::{
    CircContext, CircEvent, ItemState, RenewalBase, Transition, transition,
};
use elzevir_domain::due_date::{
    ConsiderLibraryHours, DaysMode, DueDateContext, HardDueDate, HardDueDateCompare, LengthUnit,
    LibraryHours, LoanRule, OpeningHours,
};
use elzevir_domain::fine::{FineContext, FineRule, FinesCalendar};
use jiff::Zoned;
use jiff::civil::{Date, DateTime, Time, Weekday};
use jiff::tz::TimeZone;
use serde::Deserialize;
use serde_norway::Value;

/// A `.t` file converted to cases.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KohaTestScenario {
    /// Unique id.
    pub id: String,
    /// The spec the cases prove.
    pub spec: String,
    /// `required` or `free`.
    pub parity: String,
    /// The Koha test, relative to `reference/koha/`.
    pub source: String,
    /// Context shared by every case; a case's own context is merged over it.
    #[serde(default)]
    pub defaults: Value,
    /// One case per assertion of the test.
    pub cases: Vec<Case>,
}

/// One assertion of the Koha test.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// The assertion's description in the Koha test.
    pub name: String,
    /// Context over the defaults.
    #[serde(default)]
    pub context: Value,
    /// The Item before the event.
    pub state: StateSpec,
    /// The event.
    pub event: EventSpec,
    /// What Koha's test expects.
    pub expect: Expect,
}

/// The Item's state, as written in a case.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StateSpec {
    /// Available.
    Available,
    /// On loan, due at a civil date-time in the case's time zone.
    OnLoan {
        /// `YYYY-MM-DDTHH:MM`.
        due: String,
    },
}

/// The event, as written in a case; times are civil date-times in the case's time zone.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum EventSpec {
    /// A checkout.
    Checkout {
        /// When.
        at: String,
    },
    /// A renewal.
    Renew {
        /// When.
        at: String,
    },
    /// A checkin.
    Checkin {
        /// When.
        at: String,
    },
}

/// What a case expects.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// The new due date, `YYYY-MM-DDTHH:MM`.
    #[serde(default)]
    pub due: Option<String>,
    /// The fine, as a decimal amount (`"29.00"`).
    #[serde(default)]
    pub fine: Option<String>,
}

/// The context of a case, with Koha's names for rules and preferences.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    non_snake_case,
    reason = "field names are Koha's system preference names"
)]
struct Context {
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    rule: Rule,
    #[serde(default)]
    useDaysMode: Option<String>,
    #[serde(default)]
    ReturnBeforeExpiry: Option<u8>,
    #[serde(default)]
    ConsiderLibraryHoursInCirculation: Option<String>,
    #[serde(default)]
    ManualRenewalPeriodBase: Option<String>,
    #[serde(default)]
    CalculateFinesOnReturn: Option<u8>,
    #[serde(default)]
    finesCalendar: Option<String>,
    #[serde(default)]
    FinesIncludeGracePeriod: Option<u8>,
    #[serde(default)]
    useDefaultReplacementCost: Option<u8>,
    #[serde(default)]
    dateexpiry: Option<String>,
    #[serde(default)]
    replacementprice: Option<String>,
    #[serde(default)]
    defaultreplacecost: Option<String>,
    #[serde(default)]
    overdue_recall: bool,
    #[serde(default)]
    calendar: CalendarSpec,
    #[serde(default)]
    library_hours: Vec<HoursSpec>,
}

/// Circulation rule values, as Koha stores them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    #[serde(default)]
    issuelength: Option<u32>,
    #[serde(default)]
    renewalperiod: Option<Length>,
    #[serde(default)]
    lengthunit: Option<String>,
    #[serde(default)]
    daysmode: Option<String>,
    #[serde(default)]
    hardduedate: Option<String>,
    #[serde(default)]
    hardduedatecompare: Option<i8>,
    #[serde(default)]
    fine: Option<String>,
    #[serde(default)]
    firstremind: Option<u32>,
    #[serde(default)]
    chargeperiod: Option<u32>,
    #[serde(default)]
    chargeperiod_charge_at: Option<u8>,
    #[serde(default)]
    overduefinescap: Option<String>,
    #[serde(default)]
    cap_fine_to_replacement_price: Option<u8>,
    #[serde(default)]
    recall_overdue_fine: Option<String>,
}

/// A length that Koha may also store as an empty string.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Length {
    Number(u32),
    Text(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarSpec {
    /// Weekdays closed every week, 0 = Sunday.
    #[serde(default)]
    weekly_closed: Vec<u8>,
    /// Fixed annual holidays as `[month, day]`.
    #[serde(default)]
    annual: Vec<[i8; 2]>,
    /// Single closed dates.
    #[serde(default)]
    closed: Vec<String>,
    /// Exceptions: open dates.
    #[serde(default)]
    open: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoursSpec {
    /// 0 = Sunday.
    day: u8,
    #[serde(default)]
    open: Option<String>,
    #[serde(default)]
    close: Option<String>,
}

/// A case that does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaseError {
    /// The case cannot be read.
    #[error("invalid case: {0}")]
    Invalid(String),
    /// Elzevir refused the event.
    #[error("Elzevir refused the event: {0}")]
    Refused(String),
    /// Elzevir and Koha disagree.
    #[error("{what}: Elzevir says {elzevir}, Koha's test expects {koha}")]
    Mismatch {
        /// Which value.
        what: &'static str,
        /// Elzevir's value.
        elzevir: String,
        /// The test's value.
        koha: String,
    },
}

/// Reads a `.t.yaml` file.
///
/// # Errors
///
/// When the file cannot be read or is not a [`KohaTestScenario`].
pub fn load_koha_test(path: &Path) -> Result<KohaTestScenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let failed = |e: serde_norway::Error| format!("{}: {e}", path.display());
    let mut value: Value = serde_norway::from_str(&text).map_err(failed)?;
    // Cases share context through YAML anchors and `<<` merge keys.
    value.apply_merge().map_err(failed)?;
    serde_norway::from_value(value).map_err(failed)
}

/// Runs every case; each failing case comes back with its name.
#[must_use]
pub fn run_koha_test(scenario: &KohaTestScenario) -> Vec<(String, CaseError)> {
    scenario
        .cases
        .iter()
        .filter_map(|case| {
            run_case(&scenario.defaults, case)
                .err()
                .map(|error| (case.name.clone(), error))
        })
        .collect()
}

fn invalid(what: impl std::fmt::Display) -> CaseError {
    CaseError::Invalid(what.to_string())
}

/// Runs one case.
///
/// # Errors
///
/// See [`CaseError`].
pub fn run_case(defaults: &Value, case: &Case) -> Result<(), CaseError> {
    let mut merged = defaults.clone();
    merge(&mut merged, &case.context);
    let context: Context = if merged.is_null() {
        Context::default()
    } else {
        serde_norway::from_value(merged).map_err(invalid)?
    };
    let zone = match &context.timezone {
        Some(name) => TimeZone::get(name).map_err(invalid)?,
        None => TimeZone::UTC,
    };
    let instant = |text: &str| -> Result<Zoned, CaseError> {
        let civil: DateTime = text
            .parse()
            .map_err(|_| invalid(format!("date-time {text}")))?;
        civil.to_zoned(zone.clone()).map_err(invalid)
    };
    let state = match &case.state {
        StateSpec::Available => ItemState::Available,
        StateSpec::OnLoan { due } => ItemState::OnLoan { due: instant(due)? },
    };
    let event = match &case.event {
        EventSpec::Checkout { at } => CircEvent::Checkout { at: instant(at)? },
        EventSpec::Renew { at } => CircEvent::Renew { at: instant(at)? },
        EventSpec::Checkin { at } => CircEvent::Checkin { at: instant(at)? },
    };
    let parts = Parts::from_context(&context)?;
    let circ = parts.circ_context();
    let result =
        transition(&state, &event, &circ).map_err(|e| CaseError::Refused(e.to_string()))?;
    compare(&result, &case.expect)
}

fn compare(result: &Transition, expect: &Expect) -> Result<(), CaseError> {
    if let Some(koha) = &expect.due {
        let elzevir = match &result.state {
            ItemState::OnLoan { due } => due.strftime("%Y-%m-%dT%H:%M").to_string(),
            ItemState::Available => "no loan".to_owned(),
        };
        if elzevir != *koha {
            return Err(CaseError::Mismatch {
                what: "due date",
                elzevir,
                koha: koha.clone(),
            });
        }
    }
    if let Some(koha) = &expect.fine {
        let expected = money(koha)?.unwrap_or(Money(0));
        let elzevir = result.fine.map_or(Money(0), |fine| fine.amount);
        if elzevir != expected {
            return Err(CaseError::Mismatch {
                what: "fine",
                elzevir: format_money(elzevir),
                koha: koha.clone(),
            });
        }
    }
    Ok(())
}

/// Recursive merge: mappings merge key by key, anything else is replaced.
fn merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (_, Value::Null) => {}
        (Value::Mapping(base), Value::Mapping(over)) => {
            for (key, value) in over {
                match base.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, over) => *base = over.clone(),
    }
}

/// The domain inputs built from a case's context.
struct Parts {
    loan: LoanRule,
    fine: FineRule,
    calendar: Calendar,
    hours: LibraryHours,
    due: DueSettings,
    fines: FineSettings,
    renewal_base: RenewalBase,
    fines_on_return: bool,
}

/// The due date's preferences and patron data.
struct DueSettings {
    consider_hours: ConsiderLibraryHours,
    return_before_expiry: bool,
    patron_expiry: Option<Date>,
}

/// The fine's preferences and Item data.
struct FineSettings {
    fines_calendar: FinesCalendar,
    include_grace: bool,
    replacement_price: Option<Money>,
    default_replacement_cost: Option<Money>,
    overdue_recall: bool,
}

impl Parts {
    fn from_context(context: &Context) -> Result<Self, CaseError> {
        let loan = loan_rule(context)?;
        let rule = &context.rule;
        let fine = FineRule {
            unit: loan.unit,
            grace: rule.firstremind.unwrap_or(0),
            charge_period: rule.chargeperiod.unwrap_or(0),
            charge_at_start: rule.chargeperiod_charge_at == Some(1),
            fine: optional_money(rule.fine.as_deref())?,
            cap: optional_money(rule.overduefinescap.as_deref())?,
            cap_to_replacement_price: rule.cap_fine_to_replacement_price == Some(1),
            recall_fine: optional_money(rule.recall_overdue_fine.as_deref())?,
        };
        let consider_hours = match context
            .ConsiderLibraryHoursInCirculation
            .as_deref()
            .unwrap_or("ignore")
        {
            "ignore" => ConsiderLibraryHours::Ignore,
            "close" => ConsiderLibraryHours::Close,
            "open" => ConsiderLibraryHours::Open,
            other => {
                return Err(invalid(format!(
                    "ConsiderLibraryHoursInCirculation {other}"
                )));
            }
        };
        let fines_calendar = match context.finesCalendar.as_deref().unwrap_or("ignoreCalendar") {
            "ignoreCalendar" => FinesCalendar::Ignore,
            "noFinesWhenClosed" => FinesCalendar::NoFinesWhenClosed,
            other => return Err(invalid(format!("finesCalendar {other}"))),
        };
        let renewal_base = match context
            .ManualRenewalPeriodBase
            .as_deref()
            .unwrap_or("date_due")
        {
            "date_due" => RenewalBase::DueDate,
            "now" => RenewalBase::Now,
            other => return Err(invalid(format!("ManualRenewalPeriodBase {other}"))),
        };
        let use_default_cost = context.useDefaultReplacementCost == Some(1);
        Ok(Self {
            loan,
            fine,
            calendar: calendar(&context.calendar)?,
            hours: library_hours(&context.library_hours)?,
            due: DueSettings {
                consider_hours,
                return_before_expiry: context.ReturnBeforeExpiry == Some(1),
                patron_expiry: context.dateexpiry.as_deref().map(date).transpose()?,
            },
            fines: FineSettings {
                fines_calendar,
                include_grace: context.FinesIncludeGracePeriod == Some(1),
                replacement_price: optional_money(context.replacementprice.as_deref())?,
                default_replacement_cost: if use_default_cost {
                    optional_money(context.defaultreplacecost.as_deref())?
                } else {
                    None
                },
                overdue_recall: context.overdue_recall,
            },
            renewal_base,
            fines_on_return: context.CalculateFinesOnReturn == Some(1),
        })
    }

    fn circ_context(&self) -> CircContext<'_> {
        CircContext {
            due_date: DueDateContext {
                rule: &self.loan,
                calendar: &self.calendar,
                hours: &self.hours,
                consider_hours: self.due.consider_hours,
                return_before_expiry: self.due.return_before_expiry,
                patron_expiry: self.due.patron_expiry,
            },
            fine: FineContext {
                rule: &self.fine,
                calendar: &self.calendar,
                fines_calendar: self.fines.fines_calendar,
                include_grace: self.fines.include_grace,
                replacement_price: self.fines.replacement_price,
                default_replacement_cost: self.fines.default_replacement_cost,
                overdue_recall: self.fines.overdue_recall,
            },
            renewal_base: self.renewal_base,
            fines_on_return: self.fines_on_return,
        }
    }
}

/// The loan part of the rule.
fn loan_rule(context: &Context) -> Result<LoanRule, CaseError> {
    let rule = &context.rule;
    let unit = match rule.lengthunit.as_deref().unwrap_or("days") {
        "days" => LengthUnit::Days,
        "hours" => LengthUnit::Hours,
        other => return Err(invalid(format!("lengthunit {other}"))),
    };
    // Koha: `Koha::CirculationRules::get_effective_daysmode` (an empty rule falls
    // back to the preference, whose default is `Calendar`).
    let days_mode = DaysMode::from_koha(
        rule.daysmode
            .as_deref()
            .filter(|mode| !mode.is_empty())
            .or(context.useDaysMode.as_deref())
            .unwrap_or("Calendar"),
    );
    let renewal_period = match &rule.renewalperiod {
        None => None,
        Some(Length::Number(days)) => Some(*days),
        Some(Length::Text(text)) if text.is_empty() => None,
        Some(Length::Text(text)) => Some(text.parse().map_err(|_| invalid(text))?),
    };
    let hard_due_date = match &rule.hardduedate {
        Some(text) if !text.is_empty() => Some(HardDueDate {
            date: date(text)?,
            compare: match rule.hardduedatecompare.unwrap_or(-1) {
                -1 => HardDueDateCompare::Before,
                0 => HardDueDateCompare::Exactly,
                1 => HardDueDateCompare::After,
                other => return Err(invalid(format!("hardduedatecompare {other}"))),
            },
        }),
        _ => None,
    };
    Ok(LoanRule {
        issue_length: rule.issuelength.unwrap_or(0),
        renewal_period,
        unit,
        days_mode,
        hard_due_date,
    })
}

fn date(text: &str) -> Result<Date, CaseError> {
    text.parse().map_err(|_| invalid(format!("date {text}")))
}

fn time(text: &str) -> Result<Time, CaseError> {
    text.parse().map_err(|_| invalid(format!("time {text}")))
}

fn calendar(spec: &CalendarSpec) -> Result<Calendar, CaseError> {
    let mut calendar = Calendar::always_open();
    for &day in &spec.weekly_closed {
        let weekday = i8::try_from(day)
            .ok()
            .and_then(|day| Weekday::from_sunday_zero_offset(day).ok())
            .ok_or_else(|| invalid(format!("weekday {day}")))?;
        calendar = calendar.closed_every(weekday);
    }
    for &[month, day] in &spec.annual {
        calendar = calendar.closed_annually(AnnualHoliday { month, day });
    }
    for text in &spec.closed {
        calendar = calendar.on(date(text)?, DatedEntry::Closed);
    }
    for text in &spec.open {
        calendar = calendar.on(date(text)?, DatedEntry::Open);
    }
    Ok(calendar)
}

fn library_hours(specs: &[HoursSpec]) -> Result<LibraryHours, CaseError> {
    let mut hours = LibraryHours::default();
    for spec in specs {
        let slot = hours
            .0
            .get_mut(usize::from(spec.day))
            .ok_or_else(|| invalid(format!("library hours day {}", spec.day)))?;
        *slot = Some(OpeningHours {
            open: spec.open.as_deref().map(time).transpose()?,
            close: spec.close.as_deref().map(time).transpose()?,
        });
    }
    Ok(hours)
}

/// A decimal amount in cents; empty is `None`, like Koha's empty rule values.
fn money(text: &str) -> Result<Option<Money>, CaseError> {
    if text.is_empty() {
        return Ok(None);
    }
    let bad = || invalid(format!("amount {text}"));
    let (whole, cents) = text.split_once('.').unwrap_or((text, ""));
    let whole: i64 = whole.parse().map_err(|_| bad())?;
    let cents: i64 = match cents.len() {
        0 => 0,
        1 => cents.parse::<i64>().map_err(|_| bad())? * 10,
        2 => cents.parse().map_err(|_| bad())?,
        _ => {
            let (kept, rest) = cents.split_at(2);
            if rest.bytes().any(|b| b != b'0') {
                return Err(bad());
            }
            kept.parse().map_err(|_| bad())?
        }
    };
    Ok(Some(Money(whole * 100 + cents)))
}

fn optional_money(text: Option<&str>) -> Result<Option<Money>, CaseError> {
    text.map_or(Ok(None), money)
}

fn format_money(Money(cents): Money) -> String {
    format!("{}.{:02}", cents / 100, (cents % 100).abs())
}
