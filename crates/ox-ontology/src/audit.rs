//! Graph Audit — detect mismatches between designed ontology and actual Neo4j data.
//!
//! Compares `OntologyIR` (what was designed) against `GraphSchemaOverview`
//! (what actually exists in the graph database) to identify:
//! - Orphan graph labels (exist in graph but not in ontology)
//! - Missing graph labels (exist in ontology but not in graph)
//! - Matched labels (exist in both)

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Serialize;

use crate::graph_exploration::GraphSchemaOverview;
use crate::ir::OntologyIR;
use ox_core::error::OxResult;
use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;

/// Sync status between ontology and graph data.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    /// All ontology labels exist in graph and vice versa.
    Synced,
    /// Some labels match, some don't.
    Partial,
    /// No overlap between ontology and graph labels.
    Unsynced,
}

/// Result of comparing ontology against live graph data.
#[derive(Debug, Clone, Serialize, JsonSchema, utoipa::ToSchema)]
pub struct GraphAuditReport {
    /// Node labels that exist in both ontology and graph.
    pub matched_nodes: Vec<String>,
    /// Node labels in graph but not in ontology.
    pub orphan_graph_nodes: Vec<String>,
    /// Node labels in ontology but not in graph.
    pub missing_graph_nodes: Vec<String>,
    /// Edge labels that exist in both ontology and graph.
    pub matched_edges: Vec<String>,
    /// Edge labels in graph but not in ontology.
    pub orphan_graph_edges: Vec<String>,
    /// Edge labels in ontology but not in graph.
    pub missing_graph_edges: Vec<String>,
    /// Overall sync status.
    pub sync_status: SyncStatus,
    /// Percentage of ontology labels found in graph (0-100).
    pub sync_percentage: u8,
}

/// Compare an ontology against actual graph data to detect label mismatches.
pub fn audit_graph(ontology: &OntologyIR, overview: &GraphSchemaOverview) -> GraphAuditReport {
    // Collect ontology labels
    let ont_nodes: BTreeSet<&str> = ontology
        .node_types
        .iter()
        .map(|n| n.label.as_str())
        .collect();
    let ont_edges: BTreeSet<&str> = ontology
        .edge_types
        .iter()
        .map(|e| e.label.as_str())
        .collect();

    // Collect graph labels
    let graph_nodes: BTreeSet<&str> = overview.labels.iter().map(|l| l.label.as_str()).collect();
    let graph_edges: BTreeSet<&str> = overview
        .relationships
        .iter()
        .map(|r| r.rel_type.as_str())
        .collect();

    // Set operations
    let matched_nodes: Vec<String> = ont_nodes
        .intersection(&graph_nodes)
        .map(|s| s.to_string())
        .collect();
    let orphan_graph_nodes: Vec<String> = graph_nodes
        .difference(&ont_nodes)
        .map(|s| s.to_string())
        .collect();
    let missing_graph_nodes: Vec<String> = ont_nodes
        .difference(&graph_nodes)
        .map(|s| s.to_string())
        .collect();

    let matched_edges: Vec<String> = ont_edges
        .intersection(&graph_edges)
        .map(|s| s.to_string())
        .collect();
    let orphan_graph_edges: Vec<String> = graph_edges
        .difference(&ont_edges)
        .map(|s| s.to_string())
        .collect();
    let missing_graph_edges: Vec<String> = ont_edges
        .difference(&graph_edges)
        .map(|s| s.to_string())
        .collect();

    // Calculate sync percentage
    let total_ontology = ont_nodes.len() + ont_edges.len();
    let total_matched = matched_nodes.len() + matched_edges.len();
    let sync_percentage = if total_ontology > 0 {
        ((total_matched as f64 / total_ontology as f64) * 100.0) as u8
    } else {
        100
    };

    let sync_status = if orphan_graph_nodes.is_empty()
        && orphan_graph_edges.is_empty()
        && missing_graph_nodes.is_empty()
        && missing_graph_edges.is_empty()
    {
        SyncStatus::Synced
    } else if matched_nodes.is_empty() && matched_edges.is_empty() {
        SyncStatus::Unsynced
    } else {
        SyncStatus::Partial
    };

    GraphAuditReport {
        matched_nodes,
        orphan_graph_nodes,
        missing_graph_nodes,
        matched_edges,
        orphan_graph_edges,
        missing_graph_edges,
        sync_status,
        sync_percentage,
    }
}

/// Construct an OntologyIR from live graph schema (Graph Import / Adopt).
///
/// Map Neo4j property type strings to OntologyIR PropertyType.
fn map_neo4j_type(neo4j_type: &str) -> ox_core::types::PropertyType {
    use ox_core::types::PropertyType;
    // Neo4j returns types like "String", "Long", "Double" (mixed case)
    match neo4j_type.to_uppercase().as_str() {
        "STRING" => PropertyType::String,
        "INTEGER" | "LONG" => PropertyType::Int,
        "FLOAT" | "DOUBLE" => PropertyType::Float,
        "BOOLEAN" => PropertyType::Bool,
        "DATE" => PropertyType::Date,
        "LOCAL_DATE_TIME" | "ZONED_DATE_TIME" | "DATE_TIME" | "LOCALDATETIME" | "ZONEDDATETIME" => {
            PropertyType::DateTime
        }
        "DURATION" => PropertyType::Duration,
        "BYTE_ARRAY" => PropertyType::Bytes,
        _ => PropertyType::String,
    }
}

/// Creates node types from graph labels and edge types from relationship patterns.
/// Includes property information from graph schema introspection.
/// The resulting ontology matches the actual graph labels, enabling correct AI queries.
///
/// Returns [`OxError::Validation`] if any graph label or relationship type
/// violates [`GraphLabel`]'s invariants. In practice Neo4j enforces a
/// compatible rule, so the error path exists to make an otherwise-silent
/// type coercion observable.
pub fn ontology_from_graph(overview: &GraphSchemaOverview, name: &str) -> OxResult<OntologyIR> {
    use crate::ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};

    // Build property lookup: entity_type → Vec<PropertyDef>
    let build_props =
        |schemas: &[crate::graph_exploration::PropertySchema], label: &str| -> Vec<PropertyDef> {
            schemas
                .iter()
                .filter(|p| p.entity_type == label)
                .filter_map(|p| {
                    // Skip properties whose introspected name fails
                    // `PropertyKey` validation — Neo4j in principle
                    // accepts a wider set than the Cypher identifier
                    // rule, so the minority case where a key can't be
                    // round-tripped becomes an orphaned property in the
                    // adopted ontology rather than a hard failure.
                    let name = PropertyKey::new(p.property_name.clone()).ok()?;
                    Some(PropertyDef {
                        id: format!("p_{label}_{}", p.property_name).into(),
                        name,
                        property_type: map_neo4j_type(
                            p.property_types
                                .first()
                                .map(|s| s.as_str())
                                .unwrap_or("STRING"),
                        ),
                        nullable: !p.mandatory,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    })
                })
                .collect()
        };

    // Create node types from labels with properties
    let node_types: Vec<NodeTypeDef> = overview
        .labels
        .iter()
        .enumerate()
        .map(|(i, label_stat)| {
            Ok::<_, ox_core::error::OxError>(NodeTypeDef {
                id: format!("n{i}").into(),
                label: GraphLabel::new(label_stat.label.clone())?,
                description: LocalizedText::new(format!(
                    "{} ({} nodes)",
                    label_stat.label, label_stat.count
                )),
                properties: build_props(&overview.node_properties, &label_stat.label),
                constraints: vec![],
                ..Default::default()
            })
        })
        .collect::<OxResult<_>>()?;

    // Build node label→id map
    let label_to_id: std::collections::HashMap<&str, &str> = node_types
        .iter()
        .map(|n| (n.label.as_str(), n.id.as_ref()))
        .collect();

    // Create edge types from relationship patterns (deduplicated by rel_type)
    let mut seen_rels = std::collections::HashSet::new();
    let edge_types: Vec<EdgeTypeDef> = overview
        .relationships
        .iter()
        .enumerate()
        .filter(|(_, rp)| seen_rels.insert(rp.rel_type.clone()))
        .map(|(i, rp)| {
            // Fail the whole conversion on an invalid relationship
            // label for the same reason as node labels above —
            // better an observable error than a silent coercion that
            // produces an ontology the caller can't actually query.
            let label = GraphLabel::new(rp.rel_type.clone())?;
            // Skip edges with unknown source/target labels (orphaned relationships)
            let Some(source_id) = label_to_id.get(rp.from_label.as_str()).copied() else {
                return Ok(None);
            };
            let Some(target_id) = label_to_id.get(rp.to_label.as_str()).copied() else {
                return Ok(None);
            };
            Ok(Some(EdgeTypeDef {
                id: format!("e{i}").into(),
                label,
                description: LocalizedText::new(format!(
                    "{} -[:{}]-> {} ({} relationships)",
                    rp.from_label, rp.rel_type, rp.to_label, rp.count,
                )),
                source_node_id: source_id.into(),
                target_node_id: target_id.into(),
                properties: build_props(&overview.rel_properties, &rp.rel_type),
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            }))
        })
        .collect::<OxResult<Vec<Option<EdgeTypeDef>>>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(OntologyIR::new(
        uuid::Uuid::new_v4().to_string(),
        name.to_string(),
        LocalizedText::new(format!(
            "Auto-generated from graph with {} nodes and {} relationships",
            overview.total_nodes, overview.total_relationships
        )),
        1,
        node_types,
        edge_types,
        vec![],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_exploration::{GraphSchemaOverview, LabelStat, RelationshipPattern};
    use crate::ir::OntologyIR;
    use crate::ir::{Cardinality, EdgeTypeDef, NodeTypeDef};
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;

    fn make_ontology(nodes: &[&str], edges: &[(&str, &str, &str)]) -> OntologyIR {
        let node_types: Vec<NodeTypeDef> = nodes
            .iter()
            .enumerate()
            .map(|(i, label)| NodeTypeDef {
                id: format!("n{i}").into(),
                label: GraphLabel::new(*label).expect("test label must be valid"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            })
            .collect();
        let edge_types: Vec<EdgeTypeDef> = edges
            .iter()
            .enumerate()
            .map(|(i, (src, label, tgt))| EdgeTypeDef {
                id: format!("e{i}").into(),
                label: GraphLabel::new(*label).expect("test edge label must be valid"),
                description: LocalizedText::default(),
                source_node_id: format!("n{}", nodes.iter().position(|n| n == src).unwrap_or(0))
                    .into(),
                target_node_id: format!("n{}", nodes.iter().position(|n| n == tgt).unwrap_or(0))
                    .into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            })
            .collect();
        OntologyIR::new(
            "test".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            node_types,
            edge_types,
            vec![],
        )
    }

    fn make_overview(labels: &[&str], rels: &[&str]) -> GraphSchemaOverview {
        GraphSchemaOverview {
            labels: labels
                .iter()
                .map(|l| LabelStat {
                    label: l.to_string(),
                    count: 10,
                })
                .collect(),
            relationships: rels
                .iter()
                .map(|r| RelationshipPattern {
                    from_label: "A".into(),
                    rel_type: r.to_string(),
                    to_label: "B".into(),
                    count: 5,
                })
                .collect(),
            total_nodes: labels.len() as i64 * 10,
            total_relationships: rels.len() as i64 * 5,
            node_properties: vec![],
            rel_properties: vec![],
        }
    }

    #[test]
    fn test_fully_synced() {
        let ont = make_ontology(&["Product", "Brand"], &[("Product", "MADE_BY", "Brand")]);
        let overview = make_overview(&["Product", "Brand"], &["MADE_BY"]);
        let report = audit_graph(&ont, &overview);
        assert_eq!(report.sync_status, SyncStatus::Synced);
        assert_eq!(report.sync_percentage, 100);
    }

    #[test]
    fn test_partial_sync() {
        let ont = make_ontology(
            &["Product", "Brand"],
            &[
                ("Product", "MADE_BY", "Brand"),
                ("Product", "TREATS_CONCERN", "Brand"),
            ],
        );
        let overview = make_overview(&["Product", "Brand"], &["MADE_BY", "TREATS"]);
        let report = audit_graph(&ont, &overview);
        assert_eq!(report.sync_status, SyncStatus::Partial);
        assert_eq!(report.orphan_graph_edges, vec!["TREATS"]);
        assert_eq!(report.missing_graph_edges, vec!["TREATS_CONCERN"]);
    }
}
