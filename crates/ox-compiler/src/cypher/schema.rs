use std::collections::HashMap;
use std::sync::OnceLock;

use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;
use ox_ontology::ir::{IndexDef, NodeConstraint, NodeTypeDef, OntologyIR, PropertyDef, PropertyId};

use super::CypherDialect;
use super::params::escape_identifier;

/// O(1) `PropertyId → PropertyKey` lookup over a node's properties.
/// Built once per call site that needs to resolve multiple ids against
/// the same node, replacing the linear `properties.iter().find()` walk.
fn property_name_index(node: &NodeTypeDef) -> HashMap<&PropertyId, &PropertyKey> {
    node.properties.iter().map(|p| (&p.id, &p.name)).collect()
}

/// Default maximum number of auto-generated range indices when
/// [`init_auto_index_config`] has not been called.
pub const DEFAULT_MAX_AUTO_INDICES: usize = 20;

/// Default high-priority property names for auto-index generation.
///
/// Covers both English and Korean conventions so that domain-agnostic
/// auto-indexing works on Korean-first ontologies (e.g., `고객번호`, `이름`,
/// `이메일`) without per-workspace configuration. Match is case-insensitive
/// and exact — property names are normalized to lowercase before comparison.
pub const DEFAULT_HIGH_PRIORITY_NAMES: &[&str] = &[
    // English
    "id",
    "code",
    "name",
    "email",
    // Korean
    "번호",
    "이름",
    "이메일",
    "코드",
];

/// Runtime-configurable auto-index policy.
#[derive(Debug, Clone)]
pub struct AutoIndexConfig {
    /// Hard cap on the number of auto-generated range indices per
    /// `compile_schema` call. Defaults to [`DEFAULT_MAX_AUTO_INDICES`].
    pub max_indices: usize,
    /// Property names that get the highest auto-index priority. Match is
    /// case-insensitive and exact. Defaults to [`DEFAULT_HIGH_PRIORITY_NAMES`].
    pub high_priority_names: Vec<String>,
}

impl Default for AutoIndexConfig {
    fn default() -> Self {
        Self {
            max_indices: DEFAULT_MAX_AUTO_INDICES,
            high_priority_names: DEFAULT_HIGH_PRIORITY_NAMES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }
}

static AUTO_INDEX_CONFIG: OnceLock<AutoIndexConfig> = OnceLock::new();

/// Set the auto-index policy used by every `compile_schema` call.
///
/// First-write-wins: subsequent calls are silently ignored, mirroring
/// the `ox-memory` initialization pattern. Call this once at startup
/// from `ox-api::main` before the first ontology is compiled.
pub fn init_auto_index_config(config: AutoIndexConfig) {
    // `OnceLock::set` returns `Err(value)` after the first successful
    // set — the first-write-wins semantic this function documents.
    // Silent skip is intentional; subsequent boots in long-running
    // tests don't need to know they were pre-empted.
    #[allow(clippy::let_underscore_must_use)]
    let _ = AUTO_INDEX_CONFIG.set(config);
}

/// Read the active auto-index policy, falling back to defaults.
fn auto_index_config() -> &'static AutoIndexConfig {
    AUTO_INDEX_CONFIG.get_or_init(AutoIndexConfig::default)
}

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

pub(crate) fn compile_node_constraints(node: &NodeTypeDef, dialect: CypherDialect) -> Vec<String> {
    let mut stmts = Vec::new();
    let label = &node.label;
    let escaped_label = escape_identifier(label);
    let prop_names = property_name_index(node);

    for constraint_def in &node.constraints {
        match &constraint_def.constraint {
            NodeConstraint::Unique { property_ids } => {
                let prop_paths: Vec<String> = property_ids
                    .iter()
                    .filter_map(|pid| prop_names.get(pid).copied())
                    .map(|name| format!("n.{}", escape_identifier(name)))
                    .collect();
                if prop_paths.is_empty() {
                    continue;
                }
                match dialect {
                    CypherDialect::Neo4j => {
                        let props = prop_paths.join(", ");
                        stmts.push(format!(
                            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE ({props}) IS UNIQUE"
                        ));
                    }
                    CypherDialect::Memgraph => {
                        // Memgraph 4.x syntax: one ASSERT per statement.
                        // The dialect has no composite-unique; emit one
                        // UNIQUE constraint per property. Same total guarantee
                        // as Neo4j's composite form when the caller intent is
                        // "each value in isolation is unique".
                        for path in &prop_paths {
                            stmts.push(format!(
                                "CREATE CONSTRAINT ON (n:{escaped_label}) ASSERT {path} IS UNIQUE"
                            ));
                        }
                    }
                }
            }
            NodeConstraint::Exists { property_id } => {
                let Some(prop) = node.properties.iter().find(|p| p.id == *property_id) else {
                    continue;
                };
                let prop_path = format!("n.{}", escape_identifier(&prop.name));
                match dialect {
                    CypherDialect::Neo4j => {
                        stmts.push(format!(
                            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE {prop_path} IS NOT NULL"
                        ));
                    }
                    CypherDialect::Memgraph => {
                        stmts.push(format!(
                            "CREATE CONSTRAINT ON (n:{escaped_label}) ASSERT EXISTS ({prop_path})"
                        ));
                    }
                }
            }
            NodeConstraint::NodeKey { property_ids } => match dialect {
                CypherDialect::Neo4j => {
                    let props = property_ids
                        .iter()
                        .filter_map(|pid| prop_names.get(pid).copied())
                        .map(|name| format!("n.{}", escape_identifier(name)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    stmts.push(format!(
                        "CREATE CONSTRAINT IF NOT EXISTS FOR (n:{escaped_label}) REQUIRE ({props}) IS NODE KEY"
                    ));
                }
                CypherDialect::Memgraph => {
                    // Memgraph has no equivalent to Neo4j's NODE KEY
                    // (composite uniqueness + non-null). Skipping is the
                    // safe no-op; logging surfaces the loss of guarantee
                    // for operators to decide whether to back-fill the
                    // missing invariant with explicit UNIQUE + EXISTS
                    // constraints on the same columns.
                    tracing::info!(
                        label = %label,
                        property_count = property_ids.len(),
                        "Memgraph dialect: NODE KEY constraint skipped (unsupported)",
                    );
                }
            },
        }
    }

    stmts
}

pub(super) fn compile_index(
    ontology: &OntologyIR,
    index: &IndexDef,
    dialect: CypherDialect,
) -> Option<String> {
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
            Some(match dialect {
                CypherDialect::Neo4j => {
                    format!("CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name})")
                }
                CypherDialect::Memgraph => {
                    format!("CREATE INDEX ON :{label}({prop_name})")
                }
            })
        }
        IndexDef::Composite {
            id: _,
            node_id,
            property_ids,
        } => {
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let lookup = ontology.node_by_id(node_id).map(property_name_index);
            let prop_names: Vec<String> = property_ids
                .iter()
                .map(|pid| {
                    lookup
                        .as_ref()
                        .and_then(|m| m.get(pid).copied())
                        .map(|name| escape_identifier(name))
                        .unwrap_or_else(|| escape_identifier("UNKNOWN"))
                })
                .collect();
            if prop_names.is_empty() {
                return None;
            }
            Some(match dialect {
                CypherDialect::Neo4j => {
                    let props = prop_names
                        .iter()
                        .map(|n| format!("n.{n}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("CREATE INDEX IF NOT EXISTS FOR (n:{label}) ON ({props})")
                }
                CypherDialect::Memgraph => {
                    // Memgraph does not support composite label-property
                    // indexes natively; only the first property is indexed.
                    // Log so operators can split into single-prop indexes
                    // explicitly if the remaining columns are load-bearing
                    // for their query pattern.
                    tracing::warn!(
                        label = %label,
                        prop_count = prop_names.len(),
                        first_prop = %prop_names[0],
                        "Memgraph dialect: composite index compiled as single-prop \
                         (first property only); remaining properties will not be indexed"
                    );
                    format!("CREATE INDEX ON :{label}({})", prop_names[0])
                }
            })
        }
        IndexDef::FullText {
            id: _,
            name,
            node_id,
            property_ids,
        } => {
            if dialect == CypherDialect::Memgraph {
                tracing::info!(
                    name = %name,
                    "Memgraph dialect: FULLTEXT index skipped (unsupported)",
                );
                return None;
            }
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let escaped_name = escape_identifier(name);
            let lookup = ontology.node_by_id(node_id).map(property_name_index);
            let props = property_ids
                .iter()
                .map(|pid| {
                    lookup
                        .as_ref()
                        .and_then(|m| m.get(pid).copied())
                        .map(|name| format!("n.{}", escape_identifier(name)))
                        .unwrap_or_else(|| format!("n.{}", escape_identifier("UNKNOWN")))
                })
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!(
                "CREATE FULLTEXT INDEX {escaped_name} IF NOT EXISTS FOR (n:{label}) ON EACH [{props}]"
            ))
        }
        IndexDef::Vector {
            id: _,
            node_id,
            property_id,
            dimensions,
            similarity,
        } => {
            if dialect == CypherDialect::Memgraph {
                tracing::info!("Memgraph dialect: VECTOR index skipped (unsupported)",);
                return None;
            }
            let label = escape_identifier(ontology.node_label(node_id).unwrap_or("UNKNOWN"));
            let prop_name = escape_identifier(
                ontology
                    .node_by_id(node_id)
                    .and_then(|n| n.properties.iter().find(|p| p.id == *property_id))
                    .map(|p| p.name.as_str())
                    .unwrap_or("UNKNOWN"),
            );
            let sim = match similarity {
                ox_ontology::ir::VectorSimilarity::Cosine => "cosine",
                ox_ontology::ir::VectorSimilarity::Euclidean => "euclidean",
            };
            Some(format!(
                "CREATE VECTOR INDEX IF NOT EXISTS FOR (n:{label}) ON (n.{prop_name}) \
                 OPTIONS {{indexConfig: {{`vector.dimensions`: {dimensions}, `vector.similarity_function`: '{sim}'}}}}"
            ))
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
/// Reads the active [`AutoIndexConfig`] (set via [`init_auto_index_config`])
/// or falls back to defaults.
pub(super) fn compile_auto_indices(
    ontology: &OntologyIR,
    dialect: CypherDialect,
) -> (Vec<String>, IndexStats) {
    let config = auto_index_config();
    let names: Vec<&str> = config
        .high_priority_names
        .iter()
        .map(String::as_str)
        .collect();
    compile_auto_indices_with(ontology, config.max_indices, &names, dialect)
}

/// Configurable version: allows runtime override of max indices and priority names.
pub(super) fn compile_auto_indices_with(
    ontology: &OntologyIR,
    max_auto_indices: usize,
    high_priority_names: &[&str],
    dialect: CypherDialect,
) -> (Vec<String>, IndexStats) {
    let mut candidates: Vec<AutoIndexCandidate> = Vec::new();

    for node in ontology.node_types() {
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
            let escaped_label = escape_identifier(&node.label);
            let escaped_prop = escape_identifier(&prop.name);
            let statement = match dialect {
                CypherDialect::Neo4j => format!(
                    "CREATE INDEX IF NOT EXISTS FOR (n:{escaped_label}) ON (n.{escaped_prop})"
                ),
                CypherDialect::Memgraph => {
                    format!("CREATE INDEX ON :{escaped_label}({escaped_prop})")
                }
            };
            candidates.push(AutoIndexCandidate {
                statement,
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

    let explicit = ontology.indexes().len();
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
