use std::collections::HashMap;

use crate::ontology_ir::*;
use crate::source_mapping::SourceMapping;

use super::dtos::*;

// ---------------------------------------------------------------------------
// to_exchange_format() — Canonical model → Input DTO for export/display
// ---------------------------------------------------------------------------

/// Canonical model → Input DTO for export/display.
/// - source_node_id → label
/// - property_ids → property names
/// - constraint/index ids are preserved (Some) for round-trip
pub fn to_exchange_format(
    ontology: &OntologyIR,
    source_mapping: &SourceMapping,
) -> OntologyInputIR {
    // Build lookup maps: node_id → label, property_id → name
    let node_id_to_label: HashMap<&str, &str> = ontology
        .node_types
        .iter()
        .map(|n| (&*n.id, n.label.as_str()))
        .collect();

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

            let properties = n
                .properties
                .iter()
                .map(|p| InputPropertyDef {
                    id: Some(p.id.to_string()),
                    name: p.name.clone(),
                    property_type: p.property_type.clone(),
                    nullable: p.nullable,
                    default_value: p.default_value.clone(),
                    description: p.description.clone(),
                    source_column: source_mapping
                        .column_for_property(&n.id, &p.id)
                        .map(|s| s.to_string()),
                })
                .collect();

            InputNodeTypeDef {
                id: Some(n.id.to_string()),
                label: n.label.clone(),
                description: n.description.clone(),
                source_table: source_mapping.table_for_node(&n.id).map(|s| s.to_string()),
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
            label: e.label.clone(),
            description: e.description.clone(),
            source_type: resolve_node_label(&e.source_node_id),
            target_type: resolve_node_label(&e.target_node_id),
            properties: e
                .properties
                .iter()
                .map(|p| InputPropertyDef {
                    id: Some(p.id.to_string()),
                    name: p.name.clone(),
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
                name: name.clone(),
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

    OntologyInputIR {
        format_version: 1,
        id: Some(ontology.id.clone()),
        name: ontology.name.clone(),
        description: ontology.description.clone(),
        version: ontology.version,
        node_types,
        edge_types,
        indexes,
    }
}
