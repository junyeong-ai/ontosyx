//! Locale primitives for language-tagged text across the ontology IR.
//!
//! Two types live here:
//!
//! - [`LanguageTag`] — a canonically normalised BCP 47 language tag.
//! - [`LocalizedText`] — a required canonical `default` plus optional
//!   per-locale overrides, modelled after `rdfs:label` + `@lang`.
//!
//! The design guarantees that every `LocalizedText` has a displayable string,
//! so call sites never need defensive fallback logic beyond walking a
//! user-supplied fallback chain.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors raised when parsing or resolving locale information.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LocaleError {
    #[error("invalid BCP 47 language tag: {0:?}")]
    InvalidTag(String),
}

// ---------------------------------------------------------------------------
// LanguageTag — BCP 47 subset, canonical lowercase
// ---------------------------------------------------------------------------

/// A BCP 47 language tag stored in canonical lowercase form.
///
/// Accepted syntax (a pragmatic BCP 47 subset sufficient for UI/ontology
/// locale selection):
///
/// - A primary subtag of 2 or 3 ASCII letters (`ko`, `en`, `zho`).
/// - Zero or more additional subtags of 2–8 ASCII alphanumerics,
///   separated by `-` (script, region, variant).
///
/// Examples: `ko`, `en`, `en-us`, `zh-hant`, `zh-hant-tw`.
///
/// The constructor [`LanguageTag::parse`] lowercases the input so equality
/// and hashing are case-insensitive. Use [`LanguageTag::primary`] to access
/// the language-only part when subtag-aware matching is not required.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LanguageTag(String);

impl LanguageTag {
    /// Parse a raw tag. Trims surrounding whitespace, lowercases, and
    /// validates each subtag.
    pub fn parse(raw: &str) -> Result<Self, LocaleError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LocaleError::InvalidTag(raw.to_string()));
        }

        let lowered = trimmed.to_ascii_lowercase();
        let mut parts = lowered.split('-');

        let primary = parts
            .next()
            .ok_or_else(|| LocaleError::InvalidTag(raw.to_string()))?;
        let primary_len = primary.len();
        if !(2..=3).contains(&primary_len)
            || !primary.chars().all(|c| c.is_ascii_alphabetic())
        {
            return Err(LocaleError::InvalidTag(raw.to_string()));
        }

        for sub in parts {
            let sub_len = sub.len();
            if !(2..=8).contains(&sub_len)
                || !sub.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return Err(LocaleError::InvalidTag(raw.to_string()));
            }
        }

        Ok(Self(lowered))
    }

    /// Canonical lowercase form (e.g. `zh-hant-tw`).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The primary language subtag only (`"zh"` for `zh-hant-tw`).
    pub fn primary(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for LanguageTag {
    type Err = LocaleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for LanguageTag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for LanguageTag {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LanguageTag {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for LanguageTag {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LanguageTag".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> Schema {
        let value = serde_json::json!({
            "type": "string",
            "pattern": "^[a-z]{2,3}(-[a-z0-9]{2,8})*$",
            "description": "BCP 47 language tag, canonical lowercase (e.g. 'ko', 'en-us', 'zh-hant')."
        });
        let serde_json::Value::Object(map) = value else {
            return Schema::default();
        };
        Schema::from(map)
    }
}

// ---------------------------------------------------------------------------
// LocalizedText — canonical default + optional translations
// ---------------------------------------------------------------------------

/// Language-tagged text with a canonical `default` and optional per-locale
/// `translations`.
///
/// Semantics:
///
/// - `default` is the value authored by whoever created the ontology, in
///   whichever language they chose. It is always present (possibly empty for
///   truly optional fields) so every consumer can display *something*.
/// - `translations` holds per-locale overrides keyed by [`LanguageTag`].
///
/// Consumers call [`LocalizedText::resolve`] with a caller-controlled
/// fallback chain (usually derived from the user's preference plus the
/// workspace's configured fallback). The result is always a displayable
/// `&str` — the chain is walked in order, non-empty matches win, and
/// `default` is used as the final fallback.
///
/// ## Wire format
///
/// Serialization always emits the canonical `{"default": "…", "translations":
/// {…}}` form (with `translations` omitted when empty).
///
/// Deserialization is **lenient** and accepts any of:
///
/// - `null` → [`LocalizedText::empty`]
/// - bare string `"text"` → `{default: "text", translations: {}}`
/// - canonical object `{"default": "…", "translations": {…}}`
///
/// The lenient form makes LLM structured output comfortable (LLMs happily
/// emit bare strings) without weakening the canonical serialization contract.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, JsonSchema)]
pub struct LocalizedText {
    pub default: String,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub translations: HashMap<LanguageTag, String>,
}

impl<'de> Deserialize<'de> for LocalizedText {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        match value {
            serde_json::Value::Null => Ok(Self::empty()),
            serde_json::Value::String(s) => Ok(Self::new(s)),
            serde_json::Value::Object(_) => {
                #[derive(Deserialize)]
                struct Canonical {
                    #[serde(default)]
                    default: String,
                    #[serde(default)]
                    translations: HashMap<LanguageTag, String>,
                }
                let c: Canonical =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self {
                    default: c.default,
                    translations: c.translations,
                })
            }
            other => Err(serde::de::Error::custom(format!(
                "LocalizedText: expected null, string, or object; got {}",
                match other {
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::Array(_) => "array",
                    _ => "unknown",
                }
            ))),
        }
    }
}

impl LocalizedText {
    /// Create a new localized text with only a canonical default.
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            default: default.into(),
            translations: HashMap::new(),
        }
    }

    /// Empty localized text — suitable for optional description fields that
    /// the designer chose not to fill in. Prefer [`LocalizedText::new`] for
    /// user-facing fields that require a value.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when neither `default` nor any translation carries visible text.
    pub fn is_empty(&self) -> bool {
        self.default.is_empty() && self.translations.values().all(|v| v.is_empty())
    }

    /// Builder-style: attach or overwrite a translation for `tag`.
    pub fn with_translation(mut self, tag: LanguageTag, text: impl Into<String>) -> Self {
        self.translations.insert(tag, text.into());
        self
    }

    /// Resolve for display using a fallback chain.
    ///
    /// Walks `chain` in order; the first translation that exists *and* is
    /// non-empty wins. If no translation matches, falls back to `default`
    /// (which itself may be an empty string for optional fields).
    pub fn resolve(&self, chain: &[LanguageTag]) -> &str {
        for tag in chain {
            if let Some(v) = self.translations.get(tag)
                && !v.is_empty()
            {
                return v;
            }
        }
        &self.default
    }

    /// Canonical default as `&str`.
    pub fn default_str(&self) -> &str {
        &self.default
    }

    /// Shorthand for the canonical default. Equivalent to
    /// [`LocalizedText::default_str`]; provided for ergonomic migration from
    /// code that used `Option<String>::as_deref().unwrap_or("")`.
    pub fn as_str(&self) -> &str {
        &self.default
    }

    /// Returns the canonical default as `Some(&str)` if non-empty,
    /// otherwise `None`. Mirrors the shape of
    /// `Option<String>::as_deref()` for call sites migrating from the
    /// old optional-description design.
    pub fn present(&self) -> Option<&str> {
        if self.default.is_empty() {
            None
        } else {
            Some(&self.default)
        }
    }

    /// Return the default when non-empty, otherwise the provided fallback.
    /// Convenience for the common pattern `description.as_deref().unwrap_or(&fallback)`.
    pub fn default_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.default.is_empty() {
            fallback
        } else {
            &self.default
        }
    }

    /// Iterate over every stored variant.
    ///
    /// Yields `(None, default)` first, then `(Some(tag), translation)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (Option<&LanguageTag>, &str)> {
        std::iter::once((None, self.default.as_str()))
            .chain(self.translations.iter().map(|(k, v)| (Some(k), v.as_str())))
    }
}

impl From<String> for LocalizedText {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for LocalizedText {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<Option<String>> for LocalizedText {
    /// Convenience conversion for code migrating from `Option<String>`:
    /// `None` becomes an empty [`LocalizedText`], `Some(s)` becomes a
    /// canonical value.
    fn from(opt: Option<String>) -> Self {
        opt.map(Self::new).unwrap_or_default()
    }
}

impl From<Option<&str>> for LocalizedText {
    fn from(opt: Option<&str>) -> Self {
        opt.map(Self::new).unwrap_or_default()
    }
}

impl fmt::Display for LocalizedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.default.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ko() -> LanguageTag {
        LanguageTag::parse("ko").expect("ko must parse")
    }
    fn en() -> LanguageTag {
        LanguageTag::parse("en").expect("en must parse")
    }
    fn en_us() -> LanguageTag {
        LanguageTag::parse("en-US").expect("en-US must parse")
    }
    fn zh_hant() -> LanguageTag {
        LanguageTag::parse("zh-Hant").expect("zh-Hant must parse")
    }

    // -- LanguageTag parsing --------------------------------------------------

    #[test]
    fn parses_simple_two_letter_primary() {
        assert_eq!(ko().as_str(), "ko");
        assert_eq!(en().as_str(), "en");
    }

    #[test]
    fn parses_three_letter_primary() {
        let t = LanguageTag::parse("zho").unwrap();
        assert_eq!(t.as_str(), "zho");
    }

    #[test]
    fn normalises_to_lowercase() {
        assert_eq!(LanguageTag::parse("EN").unwrap().as_str(), "en");
        assert_eq!(en_us().as_str(), "en-us");
        assert_eq!(zh_hant().as_str(), "zh-hant");
        assert_eq!(
            LanguageTag::parse("zh-Hant-TW").unwrap().as_str(),
            "zh-hant-tw"
        );
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(LanguageTag::parse("  ko  ").unwrap().as_str(), "ko");
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            LanguageTag::parse(""),
            Err(LocaleError::InvalidTag(_))
        ));
        assert!(matches!(
            LanguageTag::parse("   "),
            Err(LocaleError::InvalidTag(_))
        ));
    }

    #[test]
    fn rejects_too_short_or_too_long_primary() {
        assert!(LanguageTag::parse("k").is_err());
        assert!(LanguageTag::parse("koreaan").is_err());
    }

    #[test]
    fn rejects_non_alpha_primary() {
        assert!(LanguageTag::parse("k1").is_err());
        assert!(LanguageTag::parse("12").is_err());
    }

    #[test]
    fn rejects_invalid_subtag() {
        assert!(LanguageTag::parse("ko-").is_err());
        assert!(LanguageTag::parse("ko-a").is_err());
        assert!(LanguageTag::parse("ko-toolongsubtag").is_err());
        assert!(LanguageTag::parse("ko-!!").is_err());
    }

    #[test]
    fn primary_subtag_extraction() {
        assert_eq!(ko().primary(), "ko");
        assert_eq!(en_us().primary(), "en");
        assert_eq!(zh_hant().primary(), "zh");
    }

    #[test]
    fn language_tag_serde_round_trip() {
        let t = zh_hant();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"zh-hant\"");
        let back: LanguageTag = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn language_tag_deserialize_rejects_invalid() {
        let err = serde_json::from_str::<LanguageTag>("\"k1\"").unwrap_err();
        assert!(err.to_string().contains("invalid BCP 47"));
    }

    #[test]
    fn language_tag_equality_is_case_insensitive() {
        assert_eq!(
            LanguageTag::parse("EN").unwrap(),
            LanguageTag::parse("en").unwrap()
        );
        assert_eq!(
            LanguageTag::parse("Zh-HANT").unwrap(),
            LanguageTag::parse("zh-hant").unwrap()
        );
    }

    // -- LocalizedText basics -------------------------------------------------

    #[test]
    fn localized_text_new_stores_default() {
        let t = LocalizedText::new("고객");
        assert_eq!(t.default, "고객");
        assert!(t.translations.is_empty());
    }

    #[test]
    fn localized_text_empty_is_empty() {
        let t = LocalizedText::empty();
        assert!(t.is_empty());
        assert_eq!(t.default_str(), "");
    }

    #[test]
    fn localized_text_with_translation_adds_and_replaces() {
        let t = LocalizedText::new("고객")
            .with_translation(en(), "Customer")
            .with_translation(en(), "Client");
        assert_eq!(t.translations.get(&en()), Some(&"Client".to_string()));
        assert_eq!(t.translations.len(), 1);
    }

    #[test]
    fn localized_text_resolve_returns_translation_when_present() {
        let t = LocalizedText::new("고객").with_translation(en(), "Customer");
        assert_eq!(t.resolve(&[en()]), "Customer");
    }

    #[test]
    fn localized_text_resolve_walks_fallback_chain() {
        let t = LocalizedText::new("고객").with_translation(en(), "Customer");
        // chain: zh-hant → en; only en exists, so en wins
        assert_eq!(t.resolve(&[zh_hant(), en()]), "Customer");
    }

    #[test]
    fn localized_text_resolve_falls_back_to_default() {
        let t = LocalizedText::new("고객");
        assert_eq!(t.resolve(&[en(), zh_hant()]), "고객");
        assert_eq!(t.resolve(&[]), "고객");
    }

    #[test]
    fn localized_text_resolve_skips_empty_translation() {
        let t = LocalizedText::new("고객").with_translation(en(), "");
        // en exists but is empty → falls through to default
        assert_eq!(t.resolve(&[en()]), "고객");
    }

    #[test]
    fn localized_text_is_empty_detects_all_empty() {
        let t = LocalizedText::new("").with_translation(en(), "");
        assert!(t.is_empty());
    }

    #[test]
    fn localized_text_is_not_empty_when_any_has_value() {
        let t1 = LocalizedText::new("x");
        let t2 = LocalizedText::new("").with_translation(en(), "x");
        assert!(!t1.is_empty());
        assert!(!t2.is_empty());
    }

    #[test]
    fn localized_text_iter_yields_default_then_translations() {
        let t = LocalizedText::new("고객")
            .with_translation(en(), "Customer")
            .with_translation(zh_hant(), "客戶");
        let items: Vec<_> = t.iter().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], (None, "고객")); // default is always first
    }

    #[test]
    fn localized_text_serde_round_trip_with_translations() {
        let t = LocalizedText::new("고객")
            .with_translation(en(), "Customer")
            .with_translation(zh_hant(), "客戶");
        let json = serde_json::to_string(&t).unwrap();
        let back: LocalizedText = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn localized_text_serde_omits_empty_translations_map() {
        let t = LocalizedText::new("고객");
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"{"default":"고객"}"#);
    }

    #[test]
    fn localized_text_serde_accepts_missing_translations() {
        let t: LocalizedText = serde_json::from_str(r#"{"default":"고객"}"#).unwrap();
        assert_eq!(t.default, "고객");
        assert!(t.translations.is_empty());
    }

    #[test]
    fn localized_text_deserialize_accepts_bare_string() {
        let t: LocalizedText = serde_json::from_str(r#""고객""#).unwrap();
        assert_eq!(t.default, "고객");
        assert!(t.translations.is_empty());
    }

    #[test]
    fn localized_text_deserialize_accepts_null() {
        let t: LocalizedText = serde_json::from_str("null").unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn localized_text_deserialize_accepts_empty_object() {
        let t: LocalizedText = serde_json::from_str("{}").unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn localized_text_deserialize_rejects_number_and_array() {
        assert!(serde_json::from_str::<LocalizedText>("42").is_err());
        assert!(serde_json::from_str::<LocalizedText>("[]").is_err());
        assert!(serde_json::from_str::<LocalizedText>("true").is_err());
    }

    #[test]
    fn localized_text_from_str_and_string() {
        let t1: LocalizedText = "고객".into();
        let t2: LocalizedText = String::from("고객").into();
        assert_eq!(t1, t2);
        assert_eq!(t1.default, "고객");
    }

    #[test]
    fn localized_text_display_uses_default() {
        let t = LocalizedText::new("고객").with_translation(en(), "Customer");
        assert_eq!(format!("{t}"), "고객");
    }

    #[test]
    fn schema_names_are_stable() {
        assert_eq!(LanguageTag::schema_name(), "LanguageTag");
        assert_eq!(LocalizedText::schema_name(), "LocalizedText");
    }
}
