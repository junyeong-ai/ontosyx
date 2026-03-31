//! Schema merging and composition logic (allOf handling).

use serde_json::{Value, json};

/// Merge allOf schemas into a single object.
pub(crate) fn merge_all_of(schemas: &[Value]) -> Option<Value> {
    let mut merged_props = serde_json::Map::new();
    let mut merged_required = Vec::new();

    for schema in schemas {
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            for (key, val) in props {
                merged_props.insert(key.clone(), val.clone());
            }
        }
        if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
            for r in req {
                if !merged_required.contains(r) {
                    merged_required.push(r.clone());
                }
            }
        }
    }

    if merged_props.is_empty() {
        return None;
    }

    Some(json!({
        "type": "object",
        "properties": merged_props,
        "required": merged_required,
        "additionalProperties": false
    }))
}
