//! Union type handling: oneOf/anyOf tagged union flattening and variant selection.

use serde_json::{Value, json};

/// Try to flatten a tagged union (oneOf/anyOf where all variants share a discriminator).
///
/// Detects patterns like:
/// ```json
/// { "oneOf": [
///   { "type": "object", "properties": { "tag": {"const": "a"}, "x": ... }, "required": [...] },
///   { "type": "object", "properties": { "tag": {"const": "b"}, "y": ... }, "required": [...] }
/// ]}
/// ```
/// And merges into a single object with a string enum discriminator and all fields nullable.
pub(crate) fn flatten_tagged_union(variants: &[Value]) -> Option<Value> {
    if variants.is_empty() {
        return None;
    }

    let discriminator = find_discriminator(variants)?;
    let mut merged_props = serde_json::Map::new();
    let mut enum_values = Vec::new();

    for variant in variants {
        let props = variant.get("properties")?.as_object()?;

        // Collect discriminator const value
        if let Some(disc_val) = props
            .get(&discriminator)
            .and_then(|d| d.get("const"))
            .and_then(|c| c.as_str())
        {
            enum_values.push(Value::String(disc_val.to_string()));
        }

        // Merge variant-specific properties (make nullable since not all variants use them)
        for (key, schema) in props {
            if key == &discriminator {
                continue;
            }
            merged_props
                .entry(key.clone())
                .or_insert_with(|| make_nullable(schema));
        }
    }

    if enum_values.is_empty() {
        return None;
    }

    // Add discriminator as string enum
    merged_props.insert(
        discriminator,
        json!({ "type": "string", "enum": enum_values }),
    );

    // All properties required (nullable ones use type array with null)
    let required: Vec<Value> = merged_props
        .keys()
        .map(|k| Value::String(k.clone()))
        .collect();

    Some(json!({
        "type": "object",
        "properties": merged_props,
        "required": required,
        "additionalProperties": false
    }))
}

/// Find the discriminator key — a property that has `"const"` in every variant.
fn find_discriminator(variants: &[Value]) -> Option<String> {
    let first_props = variants.first()?.get("properties")?.as_object()?;

    for key in first_props.keys() {
        let has_const = first_props.get(key).and_then(|s| s.get("const")).is_some();
        if !has_const {
            continue;
        }

        let all_have = variants.iter().all(|v| {
            v.get("properties")
                .and_then(|p| p.get(key))
                .and_then(|s| s.get("const"))
                .is_some()
        });

        if all_have {
            return Some(key.clone());
        }
    }
    None
}

/// Make a schema nullable by adding `"null"` to its type.
pub(crate) fn make_nullable(schema: &Value) -> Value {
    let Some(map) = schema.as_object() else {
        return schema.clone();
    };

    let mut result = map.clone();

    if let Some(ty) = result.get("type") {
        if let Some(ty_str) = ty.as_str() {
            // "string" → ["string", "null"]
            if ty_str != "null" {
                result.insert("type".to_string(), json!([ty_str, "null"]));
            }
        } else if let Some(ty_arr) = ty.as_array() {
            // Already an array — add "null" if not present
            if !ty_arr.iter().any(|t| t.as_str() == Some("null")) {
                let mut types = ty_arr.clone();
                types.push(json!("null"));
                result.insert("type".to_string(), Value::Array(types));
            }
        }
    } else {
        // No "type" field — wrap in a nullable pattern
        // Just add "type": ["object", "null"] as a best guess
        result.insert("type".to_string(), json!(["object", "null"]));
    }

    Value::Object(result)
}

/// For oneOf/anyOf that isn't a tagged union (e.g., nullable pattern, string enum),
/// pick the most descriptive variant or merge const values into an enum.
pub(crate) fn pick_best_variant(variants: &[Value]) -> Option<Value> {
    // Filter out null-type variants
    let non_null: Vec<&Value> = variants
        .iter()
        .filter(|v| v.get("type").and_then(|t| t.as_str()) != Some("null"))
        .collect();
    let has_null = non_null.len() < variants.len();

    if non_null.len() == 1 && has_null {
        // Simple nullable pattern: oneOf [Type, null] → make Type nullable
        return Some(make_nullable(non_null[0]));
    }

    // Check if all non-null variants are const strings → merge into string enum
    let const_values: Vec<&str> = non_null
        .iter()
        .filter_map(|v| v.get("const").and_then(|c| c.as_str()))
        .collect();
    if const_values.len() == non_null.len() && !const_values.is_empty() {
        let enum_vals: Vec<Value> = const_values.iter().map(|s| json!(s)).collect();
        let mut result = json!({ "type": "string", "enum": enum_vals });
        if has_null {
            result = make_nullable(&result);
        }
        return Some(result);
    }

    // Multiple non-null variants without a discriminator — use permissive schema
    None
}
