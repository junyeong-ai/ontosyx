//! `VariableName` — a validated newtype wrapping a Cypher variable
//! name (the `n` in `MATCH (n:Person)`, the `c` in `RETURN c.name`).
//!
//! The same structural gap [`crate::graph_label::GraphLabel`] closes
//! for node/edge labels applies to variable names, with a different
//! rule set. A Cypher variable must:
//!
//! - be non-empty
//! - start with a letter (any Unicode alphabetic) or `_`
//! - not be the literal `"null"` or `"-"` — those are LLM-hallucination
//!   outputs that [`crate::types::sanitize_variable`] already drops
//!
//! Without a type, that check lives at every call site: the
//! `StructuredMatchQuery` converter calls `sanitize_variable`, the
//! Cypher pattern compiler checks again, and the rewriter trusts its
//! inputs. `VariableName` pins the rule once, so a type-level
//! guarantee replaces three defensive checks.
//!
//! Construction is fallible-only: no `From<&str>` / `From<String>`
//! impl. A caller without a recovery plan for an invalid name is
//! lying about their input; the type refuses to pretend.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{OxError, OxResult};
use crate::types::sanitize_variable;

/// A validated Cypher variable name. See the module docs for the rule.
///
/// Displayed and compared as the wrapped string; hashes as the string
/// too, so `HashMap<VariableName, _>` is interchangeable with a
/// previous `HashMap<String, _>` at lookup time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[schemars(transparent)]
pub struct VariableName(String);

impl VariableName {
    /// Canonical fallible constructor.
    ///
    /// Returns `OxError::Validation` for any input that
    /// `sanitize_variable` rejects — empty, `"null"`, `"-"`, or not
    /// starting with an alphabetic / `_` character.
    pub fn new(s: impl Into<String>) -> OxResult<Self> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Alias for [`FromStr::from_str`] that reads like a parser entry
    /// point when used with the `?` operator.
    pub fn parse(s: &str) -> OxResult<Self> {
        Self::new(s)
    }

    /// `true` when `s` would be accepted by [`Self::new`].
    pub fn is_valid(s: &str) -> bool {
        sanitize_variable(s).is_some()
    }

    fn validate(s: &str) -> OxResult<()> {
        if sanitize_variable(s).is_none() {
            return Err(OxError::Validation {
                field: "variable_name".to_string(),
                message: format!(
                    "invalid variable name `{s}` — must start with an alphabetic \
                     character or `_`, and must not be empty, `null`, or `-`"
                ),
            });
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for VariableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for VariableName {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for VariableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for VariableName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for VariableName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for VariableName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for VariableName {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl FromStr for VariableName {
    type Err = OxError;

    fn from_str(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for VariableName {
    type Error = OxError;

    fn try_from(s: String) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<&str> for VariableName {
    type Error = OxError;

    fn try_from(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl Serialize for VariableName {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for VariableName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_simple_ascii_name() {
        assert!(VariableName::new("n").is_ok());
        assert!(VariableName::new("customer").is_ok());
        assert!(VariableName::new("c42").is_ok());
    }

    #[test]
    fn new_accepts_leading_underscore() {
        assert!(VariableName::new("_var").is_ok());
        assert!(VariableName::new("__system").is_ok());
    }

    #[test]
    fn new_accepts_unicode_alphabetic() {
        // Korean / CJK letters are `is_alphabetic`, so they're valid.
        assert!(VariableName::new("고객").is_ok());
        assert!(VariableName::new("α").is_ok());
    }

    #[test]
    fn new_rejects_empty_and_llm_hallucinations() {
        // The same inputs `sanitize_variable` drops — the exact source
        // of truth we want to pin at the type level.
        assert!(VariableName::new("").is_err());
        assert!(VariableName::new("null").is_err());
        assert!(VariableName::new("-").is_err());
    }

    #[test]
    fn new_rejects_names_starting_with_digit() {
        // Cypher forbids `2nd` / `1var` — enforce the same rule here.
        assert!(VariableName::new("2nd").is_err());
        assert!(VariableName::new("1var").is_err());
    }

    #[test]
    fn new_rejects_leading_punctuation() {
        assert!(VariableName::new("-x").is_err());
        assert!(VariableName::new(".field").is_err());
    }

    #[test]
    fn is_valid_matches_new() {
        assert!(VariableName::is_valid("n"));
        assert!(!VariableName::is_valid(""));
        assert!(!VariableName::is_valid("null"));
    }

    #[test]
    fn fromstr_matches_new() {
        let a: VariableName = "customer".parse().unwrap();
        assert_eq!(a.as_str(), "customer");
        let bad: Result<VariableName, _> = "null".parse();
        assert!(bad.is_err());
    }

    #[test]
    fn deref_and_display() {
        let v = VariableName::new("customer").unwrap();
        assert_eq!(format!("{v}"), "customer");
        assert_eq!(&*v, "customer");
    }

    #[test]
    fn serde_roundtrip_transparent() {
        let v = VariableName::new("c").unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"c\"");
        let back: VariableName = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn deserialize_rejects_invalid_input() {
        let bad: Result<VariableName, _> = serde_json::from_str("\"null\"");
        assert!(bad.is_err());
        let empty: Result<VariableName, _> = serde_json::from_str("\"\"");
        assert!(empty.is_err());
    }

    #[test]
    fn schema_is_transparent_string() {
        let schema = schemars::schema_for!(VariableName);
        let value = serde_json::to_value(&schema).unwrap();
        assert_eq!(value.get("type").and_then(|v| v.as_str()), Some("string"));
    }

    #[test]
    fn hashes_as_wrapped_string() {
        use std::collections::HashMap;
        let mut map: HashMap<VariableName, u32> = HashMap::new();
        map.insert(VariableName::new("c").unwrap(), 1);
        assert_eq!(map.get("c"), Some(&1));
    }
}
