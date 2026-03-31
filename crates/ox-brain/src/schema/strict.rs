//! Strict mode schema transformations for Bedrock/Anthropic structured output.
//!
//! Enforces provider constraints: no nullable type arrays, no unsupported keywords,
//! `additionalProperties: false` on all objects, and correct `required` arrays.

use serde_json::Value;

/// Recursively enforce strict object schemas for Bedrock structured output:
/// - Converts `type: ["X", "null"]` → `type: "X"` (nullable fields omitted from `required`)
/// - Adds `additionalProperties: false` to all object types
/// - Adds non-nullable properties to `required`
/// - Strips unsupported keywords (`minimum`, `maxLength`, `format`, `$schema`, etc.)
pub fn enforce_strict_object_schemas(value: &mut Value) {
    let Value::Object(map) = value else { return };

    // --- Step 1: Convert nullable type arrays to simple types ---
    // Bedrock doesn't fully support type arrays like ["string", "null"].
    // Convert to simple type; nullability handled by omitting from `required`.
    let mut is_nullable = false;
    if let Some(ty) = map.get("type").cloned()
        && let Some(arr) = ty.as_array()
    {
        let non_null: Vec<&Value> = arr.iter().filter(|v| v.as_str() != Some("null")).collect();
        if arr.iter().any(|v| v.as_str() == Some("null")) {
            is_nullable = true;
        }
        if non_null.len() == 1 {
            map.insert("type".to_string(), non_null[0].clone());
        }
    }

    // --- Step 1b: Fix schemas without a type ---
    // Bedrock requires every schema to have a "type" field.
    // - Empty schemas `{}` (from broken circular refs) → `{"type": "object"}`
    // - Schemas with only description → add `"type": "object"`
    if !map.contains_key("type")
        && !map.contains_key("oneOf")
        && !map.contains_key("anyOf")
        && !map.contains_key("allOf")
        && !map.contains_key("$ref")
        && !map.contains_key("enum")
    {
        map.insert("type".to_string(), Value::String("object".to_string()));
    }

    // --- Step 2: Remove unsupported keywords ---
    for unsupported in [
        "$schema",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "minProperties",
        "maxProperties",
        "uniqueItems",
        "title",
        "default",
        "examples",
        "format",
    ] {
        map.remove(unsupported);
    }

    // --- Step 3: Recurse into children FIRST (so we can inspect their nullability) ---
    if let Some(Value::Object(props)) = map.get_mut("properties") {
        for val in props.values_mut() {
            enforce_strict_object_schemas(val);
        }
    }
    if let Some(Value::Object(defs)) = map.get_mut("$defs") {
        for val in defs.values_mut() {
            enforce_strict_object_schemas(val);
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(arr)) = map.get_mut(keyword) {
            for val in arr.iter_mut() {
                enforce_strict_object_schemas(val);
            }
        }
    }
    if let Some(items) = map.get_mut("items") {
        enforce_strict_object_schemas(items);
    }

    // --- Step 4: Add additionalProperties: false on all objects ---
    let is_object = map
        .get("type")
        .is_some_and(|t| t.as_str().is_some_and(|s| s == "object"))
        || map.contains_key("properties");
    if is_object {
        map.entry("additionalProperties")
            .or_insert(Value::Bool(false));
    }

    // --- Step 5: Build required array (non-nullable properties only) ---
    // After recursion, child properties have been simplified.
    // A property is nullable if it had type ["X", "null"] (now carries __nullable flag).
    if map.contains_key("properties")
        && let Some(Value::Object(props)) = map.get("properties")
    {
        let required_keys: Vec<Value> = props
            .iter()
            .filter(|(_, v)| {
                // Include only non-nullable properties
                v.get("__nullable").and_then(|n| n.as_bool()) != Some(true)
            })
            .map(|(k, _)| Value::String(k.clone()))
            .collect();
        if !required_keys.is_empty() {
            map.insert("required".to_string(), Value::Array(required_keys));
        } else {
            map.remove("required");
        }
    }

    // Mark this node as nullable (for parent's required-array logic)
    if is_nullable {
        map.insert("__nullable".to_string(), Value::Bool(true));
    }
}

/// Remove internal `__nullable` flags from the final schema.
pub fn clean_nullable_flags(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("__nullable");
            for val in map.values_mut() {
                clean_nullable_flags(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                clean_nullable_flags(val);
            }
        }
        _ => {}
    }
}
