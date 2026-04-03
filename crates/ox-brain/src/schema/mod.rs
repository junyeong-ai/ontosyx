//! JSON Schema transforms for LLM structured output.
//!
//! Handles provider-specific schema requirements:
//! - Anthropic/Bedrock: no `$ref`, `$defs`, `oneOf`, `anyOf`, `allOf`, `const`
//! - OpenAI: `response_format` with `type: "json_schema"`, supports oneOf/anyOf/allOf
//! - All providers: requires `additionalProperties: false` on objects
//!
//! The main entry point [`transform_for_structured_output`] converts a schemars-generated
//! schema into a fully-inlined, flattened form compatible with Anthropic/Bedrock.

mod diagnostics;
mod inline;
mod merge;
mod strict;
mod union;

use serde_json::Value;
use std::collections::{HashMap, HashSet};

use inline::resolve_node;

// --- Public re-exports ---

pub use diagnostics::{
    count_optional_params, count_total_properties, has_circular_refs, has_unsupported_composition,
};
pub use strict::{clean_nullable_flags, enforce_strict_object_schemas};

/// Maximum depth for inlining `$ref` — prevents infinite expansion on cycles.
const MAX_INLINE_DEPTH: usize = 10;

/// Transform a schemars-generated schema into Anthropic/Bedrock-compatible form.
///
/// After this transform, the schema contains no `$ref`, `$defs`, `oneOf`, `anyOf`,
/// `allOf`, or `const` — only plain types, properties, enums, and arrays.
pub fn transform_for_structured_output(schema: &Value) -> Value {
    let defs = collect_defs(schema);
    let mut result = schema.clone();
    let mut visiting = HashSet::new();
    resolve_node(&mut result, &defs, &mut visiting, 0);
    // Remove top-level $defs (everything is now inlined)
    if let Some(map) = result.as_object_mut() {
        map.remove("$defs");
    }
    result
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
    use diagnostics::collect_refs;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // enforce_strict_object_schemas
    // -----------------------------------------------------------------------

    #[test]
    fn adds_additional_properties_false_to_object_with_properties() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });
        enforce_strict_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn preserves_existing_additional_properties() {
        let mut schema = json!({
            "type": "object",
            "properties": { "x": { "type": "integer" } },
            "additionalProperties": true
        });
        enforce_strict_object_schemas(&mut schema);
        // Should NOT overwrite existing value
        assert_eq!(schema["additionalProperties"], json!(true));
    }

    #[test]
    fn adds_additional_properties_to_bare_object() {
        // Bedrock requires additionalProperties: false on ALL object types
        let mut schema = json!({ "type": "object" });
        enforce_strict_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn strips_unsupported_validation_keywords() {
        let mut schema = json!({
            "type": "integer",
            "minimum": 0,
            "maximum": 100,
            "exclusiveMinimum": 0,
            "multipleOf": 5
        });
        enforce_strict_object_schemas(&mut schema);
        assert!(schema.get("minimum").is_none());
        assert!(schema.get("maximum").is_none());
        assert!(schema.get("exclusiveMinimum").is_none());
        assert!(schema.get("multipleOf").is_none());
        // Core type should remain
        assert_eq!(schema["type"], json!("integer"));
    }

    #[test]
    fn strips_string_validation_keywords() {
        let mut schema = json!({
            "type": "string",
            "minLength": 1,
            "maxLength": 255
        });
        enforce_strict_object_schemas(&mut schema);
        assert!(schema.get("minLength").is_none());
        assert!(schema.get("maxLength").is_none());
    }

    #[test]
    fn strips_array_validation_keywords() {
        let mut schema = json!({
            "type": "array",
            "items": { "type": "string", "minLength": 1 },
            "minItems": 1,
            "maxItems": 10,
            "uniqueItems": true
        });
        enforce_strict_object_schemas(&mut schema);
        assert!(schema.get("minItems").is_none());
        assert!(schema.get("maxItems").is_none());
        assert!(schema.get("uniqueItems").is_none());
        // Also strips from nested items
        assert!(schema["items"].get("minLength").is_none());
    }

    #[test]
    fn recurses_into_nested_properties() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                }
            }
        });
        enforce_strict_object_schemas(&mut schema);
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(
            schema["properties"]["address"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn recurses_into_defs() {
        let mut schema = json!({
            "type": "object",
            "properties": { "ref": { "$ref": "#/$defs/Inner" } },
            "$defs": {
                "Inner": {
                    "type": "object",
                    "properties": {
                        "value": { "type": "integer", "minimum": 0 }
                    }
                }
            }
        });
        enforce_strict_object_schemas(&mut schema);
        let inner = &schema["$defs"]["Inner"];
        assert_eq!(inner["additionalProperties"], json!(false));
        assert!(inner["properties"]["value"].get("minimum").is_none());
    }

    #[test]
    fn recurses_into_any_of() {
        let mut schema = json!({
            "anyOf": [
                {
                    "type": "object",
                    "properties": { "a": { "type": "string" } }
                },
                { "type": "string", "minLength": 1 }
            ]
        });
        enforce_strict_object_schemas(&mut schema);
        assert_eq!(schema["anyOf"][0]["additionalProperties"], json!(false));
        assert!(schema["anyOf"][1].get("minLength").is_none());
    }

    #[test]
    fn handles_non_object_value_gracefully() {
        let mut schema = json!("string");
        enforce_strict_object_schemas(&mut schema); // should not panic
        assert_eq!(schema, json!("string"));
    }

    // -----------------------------------------------------------------------
    // has_circular_refs
    // -----------------------------------------------------------------------

    #[test]
    fn no_defs_means_no_cycles() {
        let schema = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } }
        });
        assert!(!has_circular_refs(&schema));
    }

    #[test]
    fn non_circular_defs() {
        let schema = json!({
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                },
                "Person": {
                    "type": "object",
                    "properties": {
                        "home": { "$ref": "#/$defs/Address" }
                    }
                }
            }
        });
        assert!(!has_circular_refs(&schema));
    }

    #[test]
    fn direct_self_reference() {
        let schema = json!({
            "$defs": {
                "TreeNode": {
                    "type": "object",
                    "properties": {
                        "children": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/TreeNode" }
                        }
                    }
                }
            }
        });
        assert!(has_circular_refs(&schema));
    }

    #[test]
    fn mutual_reference_cycle() {
        // A -> B -> A
        let schema = json!({
            "$defs": {
                "A": {
                    "type": "object",
                    "properties": {
                        "b": { "$ref": "#/$defs/B" }
                    }
                },
                "B": {
                    "type": "object",
                    "properties": {
                        "a": { "$ref": "#/$defs/A" }
                    }
                }
            }
        });
        assert!(has_circular_refs(&schema));
    }

    #[test]
    fn three_node_cycle() {
        // A -> B -> C -> A
        let schema = json!({
            "$defs": {
                "A": {
                    "properties": { "next": { "$ref": "#/$defs/B" } }
                },
                "B": {
                    "properties": { "next": { "$ref": "#/$defs/C" } }
                },
                "C": {
                    "properties": { "next": { "$ref": "#/$defs/A" } }
                }
            }
        });
        assert!(has_circular_refs(&schema));
    }

    #[test]
    fn diamond_dependency_no_cycle() {
        // A -> B, A -> C, B -> D, C -> D (no cycle)
        let schema = json!({
            "$defs": {
                "A": {
                    "properties": {
                        "b": { "$ref": "#/$defs/B" },
                        "c": { "$ref": "#/$defs/C" }
                    }
                },
                "B": {
                    "properties": { "d": { "$ref": "#/$defs/D" } }
                },
                "C": {
                    "properties": { "d": { "$ref": "#/$defs/D" } }
                },
                "D": {
                    "type": "string"
                }
            }
        });
        assert!(!has_circular_refs(&schema));
    }

    #[test]
    fn ref_nested_in_any_of() {
        let schema = json!({
            "$defs": {
                "Expr": {
                    "anyOf": [
                        { "type": "string" },
                        {
                            "type": "object",
                            "properties": {
                                "inner": { "$ref": "#/$defs/Expr" }
                            }
                        }
                    ]
                }
            }
        });
        assert!(has_circular_refs(&schema));
    }

    // -----------------------------------------------------------------------
    // collect_refs (tested indirectly, but verify edge cases)
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Real IR type schemas — verify no circular refs after custom JsonSchema
    // -----------------------------------------------------------------------

    #[test]
    fn ontology_ir_schema_has_no_circular_refs() {
        let schema = schemars::schema_for!(ox_core::ontology_ir::OntologyIR);
        let value = schema.to_value();
        assert!(
            !has_circular_refs(&value),
            "OntologyIR schema should NOT have circular refs"
        );
    }

    #[test]
    fn query_ir_schema_has_circular_refs() {
        // QueryIR is expected to have circular refs: GraphPattern → Expr → Exists → GraphPattern
        let schema = schemars::schema_for!(ox_core::query_ir::QueryIR);
        let value = schema.to_value();
        assert!(
            has_circular_refs(&value),
            "QueryIR schema should have circular refs (Expr → Exists → GraphPattern → Expr)"
        );
    }

    #[test]
    fn collect_refs_ignores_external_refs() {
        let value = json!({
            "$ref": "https://example.com/schema.json"
        });
        let mut refs = HashSet::new();
        collect_refs(&value, &mut refs);
        assert!(refs.is_empty());
    }

    #[test]
    fn collect_refs_finds_deeply_nested() {
        let value = json!({
            "type": "object",
            "properties": {
                "a": {
                    "type": "array",
                    "items": {
                        "anyOf": [
                            { "$ref": "#/$defs/Foo" },
                            { "$ref": "#/$defs/Bar" }
                        ]
                    }
                }
            }
        });
        let mut refs = HashSet::new();
        collect_refs(&value, &mut refs);
        assert!(refs.contains("Foo"));
        assert!(refs.contains("Bar"));
        assert_eq!(refs.len(), 2);
    }

    // -----------------------------------------------------------------------
    // transform_for_structured_output
    // -----------------------------------------------------------------------

    #[test]
    fn transform_inlines_simple_ref() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "$ref": "#/$defs/Name" }
            },
            "$defs": {
                "Name": { "type": "string" }
            }
        });
        let result = transform_for_structured_output(&schema);
        // $ref should be inlined
        assert_eq!(result["properties"]["name"]["type"], "string");
        // $defs should be removed
        assert!(result.get("$defs").is_none());
    }

    #[test]
    fn transform_flattens_tagged_union() {
        let schema = json!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "kind": { "const": "circle" },
                        "radius": { "type": "number" }
                    },
                    "required": ["kind", "radius"]
                },
                {
                    "type": "object",
                    "properties": {
                        "kind": { "const": "rect" },
                        "width": { "type": "number" },
                        "height": { "type": "number" }
                    },
                    "required": ["kind", "width", "height"]
                }
            ]
        });
        let result = transform_for_structured_output(&schema);
        // Should be flattened to single object with enum discriminator
        assert_eq!(result["type"], "object");
        assert_eq!(result["properties"]["kind"]["type"], "string");
        assert_eq!(
            result["properties"]["kind"]["enum"],
            json!(["circle", "rect"])
        );
        // Variant-specific props should be nullable
        assert!(
            result["properties"]["radius"]["type"]
                .as_array()
                .unwrap()
                .contains(&json!("null"))
        );
        assert!(
            result["properties"]["width"]["type"]
                .as_array()
                .unwrap()
                .contains(&json!("null"))
        );
        assert_eq!(result["additionalProperties"], false);
    }

    #[test]
    fn transform_breaks_circular_refs() {
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
        let result = transform_for_structured_output(&schema);
        // First level should be inlined
        assert_eq!(result["properties"]["root"]["type"], "object");
        assert_eq!(
            result["properties"]["root"]["properties"]["value"]["type"],
            "string"
        );
        // Circular ref should be replaced with permissive schema
        assert_eq!(
            result["properties"]["root"]["properties"]["child"],
            json!({})
        );
        assert!(result.get("$defs").is_none());
    }

    #[test]
    fn transform_removes_const_keyword() {
        let schema = json!({
            "type": "object",
            "properties": {
                "tag": { "const": "fixed_value" },
                "data": { "type": "string" }
            }
        });
        let result = transform_for_structured_output(&schema);
        assert!(result["properties"]["tag"].get("const").is_none());
    }

    #[test]
    fn transform_query_ir_produces_compatible_schema() {
        let schema = schemars::schema_for!(ox_core::query_ir::QueryIR);
        let value = schema.to_value();

        // Before transform: has $defs, oneOf, circular refs
        assert!(has_circular_refs(&value));
        assert!(has_unsupported_composition(&value));
        assert!(value.get("$defs").is_some());

        // After transform: no $defs, no oneOf, no $ref
        let transformed = transform_for_structured_output(&value);
        assert!(transformed.get("$defs").is_none(), "Should have no $defs");
        assert!(
            !has_unsupported_composition(&transformed),
            "Should have no oneOf/anyOf/allOf"
        );

        // Verify no $ref remains anywhere
        fn has_ref(v: &Value) -> bool {
            match v {
                Value::Object(map) => map.contains_key("$ref") || map.values().any(has_ref),
                Value::Array(arr) => arr.iter().any(has_ref),
                _ => false,
            }
        }
        assert!(!has_ref(&transformed), "Should have no $ref");
    }

    #[test]
    fn transform_ontology_ir_produces_compatible_schema() {
        let schema = schemars::schema_for!(ox_core::ontology_ir::OntologyIR);
        let value = schema.to_value();
        let transformed = transform_for_structured_output(&value);

        assert!(transformed.get("$defs").is_none());
        assert!(!has_unsupported_composition(&transformed));

        fn has_ref(v: &Value) -> bool {
            match v {
                Value::Object(map) => map.contains_key("$ref") || map.values().any(has_ref),
                Value::Array(arr) => arr.iter().any(has_ref),
                _ => false,
            }
        }
        assert!(!has_ref(&transformed));
    }

    #[test]
    fn load_plan_schema_within_optional_params_limit() {
        let schema = schemars::schema_for!(ox_core::load_plan::LoadPlan);
        let value = schema.to_value();
        let mut transformed = transform_for_structured_output(&value);
        enforce_strict_object_schemas(&mut transformed);
        clean_nullable_flags(&mut transformed);
        let count = count_optional_params(&transformed);
        let total = count_total_properties(&transformed);
        eprintln!("LoadPlan optional params: {count}, total properties: {total}");
        assert!(
            count <= 24,
            "LoadPlan has {count} optional params (limit 24)"
        );
    }

    #[test]
    fn match_query_ir_within_structured_output_limits() {
        let schema = schemars::schema_for!(ox_core::match_query_ir::MatchQueryIR);
        let value = schema.to_value();
        let mut transformed = transform_for_structured_output(&value);
        enforce_strict_object_schemas(&mut transformed);
        clean_nullable_flags(&mut transformed);
        let optional = count_optional_params(&transformed);
        let total = count_total_properties(&transformed);
        eprintln!("MatchQueryIR optional params: {optional}, total properties: {total}");
        assert!(
            optional <= 24,
            "MatchQueryIR has {optional} optional params (limit 24)"
        );
        assert!(
            total <= 50,
            "MatchQueryIR has {total} total properties (limit 50)"
        );
    }

    #[test]
    fn transform_load_plan_schema_preserves_enums() {
        let schema = schemars::schema_for!(ox_core::load_plan::LoadPlan);
        let value = schema.to_value();
        let mut transformed = transform_for_structured_output(&value);
        enforce_strict_object_schemas(&mut transformed);
        clean_nullable_flags(&mut transformed);
        let schema_str = serde_json::to_string_pretty(&transformed).unwrap();

        // ConflictStrategy should be a string enum, not an empty object
        assert!(
            schema_str.contains("\"update\""),
            "Schema should contain ConflictStrategy enum values.\nSchema: {schema_str}"
        );

        // No empty schemas {} allowed (Bedrock rejects them)
        fn has_empty_schema(v: &Value) -> bool {
            match v {
                Value::Object(map) => {
                    if map.is_empty() {
                        return true;
                    }
                    map.values().any(has_empty_schema)
                }
                Value::Array(arr) => arr.iter().any(has_empty_schema),
                _ => false,
            }
        }
        assert!(
            !has_empty_schema(&transformed),
            "Schema should have no empty {{}} schemas"
        );
    }
}

    #[test]
    fn measure_query_ir_complexity() {
        let schema = schemars::schema_for!(ox_core::query_ir::QueryIR);
        let mut value = schema.to_value();
        
        let optional_before = count_optional_params(&value);
        let total_before = count_total_properties(&value);
        eprintln!("BEFORE transform: optional={}, total={}", optional_before, total_before);
        
        // Transform
        value = transform_for_structured_output(&value);
        enforce_strict_object_schemas(&mut value);
        clean_nullable_flags(&mut value);
        
        let optional_after = count_optional_params(&value);
        let total_after = count_total_properties(&value);
        eprintln!("AFTER transform: optional={}, total={}", optional_after, total_after);
        
        // Show first few properties at root level
        if let Some(props) = value.get("properties").and_then(|p| p.as_object()) {
            eprintln!("Root properties ({}): {:?}", props.len(), props.keys().collect::<Vec<_>>());
        }
        
        // Dump full schema for analysis
        let schema_str = serde_json::to_string_pretty(&value).unwrap();
        eprintln!("Total schema size: {} chars", schema_str.len());
    }
