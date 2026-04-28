use std::collections::{BTreeMap, HashMap};

use crate::ir::OntologyIR;
use crate::mapping::{ObjectMappingDef, PropertyLocation};
use crate::source_analysis::ColumnClarification;
use ox_core::source_schema::{SourceProfile, SourceSchema};

use super::types::{
    OntologyQualityReport, QualityConfidence, QualityConfig, QualityGap, QualityGapCategory,
    QualityGapRef, QualityGapSeverity, is_cryptic_short, is_excluded,
};

/// Builder for the i18n params bag so emit sites stay tight.
fn params<I, K, V>(entries: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    entries
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
}

// ---------------------------------------------------------------------------
// Quality assessment (pure function — no LLM, no I/O)
// ---------------------------------------------------------------------------

/// Assess quality gaps in a generated ontology against the source data profile.
/// Returns a report with gaps ordered by severity (high first).
///
/// When `profile` is None (e.g., text sources), source-level checks are skipped
/// but ontology-level checks (missing descriptions, etc.) still run.
///
/// Columns that have user-provided `column_clarifications` are excluded from
/// data-observation checks (opaque enum, numeric enum, single value bias, sparse property)
/// since the user has already provided domain context for those columns.
///
/// Accepts the canonical `&[ObjectMappingDef]` slice — the single
/// source of truth for which source relation a node type binds to.
pub fn assess_quality(
    ontology: &OntologyIR,
    schema: Option<&SourceSchema>,
    profile: Option<&SourceProfile>,
    object_mappings: &[ObjectMappingDef],
    excluded_tables: &[String],
    column_clarifications: &[ColumnClarification],
    config: &QualityConfig,
) -> OntologyQualityReport {
    let mut gaps = Vec::new();
    let excluded_tables = excluded_tables
        .iter()
        .map(|table| table.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let has_clarification = |table: &str, column: &str| -> bool {
        column_clarifications
            .iter()
            .any(|c| c.table.eq_ignore_ascii_case(table) && c.column.eq_ignore_ascii_case(column))
    };

    let empty_profiles = Vec::new();
    let table_profiles = profile
        .map(|p| &p.table_profiles)
        .unwrap_or(&empty_profiles);

    for tp in table_profiles {
        if is_excluded(&tp.table_name, &excluded_tables) {
            continue;
        }

        // Table-level check: too few rows for reliable statistics
        if tp.row_count > 0 && tp.row_count < config.small_sample_threshold {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::SmallSample,
                location: QualityGapRef::SourceTable { table: tp.table_name.clone() },
                params: params([
                    ("table", tp.table_name.clone()),
                    ("row_count", tp.row_count.to_string()),
                ]),
            });
        }

        for cs in &tp.column_stats {
            // Skip data-observation checks for columns the user has already clarified.
            if has_clarification(&tp.table_name, &cs.column_name) {
                continue;
            }

            // Column-level check 1: opaque short enum value coexists with longer meaningful words.
            if !cs.sample_values.is_empty() {
                let short_cryptic: Vec<&str> = cs
                    .sample_values
                    .iter()
                    .filter(|v| is_cryptic_short(v))
                    .map(String::as_str)
                    .collect();
                let has_longer = cs.sample_values.iter().any(|v| v.len() > 2);

                if !short_cryptic.is_empty() && has_longer {
                    gaps.push(QualityGap {
                        severity: QualityGapSeverity::High,
                        category: QualityGapCategory::OpaqueEnumValue,
                        location: QualityGapRef::SourceColumn { table: tp.table_name.clone(), column: cs.column_name.clone() },
                        params: params([
                            ("sample_values", cs.sample_values.join(", ")),
                            ("short_cryptic", short_cryptic.join(", ")),
                        ]),
                    });
                }
            }

            // Column-level check 2: all sample values are integers with low cardinality
            let is_id_column = cs.column_name == "id" || cs.column_name.ends_with("_id");
            if !is_id_column
                && !cs.sample_values.is_empty()
                && cs.distinct_count >= 2
                && cs.distinct_count <= 20
                && cs.sample_values.iter().all(|v| v.parse::<i64>().is_ok())
            {
                gaps.push(QualityGap {
                    severity: QualityGapSeverity::High,
                    category: QualityGapCategory::NumericEnumCode,
                    location: QualityGapRef::SourceColumn { table: tp.table_name.clone(), column: cs.column_name.clone() },
                    params: params([
                        ("sample_values", cs.sample_values.join(", ")),
                        ("distinct_count", cs.distinct_count.to_string()),
                    ]),
                });
            }

            // Column-level check 3: sparse property (>80% null rate, >10 rows)
            if tp.row_count > 10 && cs.null_count > 0 {
                let null_rate = cs.null_count as f64 / tp.row_count as f64;
                if null_rate > 0.8 {
                    gaps.push(QualityGap {
                        severity: QualityGapSeverity::Low,
                        category: QualityGapCategory::SparseProperty,
                        location: QualityGapRef::SourceColumn { table: tp.table_name.clone(), column: cs.column_name.clone() },
                        params: params([
                            ("column", cs.column_name.clone()),
                            ("null_pct", format!("{:.0}", null_rate * 100.0)),
                            ("null_count", cs.null_count.to_string()),
                            ("row_count", tp.row_count.to_string()),
                        ]),
                    });
                }
            }

            // Column-level check 4: single value observed across many rows — possible sample bias.
            if cs.distinct_count == 1 && tp.row_count >= 5 {
                let observed = cs.sample_values.first().map(String::as_str).unwrap_or("?");
                gaps.push(QualityGap {
                    severity: QualityGapSeverity::Medium,
                    category: QualityGapCategory::SingleValueBias,
                    location: QualityGapRef::SourceColumn { table: tp.table_name.clone(), column: cs.column_name.clone() },
                    params: params([
                        ("column", cs.column_name.clone()),
                        ("observed_value", observed.to_string()),
                        ("row_count", tp.row_count.to_string()),
                    ]),
                });
            }
        }
    }

    if let Some(schema) = schema {
        // Build source_table → node index lookup once for all source-level
        // checks. Walks the canonical object_mappings slice directly —
        // each ObjectMappingDef pairs exactly one node type with exactly
        // one source relation.
        let table_to_node_idx: std::collections::HashMap<String, usize> = ontology
            .node_types
            .iter()
            .enumerate()
            .filter_map(|(i, node)| {
                object_mappings
                    .iter()
                    .find(|om| om.node_type_id == node.id)
                    .map(|om| (om.relation.to_ascii_lowercase(), i))
            })
            .collect();

        let has_source_table_mapping = !table_to_node_idx.is_empty();

        // Check if a table is naturally represented as an edge in the ontology.
        // A table with 2+ FKs whose targets are connected by an ontology edge
        // is considered "represented" — even if it has data columns (which become edge properties).
        let is_edge_source_table = |table_name: &str| -> bool {
            // Check if any edge in the ontology has source/target nodes whose
            // source_table matches, indicating this table is represented via edges.
            // Also check if the node.source_table on any node matches (already covered
            // by table_to_node_idx), or if the table name appears as a NodeTypeDef.source_table
            // that is used by an edge.
            //
            // The key case: a table like `order_items` is a junction table between
            // `orders` and `products`. It becomes an edge, not a node.
            // Check if the FKs from this table point to nodes that have an edge between them.
            let table_fks: Vec<_> = schema
                .foreign_keys
                .iter()
                .filter(|fk| fk.from_table.eq_ignore_ascii_case(table_name))
                .collect();

            if table_fks.len() < 2 {
                return false;
            }

            // For each pair of FK targets, check if there's an edge connecting those nodes
            for i in 0..table_fks.len() {
                for j in (i + 1)..table_fks.len() {
                    let to_node_a =
                        table_to_node_idx.get(&table_fks[i].to_table.to_ascii_lowercase());
                    let to_node_b =
                        table_to_node_idx.get(&table_fks[j].to_table.to_ascii_lowercase());
                    if let (Some(&idx_a), Some(&idx_b)) = (to_node_a, to_node_b) {
                        let node_a_id = &ontology.node_types[idx_a].id;
                        let node_b_id = &ontology.node_types[idx_b].id;
                        let has_edge = ontology.edge_types.iter().any(|e| {
                            (e.source_node_id == *node_a_id && e.target_node_id == *node_b_id)
                                || (e.source_node_id == *node_b_id
                                    && e.target_node_id == *node_a_id)
                        });
                        if has_edge {
                            return true;
                        }
                    }
                }
            }
            false
        };

        for table in &schema.tables {
            if is_excluded(&table.name, &excluded_tables) {
                continue;
            }

            if !has_source_table_mapping {
                continue;
            }

            let has_node = table_to_node_idx.contains_key(&table.name.to_ascii_lowercase());

            if !has_node {
                // Skip tables that are represented as edges (join/bridge tables with 2+ FKs
                // where the FK targets are connected by an edge in the ontology)
                if is_edge_source_table(&table.name) {
                    continue;
                }

                gaps.push(QualityGap {
                    severity: QualityGapSeverity::High,
                    category: QualityGapCategory::UnmappedSourceTable,
                    location: QualityGapRef::SourceTable { table: table.name.clone() },
                    params: params([("table", table.name.clone())]),
                });
            }
        }

        // Group FKs by (from_node_label, to_node_label) pair to detect multi-FK gaps.
        if has_source_table_mapping {
            let mut fk_groups: std::collections::HashMap<
                (String, String),
                Vec<&ox_core::source_schema::ForeignKeyDef>,
            > = std::collections::HashMap::new();

            for fk in &schema.foreign_keys {
                if is_excluded(&fk.from_table, &excluded_tables)
                    || is_excluded(&fk.to_table, &excluded_tables)
                {
                    continue;
                }
                let from_idx = table_to_node_idx.get(&fk.from_table.to_ascii_lowercase());
                let to_idx = table_to_node_idx.get(&fk.to_table.to_ascii_lowercase());
                if let (Some(&fi), Some(&ti)) = (from_idx, to_idx) {
                    let key = (
                        ontology.node_types[fi].id.to_string(),
                        ontology.node_types[ti].id.to_string(),
                    );
                    fk_groups.entry(key).or_default().push(fk);
                }
            }

            for ((from_node_id, to_node_id), fks) in &fk_groups {
                // Check both directions: FK direction (from→to) and semantic reverse (to→from)
                // because edge direction in an ontology is semantic, not necessarily matching FK direction.
                let edge_count = ontology
                    .edge_types
                    .iter()
                    .filter(|edge| {
                        (edge.source_node_id == *from_node_id && edge.target_node_id == *to_node_id)
                            || (edge.source_node_id == *to_node_id
                                && edge.target_node_id == *from_node_id)
                    })
                    .count();

                if edge_count >= fks.len() {
                    continue;
                }

                for fk in fks.iter().skip(edge_count) {
                    let (severity, category) = if fk.inferred {
                        (
                            QualityGapSeverity::Medium,
                            QualityGapCategory::MissingContainmentEdge,
                        )
                    } else {
                        (
                            QualityGapSeverity::High,
                            QualityGapCategory::MissingForeignKeyEdge,
                        )
                    };
                    gaps.push(QualityGap {
                        severity,
                        category,
                        location: QualityGapRef::SourceForeignKey {
                            from_table: fk.from_table.clone(),
                            from_column: fk.from_column.clone(),
                            to_table: fk.to_table.clone(),
                            to_column: fk.to_column.clone(),
                        },
                        params: params([
                            ("from_table", fk.from_table.clone()),
                            ("from_column", fk.from_column.clone()),
                            ("to_table", fk.to_table.clone()),
                            ("to_column", fk.to_column.clone()),
                            ("fk_count", fks.len().to_string()),
                            ("edge_count", edge_count.to_string()),
                        ]),
                    });
                }
            }
        }

        // Source-to-ontology property coverage: detect non-key columns with no matching property.
        for table in &schema.tables {
            if is_excluded(&table.name, &excluded_tables) {
                continue;
            }

            let mapped_node = table_to_node_idx
                .get(&table.name.to_ascii_lowercase())
                .map(|&i| &ontology.node_types[i]);

            let Some(node) = mapped_node else {
                continue;
            };

            // Walk this node's ObjectMappingDef → PropertyMappingDef to
            // collect the source-column set. JSON-path locations contribute
            // no column (same as the legacy flat shape), so they drop out.
            let node_om = object_mappings
                .iter()
                .find(|om| om.node_type_id == node.id);
            let mapped_source_columns: std::collections::HashSet<String> = node_om
                .into_iter()
                .flat_map(|om| om.property_mappings.iter())
                .filter_map(|pm| match &pm.location {
                    PropertyLocation::Column(col) => Some(col.column.to_ascii_lowercase()),
                    PropertyLocation::JsonPath { .. } => None,
                })
                .collect();

            let node_prop_names: std::collections::HashSet<String> = node
                .properties
                .iter()
                .map(|p| p.name.to_ascii_lowercase())
                .collect();

            for col in &table.columns {
                let is_pk = table.primary_key.contains(&col.name);
                let is_fk = schema.foreign_keys.iter().any(|fk| {
                    fk.from_table.eq_ignore_ascii_case(&table.name)
                        && fk.from_column.eq_ignore_ascii_case(&col.name)
                });
                if is_pk || is_fk {
                    continue;
                }

                let col_lower = col.name.to_ascii_lowercase();

                if mapped_source_columns.contains(&col_lower) {
                    continue;
                }

                let trimmed = col_lower.trim_end_matches("_id").to_string();
                let has_name_match =
                    node_prop_names.contains(&col_lower) || node_prop_names.contains(&trimmed);

                if !has_name_match {
                    gaps.push(QualityGap {
                        severity: QualityGapSeverity::Medium,
                        category: QualityGapCategory::UnmappedSourceColumn,
                        location: QualityGapRef::SourceColumn { table: table.name.clone(), column: col.name.clone() },
                        params: params([
                            ("table", table.name.clone()),
                            ("column", col.name.clone()),
                            ("node_label", node.label.to_string()),
                        ]),
                    });
                }
            }
        }
    }

    // Ontology-level check: duplicate edges between the same node pair.
    {
        let mut edge_pairs: std::collections::HashMap<(String, String), Vec<&str>> =
            std::collections::HashMap::new();
        for edge in &ontology.edge_types {
            // Normalize pair: canonical order for undirected comparison
            let (a, b) = (
                edge.source_node_id.to_string(),
                edge.target_node_id.to_string(),
            );
            let pair = if a <= b { (a, b) } else { (b, a) };
            edge_pairs.entry(pair).or_default().push(&edge.label);
        }
        for ((node_a, node_b), labels) in &edge_pairs {
            if labels.len() > 1 {
                // Check if these could be semantically distinct (e.g., TREATS vs AGGRAVATES)
                // If all labels share a common verb root, they're likely duplicates
                // Simple heuristic: flag when there are 3+ edges, or when there are 2 edges
                // with neither being a clear antonym pair
                let label_a = ontology
                    .node_types
                    .iter()
                    .find(|n| *node_a == n.id.as_ref())
                    .map(|n| &n.label);
                let label_b = ontology
                    .node_types
                    .iter()
                    .find(|n| *node_b == n.id.as_ref())
                    .map(|n| &n.label);
                let a_label = label_a.map(|l| l.as_str()).unwrap_or("?");
                let b_label = label_b.map(|l| l.as_str()).unwrap_or("?");

                // Only flag if there are 3+ edges between same pair (2 could be legitimate antonyms)
                if labels.len() >= 3 {
                    gaps.push(QualityGap {
                        severity: QualityGapSeverity::Medium,
                        category: QualityGapCategory::DuplicateEdge,
                        location: QualityGapRef::Node {
                            node_id: node_a.clone(),
                            label: a_label.to_string(),
                        },
                        params: params([
                            ("node_a", a_label.to_string()),
                            ("node_b", b_label.to_string()),
                            ("edge_count", labels.len().to_string()),
                            ("edge_labels", labels.join(", ")),
                        ]),
                    });
                }
            }
        }
    }

    // Ontology-level check: nodes without description.
    for node in &ontology.node_types {
        if node.description.as_str().trim().is_empty() {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Medium,
                category: QualityGapCategory::MissingDescription,
                location: QualityGapRef::Node {
                    node_id: node.id.to_string(),
                    label: node.label.to_string(),
                },
                params: params([("label", node.label.to_string())]),
            });
        }

        // Node property descriptions
        for prop in &node.properties {
            if prop.description.as_str().trim().is_empty() {
                gaps.push(QualityGap {
                    severity: QualityGapSeverity::Low,
                    category: QualityGapCategory::MissingDescription,
                    location: QualityGapRef::NodeProperty {
                        node_id: node.id.to_string(),
                        property_id: prop.id.to_string(),
                        label: node.label.to_string(),
                        property_name: prop.name.to_string(),
                    },
                    params: params([
                        ("label", node.label.to_string()),
                        ("property_name", prop.name.to_string()),
                    ]),
                });
            }
        }
    }

    // Ontology-level check: edges without description.
    for edge in &ontology.edge_types {
        if edge.description.as_str().trim().is_empty() {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::MissingDescription,
                location: QualityGapRef::Edge {
                    edge_id: edge.id.to_string(),
                    label: edge.label.to_string(),
                },
                params: params([("label", edge.label.to_string())]),
            });
        }

        // Edge property descriptions
        for prop in &edge.properties {
            if prop.description.as_str().trim().is_empty() {
                gaps.push(QualityGap {
                    severity: QualityGapSeverity::Low,
                    category: QualityGapCategory::MissingDescription,
                    location: QualityGapRef::EdgeProperty {
                        edge_id: edge.id.to_string(),
                        property_id: prop.id.to_string(),
                        label: edge.label.to_string(),
                        property_name: prop.name.to_string(),
                    },
                    params: params([
                        ("label", edge.label.to_string()),
                        ("property_name", prop.name.to_string()),
                    ]),
                });
            }
        }
    }

    // --- Graph-structural quality checks (ontology-only, no source data needed) ---

    detect_orphan_nodes(ontology, &mut gaps);
    detect_property_type_inconsistency(ontology, &mut gaps);
    detect_hub_nodes(ontology, &mut gaps);
    detect_overloaded_properties(ontology, &mut gaps);
    detect_self_referential_edges(ontology, &mut gaps);

    gaps.sort_by_key(|g| match g.severity {
        QualityGapSeverity::High => 0,
        QualityGapSeverity::Medium => 1,
        QualityGapSeverity::Low => 2,
    });

    let confidence = if gaps
        .iter()
        .any(|g| matches!(g.severity, QualityGapSeverity::High))
    {
        QualityConfidence::Low
    } else if gaps
        .iter()
        .any(|g| matches!(g.severity, QualityGapSeverity::Medium))
    {
        QualityConfidence::Medium
    } else {
        QualityConfidence::High
    };

    OntologyQualityReport { confidence, gaps }
}

// ---------------------------------------------------------------------------
// Graph-structural quality detectors
// ---------------------------------------------------------------------------

/// Detect node types with no incoming or outgoing edges.
/// An orphan node is disconnected from the rest of the graph — likely a design error
/// unless the ontology has only one node type.
fn detect_orphan_nodes(ontology: &OntologyIR, gaps: &mut Vec<QualityGap>) {
    // Skip if there's only one node type (trivially no edges needed)
    if ontology.node_types.len() <= 1 {
        return;
    }

    for node in &ontology.node_types {
        let has_edge = ontology
            .edge_types
            .iter()
            .any(|e| e.source_node_id == node.id || e.target_node_id == node.id);

        if !has_edge {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Medium,
                category: QualityGapCategory::OrphanNode,
                location: QualityGapRef::Node {
                    node_id: node.id.to_string(),
                    label: node.label.to_string(),
                },
                params: params([("label", node.label.to_string())]),
            });
        }
    }
}

/// Detect the same property name used with different types across node types.
/// E.g., `email` as String on Customer but Int on Supplier is suspicious.
fn detect_property_type_inconsistency(ontology: &OntologyIR, gaps: &mut Vec<QualityGap>) {
    // Collect property name → list of (node_label, property_type)
    let mut prop_types: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for node in &ontology.node_types {
        for prop in &node.properties {
            let name_lower = prop.name.to_lowercase();
            let type_str = prop.property_type.to_string();
            prop_types
                .entry(name_lower)
                .or_default()
                .push((node.label.to_string(), type_str));
        }
    }

    for (prop_name, usages) in &prop_types {
        if usages.len() < 2 {
            continue;
        }

        // Check if all usages have the same type
        let first_type = &usages[0].1;
        let inconsistent: Vec<_> = usages.iter().filter(|(_, t)| t != first_type).collect();

        if !inconsistent.is_empty() {
            let all_usages: Vec<String> = usages
                .iter()
                .map(|(label, t)| format!("{label}: {t}"))
                .collect();

            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::PropertyTypeInconsistency,
                location: QualityGapRef::Node {
                    node_id: usages[0].0.clone(),
                    label: prop_name.clone(),
                },
                params: params([
                    ("property_name", prop_name.clone()),
                    ("all_usages", all_usages.join(", ")),
                ]),
            });
        }
    }
}

/// Hub node threshold: a node with more than this many edges (in + out) is flagged.
const HUB_NODE_EDGE_THRESHOLD: usize = 8;

/// Detect node types with an unusually high number of edges.
/// A hub node with many connections may indicate a god-node that should be split.
fn detect_hub_nodes(ontology: &OntologyIR, gaps: &mut Vec<QualityGap>) {
    for node in &ontology.node_types {
        let edge_count = ontology
            .edge_types
            .iter()
            .filter(|e| e.source_node_id == node.id || e.target_node_id == node.id)
            .count();

        if edge_count > HUB_NODE_EDGE_THRESHOLD {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::HubNode,
                location: QualityGapRef::Node {
                    node_id: node.id.to_string(),
                    label: node.label.to_string(),
                },
                params: params([
                    ("label", node.label.to_string()),
                    ("edge_count", edge_count.to_string()),
                    ("threshold", HUB_NODE_EDGE_THRESHOLD.to_string()),
                ]),
            });
        }
    }
}

/// Overloaded property threshold: a property name on more than this many node types is flagged.
const OVERLOADED_PROPERTY_THRESHOLD: usize = 3;

/// Detect properties that appear on many different node types.
/// A ubiquitous property (e.g., `status` on 4+ nodes) might be better modeled
/// as a separate node or enum pattern.
fn detect_overloaded_properties(ontology: &OntologyIR, gaps: &mut Vec<QualityGap>) {
    let mut prop_nodes: HashMap<String, Vec<String>> = HashMap::new();

    for node in &ontology.node_types {
        for prop in &node.properties {
            let name_lower = prop.name.to_lowercase();
            prop_nodes
                .entry(name_lower)
                .or_default()
                .push(node.label.to_string());
        }
    }

    for (prop_name, node_labels) in &prop_nodes {
        if node_labels.len() > OVERLOADED_PROPERTY_THRESHOLD {
            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::OverloadedProperty,
                location: QualityGapRef::Node {
                    node_id: node_labels[0].clone(),
                    label: prop_name.clone(),
                },
                params: params([
                    ("property_name", prop_name.clone()),
                    ("node_count", node_labels.len().to_string()),
                    ("node_labels", node_labels.join(", ")),
                ]),
            });
        }
    }
}

/// Detect edge types where source and target are the same node type.
/// Not an error, but worth flagging for review as it represents a recursive relationship.
fn detect_self_referential_edges(ontology: &OntologyIR, gaps: &mut Vec<QualityGap>) {
    for edge in &ontology.edge_types {
        if edge.source_node_id == edge.target_node_id {
            let node_label = ontology.node_label(&edge.source_node_id).unwrap_or("?");

            gaps.push(QualityGap {
                severity: QualityGapSeverity::Low,
                category: QualityGapCategory::SelfReferentialEdge,
                location: QualityGapRef::Edge {
                    edge_id: edge.id.to_string(),
                    label: edge.label.to_string(),
                },
                params: params([
                    ("label", edge.label.to_string()),
                    ("node_label", node_label.to_string()),
                ]),
            });
        }
    }
}
