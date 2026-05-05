//! Pure helpers shared by every Bolt-driver backend.

use std::collections::HashMap;

use ox_core::error::{OxError, OxResult};
use ox_core::types::PropertyValue;

/// Truncate a query string for inclusion in error messages.
pub(crate) fn truncate_query(q: &str, max: usize) -> String {
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}...", &q[..max])
    }
}

/// Bind `PropertyValue` parameters onto a neo4rs `Query`.
pub(crate) fn bind_params(
    q: neo4rs::Query,
    params: &HashMap<String, PropertyValue>,
) -> neo4rs::Query {
    let mut q = q;
    for (name, value) in params {
        q = match value {
            PropertyValue::Bool(b) => q.param(name, *b),
            PropertyValue::Int(i) => q.param(name, *i),
            PropertyValue::Float(f) => q.param(name, *f),
            PropertyValue::String(s) => q.param(name, s.as_str()),
            PropertyValue::List(items) => {
                // neo4rs 0.8 doesn't support list params directly; serialize as JSON string
                let json = serde_json::to_string(items).unwrap_or_default();
                q.param(name, json)
            }
            PropertyValue::Map(map) => {
                let json = serde_json::to_string(map).unwrap_or_default();
                q.param(name, json)
            }
            _ => q, // Skip Null, Date, DateTime, Duration, Bytes (handled inline in Cypher)
        };
    }
    q
}

/// Bind a single `serde_json::Value` as a neo4rs parameter with the correct type.
/// Used by `execute_load` to pass per-record fields as `$row_<field>` parameters.
pub(crate) fn bind_json_field(
    q: neo4rs::Query,
    name: &str,
    value: &serde_json::Value,
) -> neo4rs::Query {
    match value {
        serde_json::Value::String(s) => q.param(name, s.as_str()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.param(name, i)
            } else if let Some(f) = n.as_f64() {
                q.param(name, f)
            } else {
                q.param(name, n.to_string())
            }
        }
        serde_json::Value::Bool(b) => q.param(name, *b),
        serde_json::Value::Null => q,
        _ => q.param(name, value.to_string()),
    }
}

/// Convert a `serde_json::Value` returned by neo4rs into a typed `PropertyValue`.
pub(crate) fn json_to_property_value(value: Option<&serde_json::Value>) -> PropertyValue {
    match value {
        Some(serde_json::Value::String(s)) => PropertyValue::String(s.clone()),
        Some(serde_json::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        Some(serde_json::Value::Bool(b)) => PropertyValue::Bool(*b),
        Some(serde_json::Value::Array(arr)) => PropertyValue::List(
            arr.iter()
                .map(|v| json_to_property_value(Some(v)))
                .collect(),
        ),
        Some(serde_json::Value::Object(obj)) => PropertyValue::Map(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_property_value(Some(v))))
                .collect(),
        ),
        Some(serde_json::Value::Null) | None => PropertyValue::Null,
    }
}

/// Validate sandbox / database identifier: only `[A-Za-z0-9_]`, length 1-63.
/// Used by both Neo4j (database names) and Memgraph (label suffixes) before
/// interpolating into DDL — guards against injection.
pub(crate) fn validate_identifier(name: &str) -> OxResult<()> {
    if name.is_empty() || name.len() > 63 {
        return Err(OxError::Validation {
            field: "name".to_string(),
            message: "Identifier must be 1-63 characters".to_string(),
        });
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(OxError::Validation {
            field: "name".to_string(),
            message: "Identifier must be alphanumeric or underscore only".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neo4rs::query;
    use serde_json::json;

    #[test]
    fn validate_identifier_accepts_safe() {
        assert!(validate_identifier("test").is_ok());
        assert!(validate_identifier("my_sandbox").is_ok());
        assert!(validate_identifier("sandbox123").is_ok());
        assert!(validate_identifier("a").is_ok());
        let max_len = "a".repeat(63);
        assert!(validate_identifier(&max_len).is_ok());
    }

    #[test]
    fn validate_identifier_rejects_unsafe() {
        for bad in [
            "",
            &"a".repeat(64),
            "test; DROP DATABASE neo4j",
            "test`; DROP",
            "my sandbox",
            "test-name",
            "test.name",
        ] {
            assert!(matches!(
                validate_identifier(bad).unwrap_err(),
                OxError::Validation { .. }
            ));
        }
    }

    #[test]
    fn bind_params_accepts_supported_types() {
        let mut params = HashMap::new();
        params.insert("name".into(), PropertyValue::String("Alice".into()));
        params.insert("age".into(), PropertyValue::Int(30));
        params.insert("score".into(), PropertyValue::Float(9.5));
        params.insert("active".into(), PropertyValue::Bool(true));
        let _q = bind_params(query("RETURN $name"), &params);
    }

    #[test]
    fn json_to_property_value_handles_all_json_types() {
        assert_eq!(
            json_to_property_value(Some(&json!("x"))),
            PropertyValue::String("x".into())
        );
        assert_eq!(
            json_to_property_value(Some(&json!(7))),
            PropertyValue::Int(7)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(2.5))),
            PropertyValue::Float(2.5)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(true))),
            PropertyValue::Bool(true)
        );
        assert_eq!(json_to_property_value(None), PropertyValue::Null);
    }

    #[test]
    fn truncate_query_short_unchanged() {
        assert_eq!(
            truncate_query("MATCH (n) RETURN n", 100),
            "MATCH (n) RETURN n"
        );
    }

    #[test]
    fn truncate_query_long_appends_ellipsis() {
        let long = "a".repeat(300);
        let result = truncate_query(&long, 50);
        assert_eq!(result.len(), 53);
        assert!(result.ends_with("..."));
    }
}
