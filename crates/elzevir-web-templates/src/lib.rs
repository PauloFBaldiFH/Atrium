// SPDX-License-Identifier: GPL-3.0-or-later
//! Askama templates for the OPAC and the staff interface, kept in their own crate to bound rebuild time.
//!
//! Templates never contain interface text: every string comes from Fluent through [`I18n`]
//! (English first, then Brazilian Portuguese; docs/arquitetura/04-decisoes.md, §1 item 7).

mod staff_catalog;

pub use staff_catalog::{FamilyLine, InstanceLine, PillLine, StaffCatalog, WorkLine};

use askama::Template;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentResource, FluentValue};
use unic_langid::{LanguageIdentifier, langid};

/// English messages, compiled into the binary.
const EN_MESSAGES: &str = include_str!("../locales/en/main.ftl");

/// Interface texts for one language.
pub struct I18n {
    lang: LanguageIdentifier,
    bundle: FluentBundle<FluentResource>,
}

/// The message files could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid Fluent messages for {lang}: {detail}")]
pub struct I18nError {
    lang: String,
    detail: String,
}

impl I18n {
    /// English, the primary language.
    ///
    /// # Errors
    ///
    /// Fails only if the embedded `.ftl` file is invalid, which the tests rule out.
    pub fn english() -> Result<Self, I18nError> {
        Self::load(langid!("en"), EN_MESSAGES)
    }

    fn load(lang: LanguageIdentifier, source: &str) -> Result<Self, I18nError> {
        let error = |detail: String| I18nError {
            lang: lang.to_string(),
            detail,
        };
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, errors)| error(format!("{errors:?}")))?;
        let mut bundle = FluentBundle::new_concurrent(vec![lang.clone()]);
        // Unicode isolation marks around placeables would end up inside HTML attributes.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .map_err(|errors| error(format!("{errors:?}")))?;
        Ok(Self { lang, bundle })
    }

    /// The language tag, for `<html lang>`.
    #[must_use]
    pub fn lang(&self) -> String {
        self.lang.to_string()
    }

    /// The message for `key`. A missing key renders as the key itself, so a gap shows up
    /// on screen instead of failing the page; the tests make sure none are missing.
    #[must_use]
    pub fn get(&self, key: &str) -> String {
        self.format(key, None)
    }

    fn format(&self, key: &str, args: Option<&fluent_bundle::FluentArgs<'_>>) -> String {
        let Some(pattern) = self.bundle.get_message(key).and_then(|m| m.value()) else {
            return key.to_owned();
        };
        let mut errors = Vec::new();
        self.bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned()
    }

    /// Whether `key` exists.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.bundle.has_message(key)
    }

    /// Formats `key` with one named argument.
    #[must_use]
    pub fn get_with(&self, key: &str, name: &str, value: &str) -> String {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set(name, FluentValue::from(value));
        self.format(key, Some(&args))
    }

    /// Formats `key` with named numbers, so that plural forms are chosen by the language.
    #[must_use]
    pub fn get_numbers(&self, key: &str, numbers: &[(&str, u64)]) -> String {
        let mut args = fluent_bundle::FluentArgs::new();
        for (name, value) in numbers {
            args.set(*name, FluentValue::from(*value));
        }
        self.format(key, Some(&args))
    }
}

/// OPAC home page.
#[derive(Template)]
#[template(path = "opac_home.html")]
pub struct OpacHome<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Text of the search box (empty on the home page).
    pub query: &'a str,
}

/// One search result as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultLine {
    /// Instance id, for the link to the record.
    pub instance: i64,
    /// Title.
    pub title: String,
    /// Main contributor.
    pub contributor: Option<String>,
    /// Date of publication.
    pub date: Option<String>,
    /// Message key of the category of material.
    pub material: Option<&'static str>,
    /// Call number of the first Item.
    pub call_number: Option<String>,
    /// Items of the Instance.
    pub items: u32,
    /// Items available.
    pub available: u32,
}

impl ResultLine {
    /// CSS class of the availability line.
    #[must_use]
    pub const fn availability_class(&self) -> &'static str {
        if self.available > 0 {
            "available"
        } else {
            "unavailable"
        }
    }

    /// Availability, as a sentence.
    #[must_use]
    pub fn availability_text(&self, t: &I18n) -> String {
        if self.items == 0 {
            t.get("opac-availability-no-items")
        } else if self.available == 0 {
            t.get("opac-availability-none")
        } else {
            t.get_numbers(
                "opac-availability-available",
                &[
                    ("available", u64::from(self.available)),
                    ("items", u64::from(self.items)),
                ],
            )
        }
    }
}

/// The list of search results with its page links: a whole page, or only the fragment
/// that HTMX swaps in.
#[derive(Template)]
#[template(path = "opac_results.html", blocks = ["results"])]
pub struct OpacResults<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// The query as used.
    pub query: &'a str,
    /// Matching records in all.
    pub total: usize,
    /// This page, from 1.
    pub page: usize,
    /// Whether a next page exists.
    pub has_next: bool,
    /// The results of this page.
    pub rows: Vec<ResultLine>,
}

impl OpacResults<'_> {
    /// How many records matched, as a sentence.
    #[must_use]
    pub fn count_text(&self) -> String {
        self.t.get_numbers(
            "opac-results-count",
            &[("count", u64::try_from(self.total).unwrap_or(u64::MAX))],
        )
    }

    /// The query, encoded for a URL.
    #[must_use]
    pub fn query_url(&self) -> String {
        url_encode(self.query)
    }
}

/// One Item as the record shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemLine {
    /// Home branch.
    pub branch: String,
    /// Shelving location.
    pub location: Option<String>,
    /// Call number.
    pub call_number: Option<String>,
    /// Whether it is available.
    pub available: bool,
    /// Message key of its status.
    pub status: &'static str,
}

/// The body of a record page. It depends only on the record, so it is rendered once per
/// record version and kept (docs/specs/opac/busca-e-registro.md).
#[derive(Template)]
#[template(path = "opac_record_body.html")]
pub struct OpacRecordBody<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Title.
    pub title: &'a str,
    /// The Work's title, when it differs from the Instance's.
    pub work_title: Option<&'a str>,
    /// Contributor headings.
    pub contributors: Vec<&'a str>,
    /// Message key of the category of material.
    pub material: Option<&'static str>,
    /// Publisher and date.
    pub publication: Option<String>,
    /// Classification number.
    pub classification: Option<&'a str>,
    /// Subject headings.
    pub subjects: Vec<&'a str>,
    /// Identifiers.
    pub identifiers: Vec<&'a str>,
    /// Language codes.
    pub languages: Vec<&'a str>,
    /// Summary.
    pub summary: Option<&'a str>,
    /// Target audience.
    pub audience: Option<&'a str>,
    /// General and local notes.
    pub notes: Vec<&'a str>,
    /// Contents notes.
    pub contents: Vec<&'a str>,
    /// Items.
    pub items: Vec<ItemLine>,
}

/// A record page around an already rendered body.
#[derive(Template)]
#[template(path = "opac_record.html")]
pub struct OpacRecord<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Title, for `<title>`.
    pub title: &'a str,
    /// The rendered [`OpacRecordBody`].
    pub body: &'a str,
}

/// The page for a record that does not exist.
#[derive(Template)]
#[template(path = "opac_not_found.html")]
pub struct OpacNotFound<'a> {
    /// Interface texts.
    pub t: &'a I18n,
}

/// Percent-encodes `text` for a query string: unreserved characters stay, everything else
/// becomes `%XX` per UTF-8 byte.
#[must_use]
pub fn url_encode(text: &str) -> String {
    let hex = |nibble: u8| char::from_digit(u32::from(nibble), 16).map(|c| c.to_ascii_uppercase());
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            // Both nibbles are below 16, so `from_digit` always gives a character.
            encoded.extend(hex(byte >> 4));
            encoded.extend(hex(byte & 0x0f));
        }
    }
    encoded
}

/// Staff sign-in page.
#[derive(Template)]
#[template(path = "staff_login.html")]
pub struct StaffLogin<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Anti-forgery token echoed in the form.
    pub csrf: &'a str,
    /// Login typed before a failed attempt, to refill the field.
    pub login: &'a str,
    /// Message key of the error to show, if any.
    pub error: Option<&'static str>,
}

/// Staff home page.
#[derive(Template)]
#[template(path = "staff_home.html")]
pub struct StaffHome<'a> {
    /// Interface texts.
    pub t: &'a I18n,
    /// Signed-in login.
    pub login: &'a str,
    /// Whether to show administration links.
    pub is_admin: bool,
    /// Anti-forgery token for the sign-out form.
    pub csrf: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `t.get("...")` key in every template exists in the English messages.
    #[test]
    fn every_template_key_exists() {
        let t = I18n::english().unwrap();
        let templates = [
            include_str!("../templates/base.html"),
            include_str!("../templates/opac_home.html"),
            include_str!("../templates/opac_search_form.html"),
            include_str!("../templates/opac_results.html"),
            include_str!("../templates/opac_results_list.html"),
            include_str!("../templates/opac_record.html"),
            include_str!("../templates/opac_record_body.html"),
            include_str!("../templates/opac_not_found.html"),
            include_str!("../templates/staff_login.html"),
            include_str!("../templates/staff_home.html"),
            include_str!("../templates/staff_catalog.html"),
        ];
        let mut missing = Vec::new();
        for source in templates {
            for chunk in source.split("t.get(\"").skip(1) {
                let key = chunk.split('"').next().unwrap();
                if !t.has(key) {
                    missing.push(key.to_owned());
                }
            }
        }
        assert!(missing.is_empty(), "missing Fluent keys: {missing:?}");
    }

    #[test]
    fn error_keys_used_by_the_web_layer_exist() {
        let t = I18n::english().unwrap();
        for key in [
            "staff-login-invalid",
            "staff-login-not-staff",
            "staff-login-unavailable",
        ] {
            assert!(t.has(key), "{key}");
        }
    }

    #[test]
    fn pages_render_escaped_html() {
        let t = I18n::english().unwrap();
        let page = StaffLogin {
            t: &t,
            csrf: "token",
            login: "<script>",
            error: Some("staff-login-invalid"),
        }
        .render()
        .unwrap();
        assert!(page.contains(r#"<html lang="en">"#));
        assert!(page.contains("Invalid username or password."));
        assert!(page.contains("&#60;script&#62;") || page.contains("&lt;script&gt;"));
        assert!(!page.contains("<script>\""));
    }

    #[test]
    fn queries_are_encoded_for_urls() {
        assert_eq!(url_encode("café & pão"), "caf%C3%A9%20%26%20p%C3%A3o");
        assert_eq!(url_encode("310.62 P94"), "310.62%20P94");
        assert_eq!(url_encode("a-b_c~d"), "a-b_c~d");
    }

    #[test]
    fn plurals_follow_the_number() {
        let t = I18n::english().unwrap();
        assert_eq!(
            t.get_numbers("opac-results-count", &[("count", 1)]),
            "1 result"
        );
        assert_eq!(
            t.get_numbers("opac-results-count", &[("count", 2)]),
            "2 results"
        );
    }

    #[test]
    fn missing_keys_render_as_the_key() {
        let t = I18n::english().unwrap();
        assert_eq!(t.get("no-such-key"), "no-such-key");
    }
}
