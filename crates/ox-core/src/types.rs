use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PropertyType — DB-agnostic type system for graph properties
// Covers the INTERSECTION of Neo4j, Neptune (openCypher), and GQL types.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PropertyType {
    Bool,
    Int,
    Float,
    String,
    Date,
    DateTime,
    Duration,
    Bytes,
    List { element: Box<PropertyType> },
    Map,
}

/// Custom JsonSchema: non-recursive, Bedrock-compatible schema for PropertyType.
/// Uses a simple string type — the custom Deserialize impl handles both bare strings
/// ("string", "int") and tagged objects ({"type": "list", "element": ...}).
/// Bedrock doesn't support `oneOf`/`const` well, so we keep the schema simple
/// and rely on the prompt + custom deserializer for correctness.
impl JsonSchema for PropertyType {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PropertyType".into()
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value = serde_json::json!({
            "type": "string",
            "description": "Property type: bool, int, float, string, date, datetime, duration, bytes, map"
        });
        let map: serde_json::Map<std::string::String, serde_json::Value> =
            serde_json::from_value(value).expect("valid schema object");
        schemars::Schema::from(map)
    }
}

/// Custom deserializer: accepts both `{"type": "string"}` (tagged) and `"string"` (bare).
impl<'de> Deserialize<'de> for PropertyType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        let value = serde_json::Value::deserialize(deserializer)?;
        match &value {
            // Bare string: "string", "int", etc.
            serde_json::Value::String(s) => match s.as_str() {
                "bool" | "boolean" => Ok(PropertyType::Bool),
                "int" | "integer" | "long" => Ok(PropertyType::Int),
                "float" | "double" | "number" => Ok(PropertyType::Float),
                "string" | "text" => Ok(PropertyType::String),
                "date" => Ok(PropertyType::Date),
                "datetime" | "date_time" | "timestamp" => Ok(PropertyType::DateTime),
                "duration" => Ok(PropertyType::Duration),
                "bytes" | "binary" => Ok(PropertyType::Bytes),
                "map" | "object" => Ok(PropertyType::Map),
                other => Err(de::Error::custom(format!("unknown property type: {other}"))),
            },
            // Tagged object: {"type": "string"} or {"type": "list", "element": {...}}
            serde_json::Value::Object(_) => {
                #[derive(Deserialize)]
                #[serde(tag = "type", rename_all = "snake_case")]
                enum Tagged {
                    Bool,
                    Int,
                    Float,
                    String,
                    Date,
                    DateTime,
                    Duration,
                    Bytes,
                    List { element: Box<PropertyType> },
                    Map,
                }
                let tagged: Tagged = serde_json::from_value(value).map_err(de::Error::custom)?;
                Ok(match tagged {
                    Tagged::Bool => PropertyType::Bool,
                    Tagged::Int => PropertyType::Int,
                    Tagged::Float => PropertyType::Float,
                    Tagged::String => PropertyType::String,
                    Tagged::Date => PropertyType::Date,
                    Tagged::DateTime => PropertyType::DateTime,
                    Tagged::Duration => PropertyType::Duration,
                    Tagged::Bytes => PropertyType::Bytes,
                    Tagged::List { element } => PropertyType::List { element },
                    Tagged::Map => PropertyType::Map,
                })
            }
            _ => Err(de::Error::custom(
                "expected string or object for PropertyType",
            )),
        }
    }
}

impl PropertyType {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    pub fn is_temporal(&self) -> bool {
        matches!(self, Self::Date | Self::DateTime | Self::Duration)
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool => write!(f, "Bool"),
            Self::Int => write!(f, "Int"),
            Self::Float => write!(f, "Float"),
            Self::String => write!(f, "String"),
            Self::Date => write!(f, "Date"),
            Self::DateTime => write!(f, "DateTime"),
            Self::Duration => write!(f, "Duration"),
            Self::Bytes => write!(f, "Bytes"),
            Self::List { element } => write!(f, "List<{element}>"),
            Self::Map => write!(f, "Map"),
        }
    }
}

// ---------------------------------------------------------------------------
// PropertyValue — runtime value carrier, DB-agnostic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    /// ISO 8601 duration string (e.g. "P1Y2M3D")
    Duration(String),
    Bytes(Vec<u8>),
    List(Vec<PropertyValue>),
    Map(HashMap<String, PropertyValue>),
}

/// Custom deserializer: accepts both the canonical tagged format and LLM shorthand.
///
/// Canonical: `{"type": "string", "value": "hello"}`
/// LLM shorthand (bare primitives):
///   - `"hello"` → `PropertyValue::String("hello")`
///   - `42` → `PropertyValue::Int(42)`
///   - `3.14` → `PropertyValue::Float(3.14)`
///   - `true` → `PropertyValue::Bool(true)`
///   - `null` → `PropertyValue::Null`
///   - `[1, 2]` → `PropertyValue::List([Int(1), Int(2)])`
///   - `{}` (empty object) → `PropertyValue::Null` (LLM artifact)
impl<'de> Deserialize<'de> for PropertyValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        deserialize_property_value_from_json(value).map_err(serde::de::Error::custom)
    }
}

/// Core deserialization logic for PropertyValue.
/// Handles both canonical tagged format and bare JSON primitives from LLM output.
fn deserialize_property_value_from_json(
    value: serde_json::Value,
) -> Result<PropertyValue, std::string::String> {
    match value {
        serde_json::Value::Null => Ok(PropertyValue::Null),
        serde_json::Value::Bool(b) => Ok(PropertyValue::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PropertyValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(PropertyValue::Float(f))
            } else {
                Ok(PropertyValue::Null)
            }
        }
        serde_json::Value::String(s) => Ok(PropertyValue::String(s)),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<PropertyValue>, _> = arr
                .into_iter()
                .map(deserialize_property_value_from_json)
                .collect();
            Ok(PropertyValue::List(items?))
        }
        serde_json::Value::Object(ref map) if map.is_empty() => Ok(PropertyValue::Null),
        serde_json::Value::Object(ref map) if map.contains_key("type") => {
            // Canonical tagged format: {"type": "string", "value": "hello"}
            #[derive(Deserialize)]
            #[serde(tag = "type", content = "value", rename_all = "snake_case")]
            enum Tagged {
                Null,
                Bool(bool),
                Int(i64),
                Float(f64),
                String(String),
                Date(NaiveDate),
                DateTime(NaiveDateTime),
                Duration(String),
                Bytes(Vec<u8>),
                List(Vec<PropertyValue>),
                Map(HashMap<String, PropertyValue>),
            }
            match serde_json::from_value::<Tagged>(value.clone()) {
                Ok(tagged) => Ok(match tagged {
                    Tagged::Null => PropertyValue::Null,
                    Tagged::Bool(v) => PropertyValue::Bool(v),
                    Tagged::Int(v) => PropertyValue::Int(v),
                    Tagged::Float(v) => PropertyValue::Float(v),
                    Tagged::String(v) => PropertyValue::String(v),
                    Tagged::Date(v) => PropertyValue::Date(v),
                    Tagged::DateTime(v) => PropertyValue::DateTime(v),
                    Tagged::Duration(v) => PropertyValue::Duration(v),
                    Tagged::Bytes(v) => PropertyValue::Bytes(v),
                    Tagged::List(v) => PropertyValue::List(v),
                    Tagged::Map(v) => PropertyValue::Map(v),
                }),
                Err(_) => {
                    // Tagged parse failed — treat as generic map
                    let map_result: Result<HashMap<String, PropertyValue>, _> = map
                        .iter()
                        .map(|(k, v)| {
                            deserialize_property_value_from_json(v.clone())
                                .map(|pv| (k.clone(), pv))
                        })
                        .collect();
                    Ok(PropertyValue::Map(map_result?))
                }
            }
        }
        serde_json::Value::Object(map) => {
            // Object without "type" key — generic map
            let map_result: Result<HashMap<String, PropertyValue>, _> = map
                .into_iter()
                .map(|(k, v)| deserialize_property_value_from_json(v).map(|pv| (k, pv)))
                .collect();
            Ok(PropertyValue::Map(map_result?))
        }
    }
}

/// Custom JsonSchema: non-recursive, Bedrock-compatible schema for PropertyValue.
/// PropertyValue is primarily used for default_value (optional), so a permissive
/// schema is acceptable — the serde deserializer handles actual validation.
impl JsonSchema for PropertyValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PropertyValue".into()
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value = serde_json::json!({
            "description": "A typed property value, e.g. {\"type\": \"string\", \"value\": \"hello\"} or null"
        });
        let map: serde_json::Map<std::string::String, serde_json::Value> =
            serde_json::from_value(value).expect("valid schema object");
        schemars::Schema::from(map)
    }
}

impl PropertyValue {
    pub fn property_type(&self) -> Option<PropertyType> {
        match self {
            Self::Null => None,
            Self::Bool(_) => Some(PropertyType::Bool),
            Self::Int(_) => Some(PropertyType::Int),
            Self::Float(_) => Some(PropertyType::Float),
            Self::String(_) => Some(PropertyType::String),
            Self::Date(_) => Some(PropertyType::Date),
            Self::DateTime(_) => Some(PropertyType::DateTime),
            Self::Duration(_) => Some(PropertyType::Duration),
            Self::Bytes(_) => None,
            Self::List(items) => Some(PropertyType::List {
                element: Box::new(
                    items
                        .first()
                        .and_then(|v| v.property_type())
                        .unwrap_or(PropertyType::String),
                ),
            }),
            Self::Map(_) => Some(PropertyType::Map),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl std::fmt::Display for PropertyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::String(v) => write!(f, "\"{v}\""),
            Self::Date(v) => write!(f, "date(\"{v}\")"),
            Self::DateTime(v) => write!(f, "datetime(\"{v}\")"),
            Self::Duration(v) => write!(f, "duration(\"{v}\")"),
            Self::Bytes(v) => write!(f, "<{} bytes>", v.len()),
            Self::List(v) => write!(f, "[{} items]", v.len()),
            Self::Map(v) => write!(f, "{{{} entries}}", v.len()),
        }
    }
}

// ---------------------------------------------------------------------------
// Direction — edge traversal direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

// ---------------------------------------------------------------------------
// CompilationTarget — which graph DB backend to target
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompilationTarget {
    Cypher,
    OpenCypher,
    Gql,
    Gremlin,
}

impl std::fmt::Display for CompilationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cypher => write!(f, "Cypher (Neo4j)"),
            Self::OpenCypher => write!(f, "openCypher (Neptune)"),
            Self::Gql => write!(f, "GQL (ISO)"),
            Self::Gremlin => write!(f, "Gremlin"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cypher identifier escaping — shared between ox-compiler and ox-runtime
// ---------------------------------------------------------------------------

/// Backtick-escapes a Cypher identifier (label, property name, relationship type).
/// Any backtick within the name is doubled, and the result is wrapped in backticks.
pub fn escape_cypher_identifier(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// Check whether a string is safe to use as a graph identifier (label or property name).
/// Allows alphanumeric characters, underscores, and spaces (common in business labels).
/// Rejects backticks, semicolons, braces, and other characters that could cause injection.
pub fn is_valid_graph_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
}

/// Sanitize a variable name from LLM output.
///
/// Returns `None` for invalid/nonsensical names:
/// - empty string, `"null"`, `"-"`, or names not starting with a letter/underscore.
///
/// Used in both MatchQueryIR conversion and Cypher pattern compilation
/// to ensure consistent handling of LLM-generated variable names.
pub fn sanitize_variable(v: &str) -> Option<&str> {
    if v.is_empty()
        || v == "null"
        || v == "-"
        || !v.starts_with(|c: char| c.is_alphabetic() || c == '_')
    {
        None
    } else {
        Some(v)
    }
}

// ---------------------------------------------------------------------------
// Shared serde helpers
// ---------------------------------------------------------------------------

/// Deserialize `Option<PropertyValue>` that maps null-like values to `None`.
///
/// - absent field → `None`
/// - `null` → `None`
/// - `PropertyValue::Null` (e.g. `{}`) → `None`
/// - any other value → `Some(value)`
pub fn deserialize_optional_property_value<'de, D>(
    deserializer: D,
) -> Result<Option<PropertyValue>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let pv: Option<PropertyValue> = Option::deserialize(deserializer)?;
    match pv {
        Some(PropertyValue::Null) => Ok(None),
        other => Ok(other),
    }
}

/// Deserialize `Option<Option<PropertyValue>>` for patch-style fields.
///
/// Used by `PropertyPatch.default_value`:
/// - field absent / `null` / `{}` → `None` (no change)
/// - any other value → `Some(Some(value))`
///
/// Note: JSON cannot distinguish "absent" from "null" in `Option<Option<T>>`.
/// Both are treated as "no change". To explicitly clear a default value,
/// the caller should use a separate mechanism (e.g., a dedicated "clear" flag).
pub fn deserialize_patch_property_value<'de, D>(
    deserializer: D,
) -> Result<Option<Option<PropertyValue>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let pv: Option<PropertyValue> = Option::deserialize(deserializer)?;
    match pv {
        None | Some(PropertyValue::Null) => Ok(None),
        Some(v) => Ok(Some(Some(v))),
    }
}

// ---------------------------------------------------------------------------
// Tests — PropertyValue deserialization robustness
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_value_canonical_tagged() {
        let pv: PropertyValue =
            serde_json::from_str(r#"{"type":"string","value":"hello"}"#).unwrap();
        assert_eq!(pv, PropertyValue::String("hello".into()));

        let pv: PropertyValue = serde_json::from_str(r#"{"type":"int","value":42}"#).unwrap();
        assert_eq!(pv, PropertyValue::Int(42));

        let pv: PropertyValue = serde_json::from_str(r#"{"type":"bool","value":true}"#).unwrap();
        assert_eq!(pv, PropertyValue::Bool(true));

        let pv: PropertyValue = serde_json::from_str(r#"{"type":"null"}"#).unwrap();
        assert_eq!(pv, PropertyValue::Null);
    }

    #[test]
    fn property_value_bare_primitives() {
        let pv: PropertyValue = serde_json::from_str(r#""hello""#).unwrap();
        assert_eq!(pv, PropertyValue::String("hello".into()));

        let pv: PropertyValue = serde_json::from_str("42").unwrap();
        assert_eq!(pv, PropertyValue::Int(42));

        let pv: PropertyValue = serde_json::from_str("3.25").unwrap();
        assert_eq!(pv, PropertyValue::Float(3.25));

        let pv: PropertyValue = serde_json::from_str("true").unwrap();
        assert_eq!(pv, PropertyValue::Bool(true));

        let pv: PropertyValue = serde_json::from_str("null").unwrap();
        assert_eq!(pv, PropertyValue::Null);
    }

    #[test]
    fn property_value_empty_object_is_null() {
        let pv: PropertyValue = serde_json::from_str("{}").unwrap();
        assert_eq!(pv, PropertyValue::Null);
    }

    #[test]
    fn property_value_bare_array() {
        let pv: PropertyValue = serde_json::from_str(r#"[1, 2, 3]"#).unwrap();
        assert_eq!(
            pv,
            PropertyValue::List(vec![
                PropertyValue::Int(1),
                PropertyValue::Int(2),
                PropertyValue::Int(3),
            ])
        );
    }

    #[test]
    fn property_value_roundtrip() {
        let original = PropertyValue::String("test".into());
        let json = serde_json::to_string(&original).unwrap();
        let parsed: PropertyValue = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn optional_property_value_null_variants() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "deserialize_optional_property_value")]
            val: Option<PropertyValue>,
        }

        let w: Wrapper = serde_json::from_str(r#"{"val": null}"#).unwrap();
        assert!(w.val.is_none());

        let w: Wrapper = serde_json::from_str(r#"{"val": {}}"#).unwrap();
        assert!(w.val.is_none());

        let w: Wrapper = serde_json::from_str(r#"{}"#).unwrap();
        assert!(w.val.is_none());

        let w: Wrapper = serde_json::from_str(r#"{"val": "hello"}"#).unwrap();
        assert_eq!(w.val, Some(PropertyValue::String("hello".into())));
    }

    #[test]
    fn patch_property_value_semantics() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "deserialize_patch_property_value")]
            val: Option<Option<PropertyValue>>,
        }

        // absent → None (no change)
        let w: Wrapper = serde_json::from_str(r#"{}"#).unwrap();
        assert!(w.val.is_none());

        // null → None (serde Option treats JSON null as None)
        let w: Wrapper = serde_json::from_str(r#"{"val": null}"#).unwrap();
        assert!(w.val.is_none());

        // {} → None (PropertyValue::Null → treated as no value)
        let w: Wrapper = serde_json::from_str(r#"{"val": {}}"#).unwrap();
        assert!(w.val.is_none());

        // bare string → Some(Some(String))
        let w: Wrapper = serde_json::from_str(r#"{"val": "hello"}"#).unwrap();
        assert_eq!(w.val, Some(Some(PropertyValue::String("hello".into()))));

        // bare number → Some(Some(Int))
        let w: Wrapper = serde_json::from_str(r#"{"val": 42}"#).unwrap();
        assert_eq!(w.val, Some(Some(PropertyValue::Int(42))));

        // tagged → Some(Some(value))
        let w: Wrapper = serde_json::from_str(r#"{"val": {"type":"string","value":"x"}}"#).unwrap();
        assert_eq!(w.val, Some(Some(PropertyValue::String("x".into()))));
    }
}
