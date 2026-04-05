use std::cell::RefCell;
use std::collections::HashMap;

use serde::Serialize;
use tracing::warn;

use crate::ontology_ir::*;
use crate::source_mapping::SourceMapping;

use super::dtos::*;

// ---------------------------------------------------------------------------
// NormalizeResult — normalize() output with structured warnings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct NormalizeWarning {
    pub kind: String,
    pub message: String,
}

#[derive(Debug)]
pub struct NormalizeResult {
    pub ontology: OntologyIR,
    pub source_mapping: SourceMapping,
    pub warnings: Vec<NormalizeWarning>,
}

// ---------------------------------------------------------------------------
// String distance for fuzzy property name matching
// ---------------------------------------------------------------------------

/// Simple Levenshtein distance for fuzzy property name matching.
fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

// ---------------------------------------------------------------------------
// normalize() — Input DTO → Canonical model
// ---------------------------------------------------------------------------

/// Input DTO → Canonical model + SourceMapping. The single ingress path.
/// 1. Assign UUID to any element missing an id
/// 2. Extract source_table/source_column into SourceMapping
/// 3. Resolve edge source_type/target_type (label) → source_node_id/target_node_id (UUID)
/// 4. Resolve constraint properties (name) → property_ids (UUID)
/// 5. Resolve index label/property (name) → node_id/property_id (UUID)
///
/// Errors: immediately fails on unknown label/name references.
pub fn normalize(input: OntologyInputIR) -> Result<NormalizeResult, Vec<String>> {
    let warnings: RefCell<Vec<NormalizeWarning>> = RefCell::new(Vec::new());

    macro_rules! norm_warn {
        ($kind:expr, $msg:expr) => {{
            warn!("{}", $msg);
            warnings.borrow_mut().push(NormalizeWarning {
                kind: $kind.to_string(),
                message: $msg.to_string(),
            });
        }};
    }

    let ontology_id = ensure_id(input.id);

    // -- Node types ----------------------------------------------------------
    // label → node_id mapping for edge resolution
    let mut label_to_node_id: HashMap<String, NodeTypeId> = HashMap::new();
    // (node_label, property_name) → property_id mapping for constraint/index resolution
    // Also: node_label → Vec<(prop_name, prop_id)> for ordered lookup
    let mut node_prop_map: HashMap<String, Vec<(String, PropertyId)>> = HashMap::new();

    let mut node_types = Vec::with_capacity(input.node_types.len());
    let mut source_mapping = SourceMapping::new();

    for input_node in input.node_types {
        let node_id: NodeTypeId = ensure_id(input_node.id).into();
        label_to_node_id.insert(input_node.label.clone(), node_id.clone());

        // Extract source_table into SourceMapping
        if let Some(table) = &input_node.source_table {
            source_mapping
                .node_tables
                .insert(node_id.to_string(), table.clone());
        }

        // Build properties with IDs, track name→id mapping
        let mut prop_name_to_id: Vec<(String, PropertyId)> = Vec::new();
        let properties: Vec<PropertyDef> = input_node
            .properties
            .iter()
            .map(|p| {
                let prop_id: PropertyId = ensure_id(p.id.clone()).into();
                prop_name_to_id.push((p.name.clone(), prop_id.clone()));
                // Extract source_column into SourceMapping
                if let Some(col) = &p.source_column {
                    source_mapping.set_column(&node_id, &prop_id, col.clone());
                }
                PropertyDef {
                    id: prop_id,
                    name: p.name.clone(),
                    property_type: p.property_type.clone(),
                    nullable: p.nullable,
                    default_value: p.default_value.clone(),
                    description: p.description.clone(),
                    classification: None,
                }
            })
            .collect();

        // Resolve constraints: property names → property IDs
        // Uses exact match first, then fuzzy (Levenshtein ≤ 2) auto-correction.
        // Truly unresolvable references are collected as errors.
        let resolve_prop = |name_or_id: &str| -> Option<PropertyId> {
            // Exact match by name
            if let Some((_, id)) = prop_name_to_id.iter().find(|(n, _)| n == name_or_id) {
                return Some(id.clone());
            }
            // Exact match by ID
            if let Some((_, id)) = prop_name_to_id
                .iter()
                .find(|(_, id)| id.as_ref() == name_or_id)
            {
                return Some(id.clone());
            }
            None
        };

        let fuzzy_resolve_prop = |name_or_id: &str| -> Option<(String, PropertyId)> {
            // Don't fuzzy-match short names — "id" matching "ip" is dangerous
            if name_or_id.len() < 4 {
                return None;
            }
            let lower = name_or_id.to_lowercase();
            let candidates: Vec<_> = prop_name_to_id
                .iter()
                .filter(|(n, _)| {
                    let dist = levenshtein(&lower, &n.to_lowercase());
                    dist > 0 && dist <= 2
                })
                .collect();
            // Only auto-correct when exactly ONE unambiguous match exists
            if candidates.len() == 1 {
                let (matched, id) = candidates[0];
                Some((matched.clone(), id.clone()))
            } else {
                None // Ambiguous or no match — don't guess
            }
        };

        let constraints: Vec<ConstraintDef> = input_node
            .constraints
            .iter()
            .filter_map(|c| {
                match c {
                    InputNodeConstraint::Unique { id, properties } => {
                        let mut property_ids = Vec::new();
                        let mut unresolved = Vec::new();
                        for p in properties {
                            if let Some(pid) = resolve_prop(p) {
                                property_ids.push(pid);
                            } else if let Some((matched, mid)) = fuzzy_resolve_prop(p) {
                                norm_warn!("auto_corrected", format!("Node '{}': auto-corrected constraint property '{}' → '{}'", input_node.label, p, matched));
                                property_ids.push(mid);
                            } else {
                                unresolved.push(p.clone());
                            }
                        }
                        if property_ids.is_empty() {
                            norm_warn!("dropped_constraint", format!("Node '{}': dropping unique constraint with unknown properties {:?}", input_node.label, unresolved));
                            None
                        } else {
                            if !unresolved.is_empty() {
                                norm_warn!("partial_constraint", format!("Node '{}': partial constraint, {} resolved, unresolved: {:?}", input_node.label, property_ids.len(), unresolved));
                            }
                            Some(ConstraintDef {
                                id: ensure_id(id.clone()).into(),
                                constraint: NodeConstraint::Unique { property_ids },
                            })
                        }
                    }
                    InputNodeConstraint::Exists { id, property } => {
                        let resolved = resolve_prop(property).or_else(|| {
                            fuzzy_resolve_prop(property).map(|(matched, mid)| {
                                norm_warn!("auto_corrected", format!("Node '{}': auto-corrected exists constraint property '{}' → '{}'", input_node.label, property, matched));
                                mid
                            })
                        });
                        match resolved {
                            Some(property_id) => Some(ConstraintDef {
                                id: ensure_id(id.clone()).into(),
                                constraint: NodeConstraint::Exists { property_id },
                            }),
                            None => {
                                norm_warn!("dropped_constraint", format!("Node '{}': dropping exists constraint with unknown property '{}'", input_node.label, property));
                                None
                            }
                        }
                    }
                    InputNodeConstraint::NodeKey { id, properties } => {
                        let mut property_ids = Vec::new();
                        let mut unresolved = Vec::new();
                        for p in properties {
                            if let Some(pid) = resolve_prop(p) {
                                property_ids.push(pid);
                            } else if let Some((matched, mid)) = fuzzy_resolve_prop(p) {
                                norm_warn!("auto_corrected", format!("Node '{}': auto-corrected constraint property '{}' → '{}'", input_node.label, p, matched));
                                property_ids.push(mid);
                            } else {
                                unresolved.push(p.clone());
                            }
                        }
                        if property_ids.is_empty() {
                            norm_warn!("dropped_constraint", format!("Node '{}': dropping node_key constraint with unknown properties {:?}", input_node.label, unresolved));
                            None
                        } else {
                            if !unresolved.is_empty() {
                                norm_warn!("partial_constraint", format!("Node '{}': partial node_key constraint, {} resolved, unresolved: {:?}", input_node.label, property_ids.len(), unresolved));
                            }
                            Some(ConstraintDef {
                                id: ensure_id(id.clone()).into(),
                                constraint: NodeConstraint::NodeKey { property_ids },
                            })
                        }
                    }
                }
            })
            .collect();

        node_prop_map.insert(input_node.label.clone(), prop_name_to_id);

        node_types.push(NodeTypeDef {
            id: node_id,
            label: input_node.label,
            description: input_node.description,
            source_table: None,
            properties,
            constraints,
        });
    }

    // -- Edge types ----------------------------------------------------------
    // Resolve edges with dangling reference removal: edges referencing unknown
    // node types are dropped with a warning instead of failing the entire normalization.
    let edge_types: Vec<EdgeTypeDef> = input
        .edge_types
        .into_iter()
        .filter_map(|e| {
            let resolve_node = |label_or_id: &str| -> Option<NodeTypeId> {
                label_to_node_id.get(label_or_id).cloned().or_else(|| {
                    label_to_node_id
                        .values()
                        .find(|id| id.as_ref() == label_or_id)
                        .cloned()
                })
            };
            let source_node_id = match resolve_node(&e.source_type) {
                Some(id) => id,
                None => {
                    norm_warn!(
                        "dropped_edge",
                        format!(
                            "Edge '{}': dropping — unknown source node type '{}'",
                            e.label, e.source_type
                        )
                    );
                    return None;
                }
            };
            let target_node_id = match resolve_node(&e.target_type) {
                Some(id) => id,
                None => {
                    norm_warn!(
                        "dropped_edge",
                        format!(
                            "Edge '{}': dropping — unknown target node type '{}'",
                            e.label, e.target_type
                        )
                    );
                    return None;
                }
            };

            let properties = e
                .properties
                .into_iter()
                .map(|p| PropertyDef {
                    id: ensure_id(p.id).into(),
                    name: p.name,
                    property_type: p.property_type,
                    nullable: p.nullable,
                    default_value: p.default_value,
                    description: p.description,
                    classification: None,
                })
                .collect();

            Some(EdgeTypeDef {
                id: ensure_id(e.id).into(),
                label: e.label,
                description: e.description,
                source_node_id,
                target_node_id,
                properties,
                cardinality: e.cardinality,
            })
        })
        .collect();

    // -- Indexes -------------------------------------------------------------
    // Resolve index references. Returns None if the node label is unknown (skip the index).
    let resolve_index = |label: &str,
                         prop_names: &[&str],
                         index_desc: &str|
     -> Option<(NodeTypeId, Vec<PropertyId>)> {
        let node_id = match label_to_node_id.get(label) {
            Some(id) => id.clone(),
            None => {
                norm_warn!(
                    "dropped_index",
                    format!(
                        "Index '{}': dropping — unknown node type '{}'",
                        index_desc, label
                    )
                );
                return None;
            }
        };

        let mut prop_ids = Vec::new();
        if let Some(props) = node_prop_map.get(label) {
            for pn in prop_names {
                match props.iter().find(|(n, _)| n == pn) {
                    Some((_, pid)) => prop_ids.push(pid.clone()),
                    None => {
                        norm_warn!(
                            "dropped_index_property",
                            format!(
                                "Index '{}': dropping property reference '{}.{}'",
                                index_desc, label, pn
                            )
                        );
                    }
                }
            }
        }

        if prop_ids.is_empty() {
            norm_warn!(
                "dropped_index",
                format!(
                    "Index '{}': dropping — no resolvable properties",
                    index_desc
                )
            );
            return None;
        }

        Some((node_id, prop_ids))
    };

    let indexes: Vec<IndexDef> = input
        .indexes
        .into_iter()
        .filter_map(|idx| match idx {
            InputIndexDef::Single {
                id,
                label,
                property,
            } => {
                let (node_id, prop_ids) = resolve_index(
                    &label,
                    &[property.as_str()],
                    &format!("single on {}.{}", label, property),
                )?;
                Some(IndexDef::Single {
                    id: ensure_id(id),
                    node_id,
                    property_id: prop_ids.into_iter().next()?,
                })
            }
            InputIndexDef::Composite {
                id,
                label,
                properties,
            } => {
                let prop_refs: Vec<&str> = properties.iter().map(|s| s.as_str()).collect();
                let (node_id, property_ids) =
                    resolve_index(&label, &prop_refs, &format!("composite on {}", label))?;
                Some(IndexDef::Composite {
                    id: ensure_id(id),
                    node_id,
                    property_ids,
                })
            }
            InputIndexDef::FullText {
                id,
                name,
                label,
                properties,
            } => {
                let prop_refs: Vec<&str> = properties.iter().map(|s| s.as_str()).collect();
                let (node_id, property_ids) =
                    resolve_index(&label, &prop_refs, &format!("fulltext '{}'", name))?;
                Some(IndexDef::FullText {
                    id: ensure_id(id),
                    name,
                    node_id,
                    property_ids,
                })
            }
            InputIndexDef::Vector {
                id,
                label,
                property,
                dimensions,
                similarity,
            } => {
                let (node_id, prop_ids) = resolve_index(
                    &label,
                    &[property.as_str()],
                    &format!("vector on {}.{}", label, property),
                )?;
                Some(IndexDef::Vector {
                    id: ensure_id(id),
                    node_id,
                    property_id: prop_ids.into_iter().next()?,
                    dimensions,
                    similarity,
                })
            }
        })
        .collect();

    let ontology = OntologyIR::new(
        ontology_id,
        input.name,
        input.description,
        input.version,
        node_types,
        edge_types,
        indexes,
    );

    // Run canonical validation and merge any errors
    let validation_errors = ontology.validate();
    if !validation_errors.is_empty() {
        return Err(validation_errors);
    }

    Ok(NormalizeResult {
        ontology,
        source_mapping,
        warnings: warnings.into_inner(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology_input::to_exchange_format;
    use crate::types::PropertyType;
    use uuid::Uuid;

    fn input_property(name: &str) -> InputPropertyDef {
        InputPropertyDef {
            id: None,
            name: name.to_string(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
            source_column: None,
        }
    }

    fn base_input() -> OntologyInputIR {
        OntologyInputIR {
            format_version: 1,
            id: None,
            name: "Test Ontology".to_string(),
            description: Some("A test".to_string()),
            version: 1,
            node_types: vec![
                InputNodeTypeDef {
                    id: None,
                    label: "User".to_string(),
                    description: None,
                    source_table: Some("users".to_string()),
                    properties: vec![input_property("id"), input_property("email")],
                    constraints: vec![
                        InputNodeConstraint::Unique {
                            id: None,
                            properties: vec!["email".to_string()],
                        },
                        InputNodeConstraint::Exists {
                            id: None,
                            property: "id".to_string(),
                        },
                    ],
                },
                InputNodeTypeDef {
                    id: None,
                    label: "Product".to_string(),
                    description: Some("A product".to_string()),
                    source_table: None,
                    properties: vec![input_property("sku"), input_property("name")],
                    constraints: vec![],
                },
            ],
            edge_types: vec![InputEdgeTypeDef {
                id: None,
                label: "PURCHASED".to_string(),
                description: None,
                source_type: "User".to_string(),
                target_type: "Product".to_string(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
            }],
            indexes: vec![InputIndexDef::Single {
                id: None,
                label: "User".to_string(),
                property: "email".to_string(),
            }],
        }
    }

    // -- normalize happy path ------------------------------------------------

    #[test]
    fn normalize_happy_path_generates_ids_and_resolves_references() {
        let input = base_input();
        let nr = normalize(input).expect("should succeed");

        // Top-level id was generated (UUID format)
        assert!(!nr.ontology.id.is_empty());
        assert!(Uuid::parse_str(&nr.ontology.id).is_ok());

        assert_eq!(nr.ontology.name, "Test Ontology");
        assert_eq!(nr.ontology.version, 1);
        assert_eq!(nr.ontology.node_types.len(), 2);
        assert_eq!(nr.ontology.edge_types.len(), 1);
        assert_eq!(nr.ontology.indexes.len(), 1);

        // Node IDs generated
        let user_node = &nr.ontology.node_types[0];
        let product_node = &nr.ontology.node_types[1];
        assert!(Uuid::parse_str(&user_node.id).is_ok());
        assert!(Uuid::parse_str(&product_node.id).is_ok());

        // Property IDs generated
        assert!(Uuid::parse_str(&user_node.properties[0].id).is_ok());
        assert!(Uuid::parse_str(&user_node.properties[1].id).is_ok());

        // Edge references resolved to node UUIDs
        assert_eq!(nr.ontology.edge_types[0].source_node_id, user_node.id);
        assert_eq!(nr.ontology.edge_types[0].target_node_id, product_node.id);
        assert!(Uuid::parse_str(&nr.ontology.edge_types[0].id).is_ok());

        // Constraint property names resolved to property IDs
        let email_prop_id = &user_node.properties[1].id; // "email" is second
        let id_prop_id = &user_node.properties[0].id; // "id" is first
        match &nr.ontology.node_types[0].constraints[0].constraint {
            NodeConstraint::Unique { property_ids } => {
                assert_eq!(property_ids, std::slice::from_ref(email_prop_id));
            }
            _ => panic!("expected Unique constraint"),
        }
        match &nr.ontology.node_types[0].constraints[1].constraint {
            NodeConstraint::Exists { property_id } => {
                assert_eq!(property_id, id_prop_id);
            }
            _ => panic!("expected Exists constraint"),
        }

        // Constraint IDs generated
        assert!(Uuid::parse_str(&nr.ontology.node_types[0].constraints[0].id).is_ok());

        // Index resolved to node/property UUIDs
        match &nr.ontology.indexes[0] {
            IndexDef::Single {
                id,
                node_id,
                property_id,
            } => {
                assert!(Uuid::parse_str(id).is_ok());
                assert_eq!(node_id, &user_node.id);
                assert_eq!(property_id, email_prop_id);
            }
            _ => panic!("expected Single index"),
        }

        // source_table extracted into mapping
        assert_eq!(
            nr.source_mapping.table_for_node(&user_node.id),
            Some("users")
        );
    }

    // -- normalize with pre-existing ids -------------------------------------

    #[test]
    fn normalize_preserves_pre_existing_ids() {
        let mut input = base_input();
        input.id = Some("my-ontology-id".to_string());
        input.node_types[0].id = Some("my-node-id".to_string());
        input.node_types[0].properties[0].id = Some("my-prop-id".to_string());
        input.node_types[0].constraints[0] = InputNodeConstraint::Unique {
            id: Some("my-constraint-id".to_string()),
            properties: vec!["email".to_string()],
        };
        input.indexes[0] = InputIndexDef::Single {
            id: Some("my-index-id".to_string()),
            label: "User".to_string(),
            property: "email".to_string(),
        };

        let nr = normalize(input).expect("should succeed");

        assert_eq!(nr.ontology.id, "my-ontology-id");
        assert_eq!(nr.ontology.node_types[0].id, "my-node-id");
        assert_eq!(nr.ontology.node_types[0].properties[0].id, "my-prop-id");
        assert_eq!(
            nr.ontology.node_types[0].constraints[0].id,
            "my-constraint-id"
        );
        match &nr.ontology.indexes[0] {
            IndexDef::Single { id, .. } => assert_eq!(id, "my-index-id"),
            _ => panic!("expected Single index"),
        }
    }

    // -- normalize error on unknown label ------------------------------------

    #[test]
    fn normalize_drops_edge_with_unknown_source() {
        let mut input = base_input();
        input.edge_types[0].source_type = "NonExistent".to_string();

        let nr = normalize(input).expect("should succeed, dropping dangling edge");
        assert!(
            nr.ontology.edge_types.is_empty(),
            "dangling edge should be dropped"
        );
    }

    #[test]
    fn normalize_drops_edge_with_unknown_target() {
        let mut input = base_input();
        input.edge_types[0].target_type = "Ghost".to_string();

        let nr = normalize(input).expect("should succeed, dropping dangling edge");
        assert!(
            nr.ontology.edge_types.is_empty(),
            "dangling edge should be dropped"
        );
    }

    #[test]
    fn normalize_auto_corrects_fuzzy_constraint_property() {
        let mut input = base_input();
        // "emal" is Levenshtein distance 1 from "email" — should auto-correct
        input.node_types[0]
            .constraints
            .push(InputNodeConstraint::Unique {
                id: None,
                properties: vec!["emal".to_string()],
            });

        let nr = normalize(input).expect("should auto-correct fuzzy match");
        let node = &nr.ontology.node_types[0];
        // All 3 constraints survive: original 2 + the auto-corrected one
        assert_eq!(node.constraints.len(), 3);
        match &node.constraints[2].constraint {
            NodeConstraint::Unique { property_ids } => {
                assert!(!property_ids.is_empty());
                // Should resolve to the "email" property ID
                assert_eq!(property_ids[0], node.properties[1].id);
            }
            _ => panic!("expected Unique constraint"),
        }
    }

    #[test]
    fn normalize_auto_corrects_fuzzy_exists_constraint() {
        let mut input = base_input();
        // "emal" is distance 1 from "email" — should auto-correct
        input.node_types[0]
            .constraints
            .push(InputNodeConstraint::Exists {
                id: None,
                property: "emal".to_string(),
            });

        let nr = normalize(input).expect("should auto-correct fuzzy match");
        let node = &nr.ontology.node_types[0];
        assert_eq!(node.constraints.len(), 3);
        match &node.constraints[2].constraint {
            NodeConstraint::Exists { property_id } => {
                assert_eq!(property_id, &node.properties[1].id);
            }
            _ => panic!("expected Exists constraint"),
        }
    }

    #[test]
    fn normalize_errors_on_completely_invalid_constraint_property() {
        let mut input = base_input();
        input.node_types[0]
            .constraints
            .push(InputNodeConstraint::Unique {
                id: None,
                properties: vec!["zzz_nonexistent_xyz".to_string()],
            });

        let nr = normalize(input).expect("should succeed, dropping invalid constraint");
        // Original unique constraint on "email" should still exist
        assert!(nr.ontology.node_types[0].has_unique_constraint());
    }

    #[test]
    fn normalize_drops_invalid_exists_constraint() {
        let mut input = base_input();
        input.node_types[0]
            .constraints
            .push(InputNodeConstraint::Exists {
                id: None,
                property: "zzz_nonexistent_xyz".to_string(),
            });

        let nr = normalize(input).expect("should succeed, dropping invalid constraint");
        // The invalid exists constraint should be dropped, original constraints remain
        assert!(nr.ontology.node_types[0].has_unique_constraint());
    }

    #[test]
    fn normalize_drops_index_with_unknown_label() {
        let mut input = base_input();
        input.indexes.push(InputIndexDef::Single {
            id: None,
            label: "Ghost".to_string(),
            property: "name".to_string(),
        });

        let nr = normalize(input).expect("should succeed, dropping invalid index");
        // base_input has 1 valid index; the Ghost index should be dropped
        assert_eq!(
            nr.ontology.indexes.len(),
            1,
            "only valid index should remain"
        );
    }

    #[test]
    fn normalize_drops_index_with_unknown_property() {
        let mut input = base_input();
        input.indexes.push(InputIndexDef::Single {
            id: None,
            label: "User".to_string(),
            property: "nonexistent".to_string(),
        });

        let nr = normalize(input).expect("should succeed, dropping invalid index");
        // base_input has 1 valid index; the nonexistent property index should be dropped
        assert_eq!(
            nr.ontology.indexes.len(),
            1,
            "only valid index should remain"
        );
    }

    // -- to_exchange_format round-trip ---------------------------------------

    #[test]
    fn round_trip_normalize_export_normalize_produces_equivalent() {
        let input = base_input();
        let nr = normalize(input).expect("first normalize");
        let exported = to_exchange_format(&nr.ontology, &nr.source_mapping);

        // Exported should have ids from canonical
        assert_eq!(exported.id, Some(nr.ontology.id.clone()));
        assert_eq!(
            exported.node_types[0].id,
            Some(nr.ontology.node_types[0].id.to_string())
        );

        // Exported edge uses labels, not UUIDs
        assert_eq!(exported.edge_types[0].source_type, "User");
        assert_eq!(exported.edge_types[0].target_type, "Product");

        // Exported constraints use property names, not IDs
        match &exported.node_types[0].constraints[0] {
            InputNodeConstraint::Unique { properties, .. } => {
                assert_eq!(properties, &["email"]);
            }
            _ => panic!("expected Unique"),
        }

        // Re-normalize exported: should produce structurally identical canonical
        let nr2 = normalize(exported).expect("second normalize");

        assert_eq!(nr.ontology.id, nr2.ontology.id);
        assert_eq!(nr.ontology.name, nr2.ontology.name);
        assert_eq!(nr.ontology.version, nr2.ontology.version);
        assert_eq!(nr.ontology.node_types.len(), nr2.ontology.node_types.len());
        assert_eq!(nr.ontology.edge_types.len(), nr2.ontology.edge_types.len());
        assert_eq!(nr.ontology.indexes.len(), nr2.ontology.indexes.len());

        // Node IDs preserved through round-trip
        for (a, b) in nr
            .ontology
            .node_types
            .iter()
            .zip(nr2.ontology.node_types.iter())
        {
            assert_eq!(a.id, b.id);
            assert_eq!(a.label, b.label);
            assert_eq!(a.description, b.description);
            assert_eq!(a.properties.len(), b.properties.len());
            assert_eq!(a.constraints.len(), b.constraints.len());
            for (pa, pb) in a.properties.iter().zip(b.properties.iter()) {
                assert_eq!(pa.id, pb.id);
                assert_eq!(pa.name, pb.name);
                assert_eq!(pa.property_type, pb.property_type);
                assert_eq!(pa.nullable, pb.nullable);
            }
        }

        // Edge IDs and resolved node references preserved
        for (a, b) in nr
            .ontology
            .edge_types
            .iter()
            .zip(nr2.ontology.edge_types.iter())
        {
            assert_eq!(a.id, b.id);
            assert_eq!(a.label, b.label);
            assert_eq!(a.source_node_id, b.source_node_id);
            assert_eq!(a.target_node_id, b.target_node_id);
            assert_eq!(a.cardinality, b.cardinality);
        }

        // Index IDs preserved
        match (&nr.ontology.indexes[0], &nr2.ontology.indexes[0]) {
            (
                IndexDef::Single {
                    id: a_id,
                    node_id: a_nid,
                    property_id: a_pid,
                },
                IndexDef::Single {
                    id: b_id,
                    node_id: b_nid,
                    property_id: b_pid,
                },
            ) => {
                assert_eq!(a_id, b_id);
                assert_eq!(a_nid, b_nid);
                assert_eq!(a_pid, b_pid);
            }
            _ => panic!("expected Single indexes"),
        }
    }

    // -- normalize catches validation errors from canonical model ------------

    #[test]
    fn normalize_catches_canonical_validation_errors() {
        let input = OntologyInputIR {
            format_version: 1,
            id: Some("  ".to_string()),
            name: "".to_string(),
            description: None,
            version: 1,
            node_types: vec![],
            edge_types: vec![],
            indexes: vec![],
        };

        let err = normalize(input).expect_err("should fail validation");
        assert!(err.iter().any(|e| e.contains("id must not be empty")));
        assert!(err.iter().any(|e| e.contains("name must not be empty")));
        assert!(err.iter().any(|e| e.contains("at least one node type")));
    }

    // -- node_key constraint resolution --------------------------------------

    #[test]
    fn normalize_resolves_node_key_constraint() {
        let input = OntologyInputIR {
            format_version: 1,
            id: None,
            name: "NodeKey Test".to_string(),
            description: None,
            version: 1,
            node_types: vec![InputNodeTypeDef {
                id: None,
                label: "Account".to_string(),
                description: None,
                source_table: None,
                properties: vec![input_property("tenant_id"), input_property("account_id")],
                constraints: vec![InputNodeConstraint::NodeKey {
                    id: None,
                    properties: vec!["tenant_id".to_string(), "account_id".to_string()],
                }],
            }],
            edge_types: vec![],
            indexes: vec![],
        };

        let nr = normalize(input).expect("should succeed");
        let node = &nr.ontology.node_types[0];
        match &node.constraints[0].constraint {
            NodeConstraint::NodeKey { property_ids } => {
                assert_eq!(property_ids.len(), 2);
                assert_eq!(property_ids[0], node.properties[0].id);
                assert_eq!(property_ids[1], node.properties[1].id);
            }
            _ => panic!("expected NodeKey"),
        }
    }
}
