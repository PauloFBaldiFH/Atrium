// SPDX-License-Identifier: GPL-3.0-or-later
//! Askama templates for the OPAC and the staff interface, kept in their own crate to bound rebuild time.
//!
//! Templates never contain interface text: every string comes from Fluent through [`I18n`]
//! (English first, then Brazilian Portuguese; docs/arquitetura/04-decisoes.md, §1 item 7).

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
}

/// OPAC home page.
#[derive(Template)]
#[template(path = "opac_home.html")]
pub struct OpacHome<'a> {
    /// Interface texts.
    pub t: &'a I18n,
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
            include_str!("../templates/staff_login.html"),
            include_str!("../templates/staff_home.html"),
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
    fn missing_keys_render_as_the_key() {
        let t = I18n::english().unwrap();
        assert_eq!(t.get("no-such-key"), "no-such-key");
    }
}
