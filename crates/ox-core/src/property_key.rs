//! `PropertyKey` — a validated newtype wrapping a property key used
//! on a node or edge (the `name` in `(n {name: 'Alice'})`, the
//! `amount` in `SET r.amount = 100`).
//!
//! The validation rule is the same one [`crate::graph_label::GraphLabel`]
//! uses — property keys flow into Cypher identifiers the same way
//! labels do, and Neo4j backticks keys the same way it backticks
//! labels. Keeping the rule identical avoids a fourth set of
//! "what exactly is allowed here?" edge cases for reviewers.
//!
//! Today `PropertyDef.name` is `String`; the validation check lives
//! inside `OntologyIR::validate()`. A future migration of that field
//! (and the `String` keys scattered through `NodePattern.properties`
//! / `RelationshipPattern.properties`) to `PropertyKey` will make
//! invalid keys unrepresentable at the type level. Like
//! [`GraphLabel`] and [`crate::variable_name::VariableName`], this
//! commit establishes the type without rewriting the fields —
//! the migration is deferred to a dedicated sweep because it touches
//! hundreds of struct-literal call sites.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{OxError, OxResult};
use crate::types::is_valid_graph_identifier;

/// A validated property key. See the module docs for the rule.
///
/// Displayed and compared as the wrapped string; hashes as the string
/// too, so `HashMap<PropertyKey, _>` is interchangeable with a
/// previous `HashMap<String, _>` at lookup time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, JsonSchema, utoipa::ToSchema)]
#[schemars(transparent)]
#[schema(value_type = String)]
pub struct PropertyKey(String);

impl PropertyKey {
    /// Canonical fallible constructor. Rejects the empty string and
    /// any input that `is_valid_graph_identifier` (the same rule
    /// `OntologyIR::validate` applies to property names) would
    /// reject.
    pub fn new(s: impl Into<String>) -> OxResult<Self> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Alias for [`FromStr::from_str`] that reads like a parser
    /// entry point when used with the `?` operator.
    pub fn parse(s: &str) -> OxResult<Self> {
        Self::new(s)
    }

    /// `true` when `s` would be accepted by [`Self::new`].
    pub fn is_valid(s: &str) -> bool {
        is_valid_graph_identifier(s)
    }

    fn validate(s: &str) -> OxResult<()> {
        if !is_valid_graph_identifier(s) {
            return Err(OxError::Validation {
                field: "property_key".to_string(),
                message: format!(
                    "invalid property key `{s}` — must be non-empty and composed \
                     of alphanumeric (including Unicode letters/digits), `_`, \
                     or space characters"
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

impl fmt::Display for PropertyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for PropertyKey {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PropertyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for PropertyKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for PropertyKey {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for PropertyKey {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for PropertyKey {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl FromStr for PropertyKey {
    type Err = OxError;

    fn from_str(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for PropertyKey {
    type Error = OxError;

    fn try_from(s: String) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<&str> for PropertyKey {
    type Error = OxError;

    fn try_from(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl Serialize for PropertyKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for PropertyKey {
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
    fn new_accepts_simple_ascii_key() {
        assert!(PropertyKey::new("name").is_ok());
        assert!(PropertyKey::new("user_id").is_ok());
        assert!(PropertyKey::new("addr2").is_ok());
    }

    #[test]
    fn new_accepts_korean_key() {
        // Korean-first ontology: property keys flow through the same
        // Cypher-quoted identifier path as labels, so they must
        // accept the same Unicode alphabet.
        assert!(PropertyKey::new("이름").is_ok());
        assert!(PropertyKey::new("고객번호").is_ok());
    }

    #[test]
    fn new_rejects_empty_string() {
        assert!(PropertyKey::new("").is_err());
    }

    #[test]
    fn new_rejects_control_characters() {
        assert!(PropertyKey::new("line\nbreak").is_err());
        assert!(PropertyKey::new("tab\there").is_err());
    }

    #[test]
    fn new_rejects_special_characters() {
        assert!(PropertyKey::new("user-id").is_err());
        assert!(PropertyKey::new("profile.email").is_err());
        assert!(PropertyKey::new("a/b").is_err());
        assert!(PropertyKey::new("key:value").is_err());
    }

    #[test]
    fn is_valid_matches_new() {
        assert!(PropertyKey::is_valid("name"));
        assert!(!PropertyKey::is_valid(""));
        assert!(!PropertyKey::is_valid("user-id"));
    }

    #[test]
    fn fromstr_matches_new() {
        let k: PropertyKey = "name".parse().unwrap();
        assert_eq!(k.as_str(), "name");
        let bad: Result<PropertyKey, _> = "bad-key".parse();
        assert!(bad.is_err());
    }

    #[test]
    fn deref_and_display() {
        let k = PropertyKey::new("email").unwrap();
        assert_eq!(format!("{k}"), "email");
        assert_eq!(&*k, "email");
    }

    #[test]
    fn serde_roundtrip_transparent() {
        let k = PropertyKey::new("name").unwrap();
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, "\"name\"");
        let back: PropertyKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn deserialize_rejects_invalid_input() {
        let bad: Result<PropertyKey, _> = serde_json::from_str("\"bad-key\"");
        assert!(bad.is_err());
        let empty: Result<PropertyKey, _> = serde_json::from_str("\"\"");
        assert!(empty.is_err());
    }

    #[test]
    fn schema_is_transparent_string() {
        let schema = schemars::schema_for!(PropertyKey);
        let value = serde_json::to_value(&schema).unwrap();
        assert_eq!(value.get("type").and_then(|v| v.as_str()), Some("string"));
    }

    #[test]
    fn hashes_as_wrapped_string() {
        use std::collections::HashMap;
        let mut map: HashMap<PropertyKey, u32> = HashMap::new();
        map.insert(PropertyKey::new("name").unwrap(), 1);
        assert_eq!(map.get("name"), Some(&1));
    }
}
