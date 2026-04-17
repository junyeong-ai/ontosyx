use std::collections::HashMap;

use ox_core::types::PropertyValue;

/// Truncate a query string for inclusion in error messages.
pub(super) fn truncate_query(q: &str, max: usize) -> String {
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}...", &q[..max])
    }
}

/// Bind `PropertyValue` parameters onto a neo4rs `Query`.
pub(super) fn bind_params(
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
pub(super) fn bind_json_field(
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
pub(super) fn json_to_property_value(value: Option<&serde_json::Value>) -> PropertyValue {
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

/// Validate sandbox name: only alphanumeric + underscore, 1-63 chars.
pub(super) fn validate_identifier(name: &str) -> ox_core::error::OxResult<()> {
    use ox_core::error::OxError;
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
    use ox_core::error::OxError;
    use serde_json::json;

    #[test]
    fn test_bind_params_string() {
        let mut params = HashMap::new();
        params.insert(
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        params.insert("age".to_string(), PropertyValue::Int(30));
        params.insert("score".to_string(), PropertyValue::Float(9.5));
        params.insert("active".to_string(), PropertyValue::Bool(true));
        let _q = bind_params(query("MATCH (n) WHERE n.name = $name RETURN n"), &params);
    }

    #[test]
    fn test_bind_params_null_skipped() {
        let mut params = HashMap::new();
        params.insert("value".to_string(), PropertyValue::Null);
        let _q = bind_params(query("RETURN $value"), &params);
    }

    #[test]
    fn test_bind_params_list_json_serialized() {
        let mut params = HashMap::new();
        params.insert(
            "tags".to_string(),
            PropertyValue::List(vec![
                PropertyValue::String("a".to_string()),
                PropertyValue::String("b".to_string()),
            ]),
        );
        let _q = bind_params(query("RETURN $tags"), &params);
    }

    #[test]
    fn test_json_to_property_value_all_types() {
        assert_eq!(
            json_to_property_value(Some(&json!("hello"))),
            PropertyValue::String("hello".to_string())
        );
        assert_eq!(
            json_to_property_value(Some(&json!(42))),
            PropertyValue::Int(42)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(3.25))),
            PropertyValue::Float(3.25)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(true))),
            PropertyValue::Bool(true)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(false))),
            PropertyValue::Bool(false)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(null))),
            PropertyValue::Null
        );
        assert_eq!(json_to_property_value(None), PropertyValue::Null);
    }

    #[test]
    fn test_json_to_property_value_nested() {
        let arr = json!([1, "two", true]);
        match json_to_property_value(Some(&arr)) {
            PropertyValue::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], PropertyValue::Int(1));
                assert_eq!(items[1], PropertyValue::String("two".to_string()));
                assert_eq!(items[2], PropertyValue::Bool(true));
            }
            other => panic!("Expected List, got {other:?}"),
        }

        let obj = json!({"key": "value", "num": 99});
        match json_to_property_value(Some(&obj)) {
            PropertyValue::Map(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get("key"),
                    Some(&PropertyValue::String("value".to_string()))
                );
                assert_eq!(map.get("num"), Some(&PropertyValue::Int(99)));
            }
            other => panic!("Expected Map, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("test").is_ok());
        assert!(validate_identifier("my_sandbox").is_ok());
        assert!(validate_identifier("sandbox123").is_ok());
        assert!(validate_identifier("a").is_ok());
        let max_len = "a".repeat(63);
        assert!(validate_identifier(&max_len).is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        for bad in [
            "",
            &"a".repeat(64),
            "test; DROP DATABASE neo4j",
            "test`; DROP",
            "my sandbox",
            "test-name",
            "test.name",
        ] {
            let err = validate_identifier(bad).unwrap_err();
            assert!(matches!(err, OxError::Validation { .. }));
        }
    }

    #[test]
    fn test_truncate_query_short() {
        let short = "MATCH (n) RETURN n";
        assert_eq!(truncate_query(short, 100), short);
    }

    #[test]
    fn test_truncate_query_long() {
        let long = "a".repeat(300);
        let result = truncate_query(&long, 50);
        assert_eq!(result.len(), 53);
        assert!(result.ends_with("..."));
    }
}
