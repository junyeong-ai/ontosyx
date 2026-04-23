use std::collections::HashMap;

use crate::ir::*;
use crate::mapping::PropertyLocation;

use super::dtos::*;

// ---------------------------------------------------------------------------
// to_exchange_format() — Canonical model → Input DTO for export/display
// ---------------------------------------------------------------------------

/// Canonical model → Input DTO for export/display.
/// - source_node_id → label
/// - property_ids → property names
/// - constraint/index ids are preserved (Some) for round-trip
/// - `ObjectMappingDef` → `source_table` + per-property
///   `source_column` (column locations only — JSON-path mappings
///   cannot round-trip through the Input DTO and are dropped with
///   a structured warning by downstream exporters if present)
pub fn to_exchange_format(ontology: &OntologyIR) -> InputOntologyDef {
    // Build lookup maps: node_id → label, property_id → name
    let node_id_to_label: HashMap<&str, &str> = ontology
        .node_types
        .iter()
        .map(|n| (&*n.id, n.label.as_str()))
        .collect();

    // node_id → "source_table" (from the ObjectMappingDef pinned to
    // that node, if any). Multi-mapping nodes pick the highest-
    // precedence mapping so round-trips prefer the canonical source.
    let mut node_to_relation: HashMap<&str, &str> = HashMap::new();
    // (node_id, property_id) → column name. Keys the lookup on both
    // so a multi-mapping node's `source_column` resolves under the
    // owning mapping's relation context.
    let mut prop_to_column: HashMap<(&str, &str), &str> = HashMap::new();
    for om in ontology.object_mappings() {
        let node_id = om.node_type_id.as_ref();
        let prior_precedence = node_to_relation.get(node_id).and_then(|_| {
            ontology
                .object_mappings()
                .iter()
                .find(|x| x.node_type_id == om.node_type_id && x.relation == om.relation)
                .map(|x| x.precedence)
        });
        if prior_precedence.is_none_or(|p| om.precedence >= p) {
            node_to_relation.insert(node_id, om.relation.as_str());
        }
        for pm in &om.property_mappings {
            if let PropertyLocation::Column(col) = &pm.location {
                prop_to_column.insert(
                    (node_id, pm.property_id.as_ref()),
                    col.column.as_str(),
                );
            }
        }
    }

    // Global property_id → name map (across all nodes and edges)
    let mut prop_id_to_name: HashMap<&str, &str> = HashMap::new();
    for node in &ontology.node_types {
        for prop in &node.properties {
            prop_id_to_name.insert(&*prop.id, prop.name.as_str());
        }
    }
    for edge in &ontology.edge_types {
        for prop in &edge.properties {
            prop_id_to_name.insert(&*prop.id, prop.name.as_str());
        }
    }

    let resolve_prop_name = |pid: &str| -> String {
        prop_id_to_name
            .get(pid)
            .map(|s| s.to_string())
            .unwrap_or_else(|| pid.to_string())
    };

    let resolve_node_label = |nid: &str| -> String {
        node_id_to_label
            .get(nid)
            .map(|s| s.to_string())
            .unwrap_or_else(|| nid.to_string())
    };

    let node_types = ontology
        .node_types
        .iter()
        .map(|n| {
            let constraints = n
                .constraints
                .iter()
                .map(|cd| match &cd.constraint {
                    NodeConstraint::Unique { property_ids } => InputNodeConstraint::Unique {
                        id: Some(cd.id.to_string()),
                        properties: property_ids
                            .iter()
                            .map(|pid| resolve_prop_name(pid))
                            .collect(),
                    },
                    NodeConstraint::Exists { property_id } => InputNodeConstraint::Exists {
                        id: Some(cd.id.to_string()),
                        property: resolve_prop_name(property_id),
                    },
                    NodeConstraint::NodeKey { property_ids } => InputNodeConstraint::NodeKey {
                        id: Some(cd.id.to_string()),
                        properties: property_ids
                            .iter()
                            .map(|pid| resolve_prop_name(pid))
                            .collect(),
                    },
                })
                .collect();

            let node_id_str: &str = n.id.as_ref();
            let properties = n
                .properties
                .iter()
                .map(|p| InputPropertyDef {
                    id: Some(p.id.to_string()),
                    name: p.name.to_string(),
                    property_type: p.property_type.clone(),
                    nullable: p.nullable,
                    default_value: p.default_value.clone(),
                    description: p.description.clone(),
                    source_column: prop_to_column
                        .get(&(node_id_str, p.id.as_ref()))
                        .map(|s| s.to_string()),
                })
                .collect();

            InputNodeTypeDef {
                id: Some(n.id.to_string()),
                label: n.label.to_string(),
                description: n.description.clone(),
                source_table: node_to_relation.get(node_id_str).map(|s| s.to_string()),
                properties,
                constraints,
            }
        })
        .collect();

    let edge_types = ontology
        .edge_types
        .iter()
        .map(|e| InputEdgeTypeDef {
            id: Some(e.id.to_string()),
            label: e.label.to_string(),
            description: e.description.clone(),
            source_type: resolve_node_label(&e.source_node_id),
            target_type: resolve_node_label(&e.target_node_id),
            properties: e
                .properties
                .iter()
                .map(|p| InputPropertyDef {
                    id: Some(p.id.to_string()),
                    name: p.name.to_string(),
                    property_type: p.property_type.clone(),
                    nullable: p.nullable,
                    default_value: p.default_value.clone(),
                    description: p.description.clone(),
                    source_column: None,
                })
                .collect(),
            cardinality: e.cardinality,
        })
        .collect();

    let indexes = ontology
        .indexes
        .iter()
        .map(|idx| match idx {
            IndexDef::Single {
                id,
                node_id,
                property_id,
            } => InputIndexDef::Single {
                id: Some(id.clone()),
                label: resolve_node_label(node_id),
                property: resolve_prop_name(property_id),
            },
            IndexDef::Composite {
                id,
                node_id,
                property_ids,
            } => InputIndexDef::Composite {
                id: Some(id.clone()),
                label: resolve_node_label(node_id),
                properties: property_ids
                    .iter()
                    .map(|pid| resolve_prop_name(pid))
                    .collect(),
            },
            IndexDef::FullText {
                id,
                name,
                node_id,
                property_ids,
            } => InputIndexDef::FullText {
                id: Some(id.clone()),
                name: name.to_string(),
                label: resolve_node_label(node_id),
                properties: property_ids
                    .iter()
                    .map(|pid| resolve_prop_name(pid))
                    .collect(),
            },
            IndexDef::Vector {
                id,
                node_id,
                property_id,
                dimensions,
                similarity,
            } => InputIndexDef::Vector {
                id: Some(id.clone()),
                label: resolve_node_label(node_id),
                property: resolve_prop_name(property_id),
                dimensions: *dimensions,
                similarity: *similarity,
            },
        })
        .collect();

    InputOntologyDef {
        format_version: 1,
        id: Some(ontology.id.clone()),
        name: ontology.name.clone(),
        description: ontology.description.clone(),
        version: ontology.version.number,
        node_types,
        edge_types,
        indexes,
    }
}
