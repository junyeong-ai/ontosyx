use ox_core::ontology_ir::{IndexDef, NodeConstraint, NodeTypeDef, OntologyIR, PropertyDef};
use ox_core::types::PropertyType;

use super::params::escape_identifier;

/// Default maximum number of auto-generated range indices.
/// Override via SystemConfig `cypher.max_auto_indices`.
pub const DEFAULT_MAX_AUTO_INDICES: usize = 20;

/// Default high-priority property names for indexing.
/// Override via SystemConfig `cypher.high_priority_names`.
pub const DEFAULT_HIGH_PRIORITY_NAMES: &[&str] = &["id", "code", "name", "email"];

// ---------------------------------------------------------------------------
// IndexStats — compilation statistics for auto-index generation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStats {
    pub total: usize,
    pub explicit: usize,
    pub auto_generated: usize,
    /// How many auto-index candidates were dropped due to the cap.
    pub truncated: usize,
}

pub(crate) fn compile_node_constraints(node: &NodeTypeDef) -> Vec<String> {
    let mut stmts = Vec::new();
    let label = &node.label;

    for constraint_def in &node.constraints {
        match &constraint_def.constraint {
            NodeConstraint::Unique { property_ids } => {
                let props = property_ids
                    .iter()
                    .filter_map(|pid| node.properties.iter().find(|p| p.id == *pid))
                    .map(|p| format!("n.{}", escape_identifier(&p.name)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let escaped_label = escape_identifier(label);
                stmts.push(format!(
                    "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE ({props}) IS UNIQUE"
                ));
            }
            NodeConstraint::Exists { property_id } => {
                if let Some(prop) = node.properties.iter().find(|p| p.id == *property_id) {
                    let escaped_label = escape_identifier(label);
                    stmts.push(format!(
                        "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE n.{} IS NOT NULL",
                        escape_identifier(&prop.name)
                    ));
                }
            }
            NodeConstraint::NodeKey { property_ids } => {
                let props = property_ids
                    .iter()
                    .filter_map(|pid| node.properties.iter().find(|p| p.id == *pid))
                    .map(|p| format!("n.{}", escape_identifier(&p.name)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let escaped_label = escape_identifier(label);
                stmts.push(format!(
                    "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE ({props}) IS NODE KEY"
                ));
            }
        }
    }

    stmts
}

pub(super) fn compile_index(ontology: &OntologyIR, index: &IndexDef) -> String {
    match index {
        IndexDef::Single {
            id: _,
            node_id,
            property_id,
        } => {
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let prop_name = escape_identifier(
                ontology
                    .node_by_id(node_id)
                    .and_then(|n| n.properties.iter().find(|p| p.id == *property_id))
                    .map(|p| p.name.as_str())
                    .unwrap_or("UNKNOWN"),
            );
            format!("CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name})")
        }
        IndexDef::Composite {
            id: _,
            node_id,
            property_ids,
        } => {
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let node = ontology.node_by_id(node_id);
            let props = property_ids
                .iter()
                .map(|pid| {
                    node.and_then(|n| n.properties.iter().find(|p| p.id == *pid))
                        .map(|p| format!("n.{}", escape_identifier(&p.name)))
                        .unwrap_or_else(|| format!("n.{}", escape_identifier("UNKNOWN")))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON ({props})")
        }
        IndexDef::FullText {
            id: _,
            name,
            node_id,
            property_ids,
        } => {
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let escaped_name = escape_identifier(name);
            let node = ontology.node_by_id(node_id);
            let props = property_ids
                .iter()
                .map(|pid| {
                    node.and_then(|n| n.properties.iter().find(|p| p.id == *pid))
                        .map(|p| format!("n.{}", escape_identifier(&p.name)))
                        .unwrap_or_else(|| format!("n.{}", escape_identifier("UNKNOWN")))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "CREATE FULLTEXT INDEX {escaped_name} IF NOT EXISTS FOR (n:{label}) ON EACH [{props}]"
            )
        }
        IndexDef::Vector {
            id: _,
            node_id,
            property_id,
            dimensions,
            similarity,
        } => {
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let prop_name = escape_identifier(
                ontology
                    .node_by_id(node_id)
                    .and_then(|n| n.properties.iter().find(|p| p.id == *property_id))
                    .map(|p| p.name.as_str())
                    .unwrap_or("UNKNOWN"),
            );
            let sim = match similarity {
                ox_core::ontology_ir::VectorSimilarity::Cosine => "cosine",
                ox_core::ontology_ir::VectorSimilarity::Euclidean => "euclidean",
            };
            format!(
                "CREATE VECTOR INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name}) \
                 OPTIONS {{indexConfig: {{`vector.dimensions`: {dimensions}, `vector.similarity_function`: '{sim}'}}}}"
            )
        }
    }
}

pub(super) fn constraint_covers_prop(
    constraint: &NodeConstraint,
    properties: &[PropertyDef],
    prop_name: &str,
) -> bool {
    let prop_id_matches = |pid: &str| -> bool {
        properties
            .iter()
            .any(|p| p.id == pid && p.name == prop_name)
    };
    match constraint {
        NodeConstraint::Unique { property_ids } | NodeConstraint::NodeKey { property_ids } => {
            property_ids.iter().any(|pid| prop_id_matches(pid))
        }
        NodeConstraint::Exists { property_id } => prop_id_matches(property_id),
    }
}

// ---------------------------------------------------------------------------
// Auto-index generation with priority sorting and cap
// ---------------------------------------------------------------------------

/// Priority score for an auto-index candidate (lower = higher priority).
fn auto_index_priority(prop: &PropertyDef, high_priority_names: &[&str]) -> u8 {
    let name_lower = prop.name.to_lowercase();
    if high_priority_names.iter().any(|n| *n == name_lower) {
        return 0; // common query targets
    }
    match prop.property_type {
        PropertyType::String | PropertyType::Int => 1, // likely filtered on
        _ => 2,
    }
}

/// An auto-index candidate before truncation.
struct AutoIndexCandidate {
    statement: String,
    priority: u8,
}

/// Collect, prioritize, and cap auto-generated range indices for non-nullable
/// properties not already covered by a constraint.
///
/// Returns the index statements (truncated to `max_auto_indices`) and stats.
pub(super) fn compile_auto_indices(ontology: &OntologyIR) -> (Vec<String>, IndexStats) {
    compile_auto_indices_with(
        ontology,
        DEFAULT_MAX_AUTO_INDICES,
        DEFAULT_HIGH_PRIORITY_NAMES,
    )
}

/// Configurable version: allows runtime override of max indices and priority names.
pub(super) fn compile_auto_indices_with(
    ontology: &OntologyIR,
    max_auto_indices: usize,
    high_priority_names: &[&str],
) -> (Vec<String>, IndexStats) {
    let mut candidates: Vec<AutoIndexCandidate> = Vec::new();

    for node in &ontology.node_types {
        for prop in &node.properties {
            if prop.nullable {
                continue;
            }
            let covered = node
                .constraints
                .iter()
                .any(|c| constraint_covers_prop(&c.constraint, &node.properties, &prop.name));
            if covered {
                continue;
            }
            candidates.push(AutoIndexCandidate {
                statement: format!(
                    "CREATE INDEX IF NOT EXISTS FOR (n:{}) ON (n.{})",
                    escape_identifier(&node.label),
                    escape_identifier(&prop.name),
                ),
                priority: auto_index_priority(prop, high_priority_names),
            });
        }
    }

    // Stable sort by priority so that within the same priority the original
    // (ontology-definition) order is preserved.
    candidates.sort_by_key(|c| c.priority);

    let total_candidates = candidates.len();
    let truncated = total_candidates.saturating_sub(max_auto_indices);

    if truncated > 0 {
        tracing::warn!(
            total_candidates,
            max = max_auto_indices,
            truncated,
            "Auto-index cap reached; some non-nullable properties will not have range indices"
        );
    }

    candidates.truncate(max_auto_indices);

    let explicit = ontology.indexes.len();
    let auto_generated = candidates.len();

    let stats = IndexStats {
        total: explicit + auto_generated,
        explicit,
        auto_generated,
        truncated,
    };

    let statements = candidates.into_iter().map(|c| c.statement).collect();
    (statements, stats)
}
