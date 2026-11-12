// SPDX-License-Identifier: GPL-3.0-or-later
//! The Koha staff interface and REST API, driven through `curl`.

use std::io::Write;
use std::process::{Command, Stdio};

use atrium_scenarios::{CalendarSnapshot, LoanRecorded, RecordedInputs, RuleSnapshot};
use serde_json::Value;

use crate::Error;

/// A signed-in staff session on the oracle.
pub(crate) struct Oracle {
    base: String,
    basic: Option<(String, String)>,
    cookies: tempfile::TempDir,
}

/// An item picked for a checkout.
pub(crate) struct Item {
    id: i64,
    barcode: String,
}

impl Oracle {
    pub(crate) fn connect() -> Result<Self, Error> {
        let var = |name: &str| std::env::var(name).map_err(|_| format!("{name} is not set"));
        let base = var("ATRIUM_ORACLE_URL")?.trim_end_matches('/').to_owned();
        let basic = match (
            std::env::var("ATRIUM_ORACLE_BASIC_USER"),
            std::env::var("ATRIUM_ORACLE_BASIC_PASSWORD"),
        ) {
            (Ok(user), Ok(password)) => Some((user, password)),
            _ => None,
        };
        let oracle = Self {
            base,
            basic,
            cookies: tempfile::tempdir()?,
        };
        let page = oracle.get("/cgi-bin/koha/mainpage.pl")?;
        let token = csrf_token(&page).ok_or("no CSRF token on the login page")?;
        let signed_in = oracle.post_form(
            "/cgi-bin/koha/mainpage.pl",
            &[
                ("login_op", "cud-login"),
                ("csrf_token", &token),
                ("login_userid", &var("ATRIUM_ORACLE_USER")?),
                ("login_password", &var("ATRIUM_ORACLE_PASSWORD")?),
            ],
        )?;
        if !signed_in.contains("logout.x=1") {
            return Err("staff sign-in failed".into());
        }
        Ok(oracle)
    }

    /// Koha's version from the staff interface's generator tag, e.g. `26.05.05`.
    pub(crate) fn koha_version(&self) -> Result<String, Error> {
        let page = self.get("/cgi-bin/koha/mainpage.pl")?;
        let tag = between(&page, "<meta name=\"generator\" content=\"Koha ", "\"")
            .ok_or("no generator tag")?;
        Ok(tag.to_owned())
    }

    /// The one patron of the sanitized instance; refuses if there are others.
    pub(crate) fn example_patron(&self, category: &str) -> Result<i64, Error> {
        let patrons = self.api_get("/api/v1/patrons?_per_page=2")?;
        let [patron] = patrons.as_array().map(Vec::as_slice).unwrap_or_default() else {
            return Err(
                "the oracle must have exactly one patron (the authorised example account)".into(),
            );
        };
        if patron.get("category_id").and_then(Value::as_str) != Some(category) {
            return Err(format!("the example patron is not in category {category}").into());
        }
        patron
            .get("patron_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| "no patron_id".into())
    }

    /// An item of `itemtype` at `branch` that can be lent now.
    pub(crate) fn available_item(&self, itemtype: &str, branch: &str) -> Result<Item, Error> {
        let query = format!(
            "/api/v1/items?_per_page=50&q={}",
            percent_encode(&format!(
                r#"{{"item_type_id":"{itemtype}","home_library_id":"{branch}","withdrawn":0,"lost_status":0,"not_for_loan_status":0,"damaged_status":0,"checked_out_date":null}}"#
            ))
        );
        let items = self.api_get(&query)?;
        let item = items
            .as_array()
            .and_then(|items| items.first())
            .ok_or_else(|| format!("no available {itemtype} item at {branch}"))?;
        Ok(Item {
            id: item
                .get("item_id")
                .and_then(Value::as_i64)
                .ok_or("no item_id")?,
            barcode: item
                .get("external_id")
                .and_then(Value::as_str)
                .ok_or("no barcode")?
                .to_owned(),
        })
    }

    /// The rule, preferences and calendar that `CalcDateDue` will read.
    pub(crate) fn inputs(
        &self,
        branch: &str,
        category: &str,
        itemtype: &str,
    ) -> Result<RecordedInputs, Error> {
        let rules = self.api_get(&format!(
            "/api/v1/circulation_rules?library_id={branch}&patron_category_id={category}&item_type_id={itemtype}&rules=issuelength,lengthunit,daysmode,hardduedate"
        ))?;
        let rule = rules
            .as_array()
            .and_then(|r| r.first())
            .ok_or("no effective rule")?;
        let text = |name: &str| rule.get(name).and_then(Value::as_str).map(str::to_owned);
        Ok(RecordedInputs {
            rule: RuleSnapshot {
                issuelength: text("issuelength"),
                lengthunit: text("lengthunit"),
                daysmode: text("daysmode"),
                hardduedate: text("hardduedate"),
            },
            use_days_mode: self.preference("useDaysMode")?,
            return_before_expiry: self.preference("ReturnBeforeExpiry")?,
            consider_library_hours: self.preference("ConsiderLibraryHoursInCirculation")?,
            calendar: self.calendar(branch)?,
        })
    }

    /// Checks the item out, reads the loan, and checks it in again.
    pub(crate) fn checkout_and_return(
        &self,
        patron: i64,
        item: &Item,
    ) -> Result<LoanRecorded, Error> {
        let body = serde_json::json!({ "patron_id": patron, "item_id": item.id }).to_string();
        let created = self.request("POST", "/api/v1/checkouts", Body::Json(&body))?;
        let checkout: Value = serde_json::from_str(&created)?;
        let field = |name: &str| {
            checkout
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("checkout failed: {created}"))
        };
        let loan = LoanRecorded {
            checkout_date: field("checkout_date")?,
            due_date: field("due_date")?,
        };
        let page = self.get("/cgi-bin/koha/circ/returns.pl")?;
        let token = csrf_token(&page).ok_or("no CSRF token on the check-in page")?;
        self.post_form(
            "/cgi-bin/koha/circ/returns.pl",
            &[
                ("op", "cud-checkin"),
                ("csrf_token", &token),
                ("barcode", &item.barcode),
            ],
        )?;
        let open = self.api_get(&format!(
            "/api/v1/checkouts?q={}",
            percent_encode(&format!(r#"{{"item_id":{}}}"#, item.id))
        ))?;
        if open.as_array().is_none_or(|loans| !loans.is_empty()) {
            return Err(format!(
                "item {} is still checked out; check it in by hand",
                item.barcode
            )
            .into());
        }
        Ok(loan)
    }

    fn preference(&self, name: &str) -> Result<String, Error> {
        let page = self.get(&format!(
            "/cgi-bin/koha/admin/preferences.pl?op=search&searchfield={name}"
        ))?;
        let select = between(&page, &format!("<select name=\"pref_{name}\""), "</select>")
            .ok_or_else(|| format!("preference {name} not found"))?;
        let selected = select
            .split("<option ")
            .find(|option| option.contains("selected=\"selected\""))
            .and_then(|option| between(option, "value=\"", "\""))
            .ok_or_else(|| format!("preference {name} has no selected value"))?;
        Ok(selected.to_owned())
    }

    /// Reads the staff calendar screen, which lists every entry in JavaScript arrays.
    fn calendar(&self, branch: &str) -> Result<CalendarSnapshot, Error> {
        let page = self.get(&format!("/cgi-bin/koha/tools/holidays.pl?branch={branch}"))?;
        // `holidays["` also matches inside `day_month_holidays["`, so the name must not
        // follow another identifier character.
        let keys = |array: &str| -> Vec<String> {
            let needle = format!("{array}[\"");
            page.match_indices(&needle)
                .filter(|(at, _)| {
                    page.get(..*at)
                        .and_then(|before| before.chars().next_back())
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
                })
                .filter_map(|(at, _)| page.get(at + needle.len()..)?.split('"').next())
                .map(str::to_owned)
                .collect()
        };
        let mut weekly_closed = keys("week_days")
            .iter()
            .map(|d| d.parse::<u8>())
            .collect::<Result<Vec<_>, _>>()?;
        weekly_closed.sort_unstable();
        let mut annual = keys("day_month_holidays")
            .iter()
            .map(|key| {
                let (month, day) = key.split_once('/').ok_or("bad day_month key")?;
                Ok::<_, Error>([month.parse()?, day.parse()?])
            })
            .collect::<Result<Vec<_>, _>>()?;
        annual.sort_unstable();
        let dates = |array: &str| -> Result<Vec<String>, Error> {
            let mut dates = keys(array)
                .iter()
                .map(|key| {
                    let mut parts = key.split('/').map(str::parse::<u16>);
                    match (parts.next(), parts.next(), parts.next()) {
                        (Some(Ok(y)), Some(Ok(m)), Some(Ok(d))) => {
                            Ok(format!("{y:04}-{m:02}-{d:02}"))
                        }
                        _ => Err(Error::from(format!("bad date key {key}"))),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            dates.sort();
            Ok(dates)
        };
        Ok(CalendarSnapshot {
            weekly_closed,
            annual,
            closed: dates("holidays")?,
            open: dates("exception_holidays")?,
        })
    }

    fn api_get(&self, path: &str) -> Result<Value, Error> {
        Ok(serde_json::from_str(&self.get(path)?)?)
    }

    fn get(&self, path: &str) -> Result<String, Error> {
        self.request("GET", path, Body::None)
    }

    fn post_form(&self, path: &str, fields: &[(&str, &str)]) -> Result<String, Error> {
        self.request("POST", path, Body::Form(fields))
    }

    fn request(&self, method: &str, path: &str, body: Body<'_>) -> Result<String, Error> {
        let jar = self.cookies.path().join("jar");
        let jar = quote(&jar.to_string_lossy());
        let mut lines = vec![
            format!("url = {}", quote(&format!("{}{path}", self.base))),
            format!("request = {method}"),
            format!("cookie = {jar}"),
            format!("cookie-jar = {jar}"),
            "silent".to_owned(),
            "show-error".to_owned(),
            "fail-with-body".to_owned(),
        ];
        if let Some((user, password)) = &self.basic {
            lines.push(format!("user = {}", quote(&format!("{user}:{password}"))));
        }
        match body {
            Body::None => {}
            Body::Json(json) => {
                lines.push("header = \"Content-Type: application/json\"".to_owned());
                lines.push(format!("data-raw = {}", quote(json)));
            }
            Body::Form(fields) => {
                for (name, value) in fields {
                    lines.push(format!(
                        "data-urlencode = {}",
                        quote(&format!("{name}={value}"))
                    ));
                }
            }
        }
        let config = lines.join("\n") + "\n";
        let mut curl = Command::new("curl")
            .args(["--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        curl.stdin
            .take()
            .ok_or("no stdin")?
            .write_all(config.as_bytes())?;
        let output = curl.wait_with_output()?;
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            return Err(format!(
                "{method} {path} failed: {}{text}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(text)
    }
}

#[derive(Clone, Copy)]
enum Body<'a> {
    None,
    Json(&'a str),
    Form(&'a [(&'a str, &'a str)]),
}

/// A curl config string: double-quoted with `\` and `"` escaped.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn csrf_token(page: &str) -> Option<String> {
    between(page, "name=\"csrf_token\" value=\"", "\"").map(str::to_owned)
}

/// The text after the first `start` and before the next `end`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(start)?;
    rest.split_once(end).map(|(inside, _)| inside)
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
