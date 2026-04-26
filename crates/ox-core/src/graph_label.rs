//! `GraphLabel` — a validated newtype wrapping a node-type or
//! edge-type identifier.
//!
//! Every label in an `OntologyIR` must satisfy
//! [`crate::types::is_valid_graph_identifier`] — non-empty, composed
//! only of characters that backticks can safely quote in Cypher.
//! Today that invariant is enforced as a runtime validation pass
//! inside [`crate::ontology_ir::OntologyIR::validate`]; nothing in the
//! type system stops a caller from constructing a `NodeTypeDef` with a
//! `String` that contains a newline or a control character until the
//! validator runs.
//!
//! `GraphLabel` exists to close that gap progressively. A future
//! migration of `NodeTypeDef.label` / `EdgeTypeDef.label` from
//! `String` to `GraphLabel` will make invalid labels unrepresentable;
//! callers that still need the raw string reach through
//! [`GraphLabel::as_str`] or the `Deref<Target=str>` impl.
//!
//! The type is deliberately `fallible-only`: there is no `From<&str>`
//! or `From<String>` impl. Construction goes through [`GraphLabel::new`]
//! (returns `OxResult<Self>`), [`GraphLabel::parse`] (for `?`-propagated
//! construction from parsers), or the `Deserialize` impl, which
//! validates every string it reads. A caller without a recovery path
//! for an invalid label is telling a lie about their input; the type
//! refuses to pretend.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{OxError, OxResult};
use crate::types::is_valid_graph_identifier;

/// A validated graph label. See the module docs for the invariants.
///
/// Displayed and compared as the wrapped string; hashes as the string
/// too, so `HashMap<GraphLabel, _>` is interchangeable with the
/// previous `HashMap<String, _>` at lookup time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, JsonSchema, utoipa::ToSchema)]
#[schemars(transparent)]
#[schema(value_type = String)]
pub struct GraphLabel(String);

impl GraphLabel {
    /// Canonical fallible constructor.
    ///
    /// Returns `OxError::Validation` when the input fails
    /// `is_valid_graph_identifier` — empty string, or containing
    /// characters that Cypher can't safely quote (newlines, control
    /// characters, leading/trailing whitespace, etc.).
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
        is_valid_graph_identifier(s)
    }

    /// Split out of the constructor so we can produce a richer error
    /// than `is_valid_graph_identifier` does on its own (the caller
    /// sees which input failed, not just `false`).
    fn validate(s: &str) -> OxResult<()> {
        if !is_valid_graph_identifier(s) {
            return Err(OxError::Validation {
                field: "graph_label".to_string(),
                message: format!(
                    "invalid graph label `{s}` — must be non-empty and composed \
                     of alphanumeric (including Unicode letters/digits), `_`, \
                     or space characters"
                ),
            });
        }
        Ok(())
    }

    /// Read the underlying string slice. Cheap; no allocation.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the label and return its `String` backing storage.
    /// Callers that need to hand ownership of the raw string to an
    /// external API (e.g. a Cypher emitter that wants to own a
    /// `String`) use this instead of cloning.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for GraphLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for GraphLabel {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for GraphLabel {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for GraphLabel {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for GraphLabel {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for GraphLabel {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for GraphLabel {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl FromStr for GraphLabel {
    type Err = OxError;

    fn from_str(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for GraphLabel {
    type Error = OxError;

    fn try_from(s: String) -> OxResult<Self> {
        Self::new(s)
    }
}

impl TryFrom<&str> for GraphLabel {
    type Error = OxError;

    fn try_from(s: &str) -> OxResult<Self> {
        Self::new(s)
    }
}

impl Serialize for GraphLabel {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for GraphLabel {
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
    fn new_accepts_simple_ascii_label() {
        let label = GraphLabel::new("Person").expect("Person is valid");
        assert_eq!(label.as_str(), "Person");
    }

    #[test]
    fn new_accepts_korean_label() {
        // Domain-agnostic: Unicode letters must pass.
        let label = GraphLabel::new("고객").expect("Korean label is valid");
        assert_eq!(label.as_str(), "고객");
    }

    #[test]
    fn new_accepts_underscore_and_digits() {
        assert!(GraphLabel::new("User_v2").is_ok());
        assert!(GraphLabel::new("table_42").is_ok());
    }

    #[test]
    fn new_rejects_empty_string() {
        let err = GraphLabel::new("").unwrap_err();
        match err {
            OxError::Validation { field, .. } => assert_eq!(field, "graph_label"),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn new_rejects_control_characters() {
        assert!(GraphLabel::new("Line\nBreak").is_err());
        assert!(GraphLabel::new("Tab\there").is_err());
    }

    #[test]
    fn new_rejects_special_characters() {
        // `-`, `.`, `/`, `:` are the ones most likely to leak in from a
        // source schema name; a fail here prevents Cypher-injection-
        // shaped strings from surviving into emitted queries.
        assert!(GraphLabel::new("Node-Type").is_err());
        assert!(GraphLabel::new("Table.Column").is_err());
        assert!(GraphLabel::new("a/b").is_err());
        assert!(GraphLabel::new("Label:Sub").is_err());
    }

    #[test]
    fn parse_matches_new() {
        assert_eq!(
            GraphLabel::parse("Valid").unwrap(),
            GraphLabel::new("Valid").unwrap()
        );
        assert!(GraphLabel::parse("").is_err());
    }

    #[test]
    fn fromstr_matches_new() {
        let a: GraphLabel = "Foo".parse().unwrap();
        assert_eq!(a.as_str(), "Foo");
        let bad: Result<GraphLabel, _> = "bad-name".parse();
        assert!(bad.is_err());
    }

    #[test]
    fn deref_and_asref_expose_str() {
        let label = GraphLabel::new("Person").unwrap();
        fn takes_str(s: &str) -> usize {
            s.len()
        }
        assert_eq!(takes_str(&label), 6);
        assert_eq!(takes_str(label.as_ref()), 6);
    }

    #[test]
    fn partial_eq_with_str_and_string() {
        let label = GraphLabel::new("Person").unwrap();
        assert_eq!(label, "Person");
        assert_eq!(label, String::from("Person"));
    }

    #[test]
    fn display_yields_plain_string() {
        let label = GraphLabel::new("Person").unwrap();
        assert_eq!(format!("{label}"), "Person");
    }

    #[test]
    fn serde_roundtrip_transparent() {
        let label = GraphLabel::new("Person").unwrap();
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, "\"Person\"");
        let parsed: GraphLabel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, label);
    }

    #[test]
    fn deserialize_rejects_invalid_input() {
        // A JSON doc that carries an invalid label must fail to parse,
        // not silently round-trip the garbage through.
        let bad: Result<GraphLabel, _> = serde_json::from_str("\"bad-name\"");
        assert!(bad.is_err());
        let empty: Result<GraphLabel, _> = serde_json::from_str("\"\"");
        assert!(empty.is_err());
    }

    #[test]
    fn schema_is_transparent_string() {
        // The JsonSchema representation must be a plain string so that
        // consumers (LLM structured output, OpenAPI clients) see the
        // same shape they do today, not an anonymous object wrapping a
        // `0` field.
        let schema = schemars::schema_for!(GraphLabel);
        let value = serde_json::to_value(&schema).unwrap();
        // Transparent schemas render their inner type directly.
        assert_eq!(value.get("type").and_then(|v| v.as_str()), Some("string"));
    }

    #[test]
    fn hashes_as_wrapped_string() {
        use std::collections::HashMap;
        let mut map: HashMap<GraphLabel, u32> = HashMap::new();
        map.insert(GraphLabel::new("Person").unwrap(), 1);
        // Borrow<str> makes string-keyed lookup work.
        assert_eq!(map.get("Person"), Some(&1));
    }
}
