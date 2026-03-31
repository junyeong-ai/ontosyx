//! Inline/flatten operations for nested schemas: $ref resolution and node traversal.

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::MAX_INLINE_DEPTH;
use super::merge::merge_all_of;
use super::union::{flatten_tagged_union, pick_best_variant};

/// Recursively resolve a schema node: inline refs, flatten unions, clean keywords.
pub(crate) fn resolve_node(
    value: &mut Value,
    defs: &HashMap<String, Value>,
    visiting: &mut HashSet<String>,
    depth: usize,
) {
    if depth > MAX_INLINE_DEPTH {
        *value = json!({});
        return;
    }

    // Only process objects
    let Some(map) = value.as_object_mut() else {
        // Also handle arrays (e.g., items in `enum`)
        if let Some(arr) = value.as_array_mut() {
            for item in arr.iter_mut() {
                resolve_node(item, defs, visiting, depth);
            }
        }
        return;
    };

    // --- Step 1: Inline $ref ---
    if let Some(ref_str) = map.get("$ref").and_then(|r| r.as_str()).map(String::from) {
        if let Some(name) = ref_str.strip_prefix("#/$defs/") {
            if visiting.contains(name) {
                // Circular ref — use permissive schema
                *value = json!({});
                return;
            }
            if let Some(def) = defs.get(name).cloned() {
                visiting.insert(name.to_string());
                let mut inlined = def;
                resolve_node(&mut inlined, defs, visiting, depth + 1);
                visiting.remove(name);
                *value = inlined;
                return;
            }
        }
        // Unknown ref — use permissive schema
        *value = json!({});
        return;
    }

    // --- Step 2: Flatten oneOf / anyOf tagged unions ---
    // Important: resolve only $refs in variants first (don't clean keywords yet),
    // because flatten_tagged_union needs `const` values to detect discriminators.
    for keyword in ["oneOf", "anyOf"] {
        if let Some(variants_val) = map.remove(keyword) {
            if let Some(variants) = variants_val.as_array() {
                // Inline $refs in variants but preserve const/keywords
                let mut resolved: Vec<Value> = variants.clone();
                for v in &mut resolved {
                    inline_refs(v, defs, visiting, depth + 1);
                }

                if let Some(mut flattened) = flatten_tagged_union(&resolved) {
                    // Now fully resolve the flattened result (clean keywords, recurse)
                    resolve_node(&mut flattened, defs, visiting, depth);
                    *value = flattened;
                    return;
                }

                // Not a tagged union — try to pick the most descriptive variant
                // or merge const string values into a string enum.
                // IMPORTANT: call pick_best_variant BEFORE resolve_node, because
                // resolve_node removes `const` which we need for enum detection.
                if let Some(mut picked) = pick_best_variant(&resolved) {
                    resolve_node(&mut picked, defs, visiting, depth);
                    *value = picked;
                    return;
                }

                // Last resort: fully resolve and try again
                let mut fully_resolved: Vec<Value> = resolved;
                for v in &mut fully_resolved {
                    resolve_node(v, defs, visiting, depth + 1);
                }
                if let Some(picked) = pick_best_variant(&fully_resolved) {
                    *value = picked;
                    return;
                }
            }
            // Could not flatten — use permissive schema
            *value = json!({});
            return;
        }
    }

    // Handle allOf — merge into single object
    if let Some(all_of_val) = map.remove("allOf") {
        if let Some(variants) = all_of_val.as_array() {
            let mut resolved: Vec<Value> = variants.clone();
            for v in &mut resolved {
                resolve_node(v, defs, visiting, depth + 1);
            }
            if let Some(merged) = merge_all_of(&resolved) {
                *value = merged;
                return;
            }
        }
        *value = json!({});
        return;
    }

    // --- Step 3: Remove unsupported keywords ---
    map.remove("const");
    map.remove("$defs");
    map.remove("default");
    map.remove("examples");
    map.remove("not");
    map.remove("if");
    map.remove("then");
    map.remove("else");
    map.remove("patternProperties");
    map.remove("dependentRequired");
    map.remove("dependentSchemas");

    // --- Step 4: Recurse into nested schemas ---
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if let Some(val) = map.get_mut(&key) {
            match val {
                Value::Object(_) => resolve_node(val, defs, visiting, depth),
                Value::Array(arr) => {
                    for item in arr.iter_mut() {
                        if item.is_object() {
                            resolve_node(item, defs, visiting, depth);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Inline `$ref` references only — no keyword cleaning or oneOf flattening.
/// Used to prepare oneOf variants for discriminator detection (which needs `const`).
pub(crate) fn inline_refs(
    value: &mut Value,
    defs: &HashMap<String, Value>,
    visiting: &mut HashSet<String>,
    depth: usize,
) {
    if depth > MAX_INLINE_DEPTH {
        *value = json!({});
        return;
    }

    match value {
        Value::Object(map) => {
            // Inline $ref
            if let Some(ref_str) = map.get("$ref").and_then(|r| r.as_str()).map(String::from) {
                if let Some(name) = ref_str.strip_prefix("#/$defs/") {
                    if visiting.contains(name) {
                        *value = json!({});
                        return;
                    }
                    if let Some(def) = defs.get(name).cloned() {
                        visiting.insert(name.to_string());
                        let mut inlined = def;
                        inline_refs(&mut inlined, defs, visiting, depth + 1);
                        visiting.remove(name);
                        *value = inlined;
                        return;
                    }
                }
                *value = json!({});
                return;
            }

            // Recurse into values
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(val) = map.get_mut(&key) {
                    inline_refs(val, defs, visiting, depth);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                inline_refs(item, defs, visiting, depth);
            }
        }
        _ => {}
    }
}
