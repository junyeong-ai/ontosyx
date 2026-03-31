//! Diagnostic reporting and validation: circular ref detection, composition checks,
//! property counting.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Check if a JSON Schema has circular `$ref` references in its `$defs`.
///
/// Providers like Bedrock reject schemas with self-referencing or mutually-referencing
/// definitions. When detected, callers should fall back to plain JSON mode.
pub fn has_circular_refs(schema: &Value) -> bool {
    let defs = match schema.get("$defs").and_then(|d| d.as_object()) {
        Some(d) => d,
        None => return false,
    };

    // Build adjacency: def_name -> set of def_names it references
    let mut edges: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (name, def_value) in defs {
        let mut refs = HashSet::new();
        collect_refs(def_value, &mut refs);
        edges.insert(name.as_str(), refs);
    }

    // DFS cycle detection
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();

    fn dfs<'a>(
        node: &'a str,
        edges: &HashMap<&'a str, HashSet<&'a str>>,
        visited: &mut HashSet<&'a str>,
        stack: &mut HashSet<&'a str>,
    ) -> bool {
        if stack.contains(node) {
            return true; // cycle
        }
        if visited.contains(node) {
            return false;
        }
        visited.insert(node);
        stack.insert(node);
        if let Some(neighbors) = edges.get(node) {
            for &neighbor in neighbors {
                if dfs(neighbor, edges, visited, stack) {
                    return true;
                }
            }
        }
        stack.remove(node);
        false
    }

    for name in defs.keys() {
        if dfs(name.as_str(), &edges, &mut visited, &mut stack) {
            return true;
        }
    }
    false
}

/// Check if a JSON Schema uses `oneOf`, `anyOf`, or `allOf` composition keywords.
///
/// Bedrock's structured output rejects these entirely ("Schema type 'oneOf' is not supported").
/// When detected, callers should fall back to plain JSON mode.
pub fn has_unsupported_composition(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => {
            for keyword in ["oneOf", "anyOf", "allOf"] {
                if map.contains_key(keyword) {
                    return true;
                }
            }
            for val in map.values() {
                if has_unsupported_composition(val) {
                    return true;
                }
            }
            false
        }
        Value::Array(arr) => arr.iter().any(has_unsupported_composition),
        _ => false,
    }
}

/// Recursively collect all `$ref` targets from a JSON value.
/// Extracts def name from `"#/$defs/Foo"` references.
pub(crate) fn collect_refs<'a>(value: &'a Value, refs: &mut HashSet<&'a str>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref")
                && let Some(name) = r.strip_prefix("#/$defs/")
            {
                refs.insert(name);
            }
            for val in map.values() {
                collect_refs(val, refs);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                collect_refs(val, refs);
            }
        }
        _ => {}
    }
}

/// Count optional parameters in a schema (properties not in `required`).
/// Bedrock limits this to 24 for structured output.
pub fn count_optional_params(schema: &Value) -> usize {
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
                    count += count_optional_params(val);
                }
            }
            if let Some(items) = map.get("items") {
                count += count_optional_params(items);
            }
            count
        }
        Value::Array(arr) => arr.iter().map(count_optional_params).sum(),
        _ => 0,
    }
}

/// Estimate total property count across all schema levels.
/// Used to decide whether structured output will produce too much JSON.
pub fn count_total_properties(schema: &Value) -> usize {
    match schema {
        Value::Object(map) => {
            let mut count = 0;
            if let Some(Value::Object(props)) = map.get("properties") {
                count += props.len();
                for val in props.values() {
                    count += count_total_properties(val);
                }
            }
            if let Some(items) = map.get("items") {
                count += count_total_properties(items);
            }
            count
        }
        Value::Array(arr) => arr.iter().map(count_total_properties).sum(),
        _ => 0,
    }
}
