use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ontology_ir::*;
use crate::types::{PropertyType, PropertyValue, deserialize_optional_property_value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn default_format_version() -> u32 {
    1
}

pub(crate) fn default_cardinality() -> Cardinality {
    Cardinality::ManyToMany
}

pub(crate) fn gen_id() -> String {
    Uuid::new_v4().to_string()
}

/// Ensure an id is present: use existing or generate a new UUID.
pub(crate) fn ensure_id(id: Option<String>) -> String {
    id.unwrap_or_else(gen_id)
}

// ---------------------------------------------------------------------------
// OntologyInputIR — LLM output / external JSON / file upload
// ---------------------------------------------------------------------------

/// LLM output / external JSON / legacy load. Label/name based references.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OntologyInputIR {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub version: u32,
    pub node_types: Vec<InputNodeTypeDef>,
    #[serde(default)]
    pub edge_types: Vec<InputEdgeTypeDef>,
    #[serde(default, deserialize_with = "deserialize_indexes_tolerant")]
    pub indexes: Vec<InputIndexDef>,
}

/// Deserialize indexes tolerantly: skip individual items that fail to parse.
/// LLMs frequently generate unsupported index types (e.g., "range") or omit required
/// fields. Failing the entire ontology for a malformed index is disproportionate.
fn deserialize_indexes_tolerant<'de, D>(deserializer: D) -> Result<Vec<InputIndexDef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<InputIndexDef>(v) {
            Ok(idx) => Some(idx),
            Err(e) => {
                tracing::warn!(error = %e, "Skipping malformed index definition from LLM output");
                None
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// InputNodeTypeDef
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct InputNodeTypeDef {
    pub id: Option<String>,
    pub label: String,
    pub description: Option<String>,
    /// Source table name this node was derived from (e.g., "products").
    /// Extracted into SourceMapping during normalization; not stored on OntologyIR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_table: Option<String>,
    #[serde(default)]
    pub properties: Vec<InputPropertyDef>,
    #[serde(default)]
    pub constraints: Vec<InputNodeConstraint>,
}

// ---------------------------------------------------------------------------
// InputEdgeTypeDef
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct InputEdgeTypeDef {
    pub id: Option<String>,
    pub label: String,
    pub description: Option<String>,
    /// Source node type label or ID (resolved to node UUID during normalize)
    #[serde(alias = "source_node_id")]
    pub source_type: String,
    /// Target node type label or ID (resolved to node UUID during normalize)
    #[serde(alias = "target_node_id")]
    pub target_type: String,
    #[serde(default)]
    pub properties: Vec<InputPropertyDef>,
    #[serde(default = "default_cardinality")]
    pub cardinality: Cardinality,
}

// ---------------------------------------------------------------------------
// InputPropertyDef
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct InputPropertyDef {
    pub id: Option<String>,
    pub name: String,
    pub property_type: PropertyType,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default, deserialize_with = "deserialize_optional_property_value")]
    pub default_value: Option<PropertyValue>,
    pub description: Option<String>,
    /// Source column name this property was derived from (e.g., "cust_nm").
    /// Extracted into SourceMapping during normalization; not stored on OntologyIR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_column: Option<String>,
}

// ---------------------------------------------------------------------------
// InputNodeConstraint — uses property NAMES (not IDs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputNodeConstraint {
    Unique {
        id: Option<String>,
        #[serde(alias = "property_ids")]
        properties: Vec<String>,
    },
    Exists {
        id: Option<String>,
        #[serde(alias = "property_id")]
        property: String,
    },
    NodeKey {
        id: Option<String>,
        #[serde(alias = "property_ids")]
        properties: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// InputIndexDef — uses label/property NAMES (not IDs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputIndexDef {
    Single {
        id: Option<String>,
        label: String,
        property: String,
    },
    Composite {
        id: Option<String>,
        label: String,
        properties: Vec<String>,
    },
    FullText {
        id: Option<String>,
        name: String,
        label: String,
        properties: Vec<String>,
    },
    Vector {
        id: Option<String>,
        label: String,
        property: String,
        dimensions: usize,
        similarity: VectorSimilarity,
    },
}

/// Custom deserializer: accepts standard tagged format AND LLM-generated `"index"` variant.
/// LLMs often generate `{"type": "index", "node_type": "Foo", "properties": ["bar"]}`
/// instead of `{"type": "single", "label": "Foo", "property": "bar"}`.
impl<'de> Deserialize<'de> for InputIndexDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| de::Error::custom("expected object for InputIndexDef"))?;

        let type_str = obj.get("type").and_then(|t| t.as_str()).unwrap_or("index");

        // Extract optional id
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Resolve label: accept "label", "node_type", or "node_label" (LLM variants)
        let label = obj
            .get("label")
            .or_else(|| obj.get("node_type"))
            .or_else(|| obj.get("node_label"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match type_str {
            "single" => {
                let label = label.ok_or_else(|| de::Error::missing_field("label"))?;
                let property = obj
                    .get("property")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| de::Error::missing_field("property"))?;
                Ok(InputIndexDef::Single {
                    id,
                    label,
                    property,
                })
            }
            "composite" => {
                let label = label.ok_or_else(|| de::Error::missing_field("label"))?;
                let properties = obj
                    .get("properties")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .ok_or_else(|| de::Error::missing_field("properties"))?;
                Ok(InputIndexDef::Composite {
                    id,
                    label,
                    properties,
                })
            }
            "full_text" | "fulltext" => {
                let label = label.ok_or_else(|| de::Error::missing_field("label"))?;
                let name = obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("ft_{}", label.to_lowercase()));
                let properties = obj
                    .get("properties")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .ok_or_else(|| de::Error::missing_field("properties"))?;
                Ok(InputIndexDef::FullText {
                    id,
                    name,
                    label,
                    properties,
                })
            }
            "vector" => {
                let label = label.ok_or_else(|| de::Error::missing_field("label"))?;
                let property = obj
                    .get("property")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| de::Error::missing_field("property"))?;
                let dimensions = obj
                    .get("dimensions")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .ok_or_else(|| de::Error::missing_field("dimensions"))?;
                let similarity = serde_json::from_value(
                    obj.get("similarity")
                        .cloned()
                        .unwrap_or(serde_json::Value::String("cosine".to_string())),
                )
                .map_err(de::Error::custom)?;
                Ok(InputIndexDef::Vector {
                    id,
                    label,
                    property,
                    dimensions,
                    similarity,
                })
            }
            // LLM-generated generic/unknown index — infer Single vs Composite
            _ => {
                let label = label.ok_or_else(|| de::Error::missing_field("label or node_type"))?;
                if let Some(prop) = obj.get("property").and_then(|v| v.as_str()) {
                    Ok(InputIndexDef::Single {
                        id,
                        label,
                        property: prop.to_string(),
                    })
                } else if let Some(props) = obj.get("properties").and_then(|v| v.as_array()) {
                    let properties: Vec<String> = props
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    match properties.len() {
                        0 => Err(de::Error::custom(format!(
                            "InputIndexDef: empty properties array for '{type_str}'"
                        ))),
                        1 => Ok(InputIndexDef::Single {
                            id,
                            label,
                            property: properties.into_iter().next().expect("len checked"),
                        }),
                        _ => Ok(InputIndexDef::Composite {
                            id,
                            label,
                            properties,
                        }),
                    }
                } else {
                    Err(de::Error::custom(format!(
                        "InputIndexDef: cannot determine index type for '{type_str}'"
                    )))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_defaults_to_1() {
        let json = r#"{
            "name": "Minimal",
            "version": 1,
            "node_types": [{
                "label": "Thing",
                "properties": [{"name": "id", "property_type": "string"}]
            }]
        }"#;

        let input: OntologyInputIR = serde_json::from_str(json).expect("parse");
        assert_eq!(input.format_version, 1);
        assert!(input.id.is_none());
    }

    // -- InputIndexDef LLM-tolerant deserialization --------------------------

    #[test]
    fn index_def_deserialize_generic_index_type() {
        let json = r#"{"type": "index", "node_type": "User", "property": "email"}"#;
        let idx: InputIndexDef = serde_json::from_str(json).expect("parse");
        assert!(matches!(
            idx,
            InputIndexDef::Single {
                label,
                property,
                ..
            } if label == "User" && property == "email"
        ));
    }

    #[test]
    fn index_def_deserialize_generic_with_multiple_properties() {
        let json = r#"{"type": "index", "node_type": "User", "properties": ["email", "name"]}"#;
        let idx: InputIndexDef = serde_json::from_str(json).expect("parse");
        assert!(matches!(
            idx,
            InputIndexDef::Composite {
                label,
                properties,
                ..
            } if label == "User" && properties == vec!["email", "name"]
        ));
    }

    #[test]
    fn index_def_deserialize_fulltext_variants() {
        let json =
            r#"{"type": "fulltext", "label": "Product", "properties": ["name", "description"]}"#;
        let idx: InputIndexDef = serde_json::from_str(json).expect("parse");
        assert!(matches!(
            idx,
            InputIndexDef::FullText {
                name,
                label,
                properties,
                ..
            } if label == "Product" && name == "ft_product" && properties.len() == 2
        ));
    }

    #[test]
    fn index_def_preserves_id() {
        let json = r#"{"type": "single", "id": "idx-001", "label": "User", "property": "email"}"#;
        let idx: InputIndexDef = serde_json::from_str(json).expect("parse");
        assert!(matches!(
            idx,
            InputIndexDef::Single {
                id: Some(ref id),
                ..
            } if id == "idx-001"
        ));
    }

    // -- deserialize_optional_property_value ---------------------------------

    #[test]
    fn property_def_default_value_tolerates_null_and_empty() {
        let json = r#"{"name": "x", "property_type": "string", "default_value": null}"#;
        let p: InputPropertyDef = serde_json::from_str(json).expect("parse");
        assert!(p.default_value.is_none());

        let json2 = r#"{"name": "x", "property_type": "string", "default_value": {}}"#;
        let p2: InputPropertyDef = serde_json::from_str(json2).expect("parse");
        assert!(p2.default_value.is_none());

        let json3 = r#"{"name": "x", "property_type": "string"}"#;
        let p3: InputPropertyDef = serde_json::from_str(json3).expect("parse");
        assert!(p3.default_value.is_none());
    }
}
