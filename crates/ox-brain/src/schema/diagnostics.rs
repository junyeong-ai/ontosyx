//! Schema complexity analysis: property counting for structured output quality thresholds.
//!
//! These functions count on the raw schemars-generated schema, which contains
//! `$ref` and `$defs`. To get accurate counts, `$ref` pointers are resolved
//! inline with cycle-safe backtracking.
//!
//! **Design:** The `visiting` set tracks the current recursion stack, not a
//! global "already seen" set. When traversal of a def completes, the def is
//! removed from `visiting` so sibling references to the same def are counted
//! independently. This accurately reflects the expanded schema that the LLM
//! actually processes (shared defs are inlined at every use site).

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Count optional parameters in a schema (properties not in `required`).
/// Follows `$ref` into `$defs` with cycle-safe backtracking.
pub fn count_optional_params(schema: &Value) -> usize {
    let defs = collect_defs(schema);
    let mut stack = HashSet::new();
    count_optional_inner(schema, &defs, &mut stack)
}

fn count_optional_inner(
    schema: &Value,
    defs: &HashMap<String, Value>,
    stack: &mut HashSet<String>,
) -> usize {
    // Resolve $ref with backtracking
    if let Some((resolved, def_name)) = resolve_ref(schema, defs, stack) {
        stack.insert(def_name.clone());
        let count = count_optional_inner(resolved, defs, stack);
        stack.remove(&def_name);
        return count;
    }

    match schema {
        Value::Object(map) => {
            let mut count = 0;
            if let Some(Value::Object(props)) = map.get("properties") {
                let required: HashSet<&str> = map
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                count += props
                    .keys()
                    .filter(|k| !required.contains(k.as_str()))
                    .count();
                for val in props.values() {
                    count += count_optional_inner(val, defs, stack);
                }
            }
            if let Some(items) = map.get("items") {
                count += count_optional_inner(items, defs, stack);
            }
            for keyword in ["oneOf", "anyOf", "allOf"] {
                if let Some(Value::Array(variants)) = map.get(keyword) {
                    for variant in variants {
                        count += count_optional_inner(variant, defs, stack);
                    }
                }
            }
            count
        }
        Value::Array(arr) => arr
            .iter()
            .map(|v| count_optional_inner(v, defs, stack))
            .sum(),
        _ => 0,
    }
}

/// Estimate total property count across all schema levels.
/// Follows `$ref` into `$defs` with cycle-safe backtracking.
pub fn count_total_properties(schema: &Value) -> usize {
    let defs = collect_defs(schema);
    let mut stack = HashSet::new();
    count_total_inner(schema, &defs, &mut stack)
}

fn count_total_inner(
    schema: &Value,
    defs: &HashMap<String, Value>,
    stack: &mut HashSet<String>,
) -> usize {
    // Resolve $ref with backtracking
    if let Some((resolved, def_name)) = resolve_ref(schema, defs, stack) {
        stack.insert(def_name.clone());
        let count = count_total_inner(resolved, defs, stack);
        stack.remove(&def_name);
        return count;
    }

    match schema {
        Value::Object(map) => {
            let mut count = 0;
            if let Some(Value::Object(props)) = map.get("properties") {
                count += props.len();
                for val in props.values() {
                    count += count_total_inner(val, defs, stack);
                }
            }
            if let Some(items) = map.get("items") {
                count += count_total_inner(items, defs, stack);
            }
            for keyword in ["oneOf", "anyOf", "allOf"] {
                if let Some(Value::Array(variants)) = map.get(keyword) {
                    for variant in variants {
                        count += count_total_inner(variant, defs, stack);
                    }
                }
            }
            count
        }
        Value::Array(arr) => arr.iter().map(|v| count_total_inner(v, defs, stack)).sum(),
        _ => 0,
    }
}

/// Resolve a `$ref` pointer against `$defs`. Returns the resolved def and
/// its name (for the caller to manage the recursion stack).
///
/// Returns `None` if:
/// - Not a `$ref` node
/// - Def not found in `$defs`
/// - Def is already on the recursion stack (true cycle like Node → child → Node)
fn resolve_ref<'a>(
    schema: &'a Value,
    defs: &'a HashMap<String, Value>,
    stack: &HashSet<String>,
) -> Option<(&'a Value, String)> {
    let ref_str = schema
        .as_object()
        .and_then(|m| m.get("$ref"))
        .and_then(|r| r.as_str())?;

    let def_name = ref_str.strip_prefix("#/$defs/")?;

    // True cycle: this def is an ancestor in the current recursion path
    if stack.contains(def_name) {
        return None;
    }

    let def = defs.get(def_name)?;
    Some((def, def_name.to_string()))
}

/// Collect `$defs` from the root schema into a lookup map.
fn collect_defs(schema: &Value) -> HashMap<String, Value> {
    schema
        .get("$defs")
        .and_then(|d| d.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counts_follow_refs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "address": { "$ref": "#/$defs/Address" }
            },
            "required": ["address"],
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "street": { "type": "string" },
                        "city": { "type": "string" },
                        "zip": { "type": "string" }
                    },
                    "required": ["street", "city"]
                }
            }
        });
        assert_eq!(count_total_properties(&schema), 4);
        assert_eq!(count_optional_params(&schema), 1);
    }

    #[test]
    fn counts_handle_circular_refs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "root": { "$ref": "#/$defs/Node" }
            },
            "$defs": {
                "Node": {
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" },
                        "child": { "$ref": "#/$defs/Node" }
                    }
                }
            }
        });
        // root(1) + Node.value(1) + Node.child(circular→0) = 3 total
        let total = count_total_properties(&schema);
        assert_eq!(total, 3);
    }

    #[test]
    fn shared_refs_counted_at_each_use_site() {
        // Two properties reference the same $def — both should count
        let schema = json!({
            "type": "object",
            "properties": {
                "home": { "$ref": "#/$defs/Address" },
                "work": { "$ref": "#/$defs/Address" }
            },
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "street": { "type": "string" },
                        "city": { "type": "string" }
                    }
                }
            }
        });
        // root: 2 props (home, work)
        // home→Address: 2 props (street, city)
        // work→Address: 2 props (street, city) — NOT treated as cycle
        assert_eq!(count_total_properties(&schema), 6);
    }

    #[test]
    fn counts_handle_one_of() {
        let schema = json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string" },
                        "radius": { "type": "number" }
                    },
                    "required": ["kind", "radius"]
                },
                {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string" },
                        "width": { "type": "number" }
                    },
                    "required": ["kind"]
                }
            ]
        });
        assert_eq!(count_total_properties(&schema), 4);
        assert_eq!(count_optional_params(&schema), 1);
    }
}
