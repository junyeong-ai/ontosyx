use std::collections::{HashMap, HashSet};

use ox_core::design_project::{SourceConfig, SourceTypeKind};
use ox_core::ontology_input::OntologyInputIR;
use ox_core::source_analysis::{
    AnalysisWarning, DesignOptions, ENUM_CARDINALITY_THRESHOLD, PiiDecision, SourceAnalysisReport,
};
use ox_core::source_schema::{ForeignKeyDef, SourceProfile, SourceSchema};
use ox_core::table_clustering::TableCluster;
use ox_source::analyzer::apply_pii_masking;
use ox_store::DesignProject;

use crate::error::AppError;
use crate::system_config::SystemConfig;

/// Context needed to build LLM input, extracted from a DesignProject.
pub(crate) struct LlmInputContext<'a> {
    pub source_data: Option<&'a str>,
    pub source_schema: Option<&'a serde_json::Value>,
    pub source_profile: Option<&'a serde_json::Value>,
    pub analysis_report: Option<&'a serde_json::Value>,
}

impl<'a> LlmInputContext<'a> {
    /// Extract context from a stored `DesignProject`.
    pub fn from_project(project: &'a DesignProject) -> Self {
        Self {
            source_data: project.source_data.as_deref(),
            source_schema: project.source_schema.as_ref(),
            source_profile: project.source_profile.as_ref(),
            analysis_report: project.analysis_report.as_ref(),
        }
    }
}

/// Build the sample data string for LLM from project context.
pub(crate) fn build_llm_input(
    ctx: &LlmInputContext<'_>,
    source_config: &SourceConfig,
    effective_opts: &DesignOptions,
    config: &SystemConfig,
) -> Result<String, AppError> {
    match source_config.source_type {
        SourceTypeKind::Text => {
            // Text: use raw source_data directly
            Ok(ctx.source_data.unwrap_or_default().to_owned())
        }
        SourceTypeKind::Csv
        | SourceTypeKind::Json
        | SourceTypeKind::Postgresql
        | SourceTypeKind::Mysql
        | SourceTypeKind::Mongodb
        | SourceTypeKind::CodeRepository
        | SourceTypeKind::Ontology => {
            // Structured: format from stored schema + profile
            let schema: SourceSchema = ctx
                .source_schema
                .ok_or_else(|| AppError::bad_request("Project has no source schema"))
                .and_then(|v| {
                    serde_json::from_value(v.clone())
                        .map_err(|e| AppError::internal(format!("Corrupt source_schema: {e}")))
                })?;
            let profile: SourceProfile = ctx
                .source_profile
                .ok_or_else(|| AppError::bad_request("Project has no source profile"))
                .and_then(|v| {
                    serde_json::from_value(v.clone())
                        .map_err(|e| AppError::internal(format!("Corrupt source_profile: {e}")))
                })?;

            let warnings: Vec<AnalysisWarning> = ctx
                .analysis_report
                .and_then(|v| {
                    serde_json::from_value::<SourceAnalysisReport>(v.clone())
                        .map_err(|e| {
                            tracing::warn!("Failed to parse analysis_report: {e}");
                            e
                        })
                        .ok()
                        .map(|r| r.analysis_warnings)
                })
                .unwrap_or_default();

            // Reduce schema: filter excluded tables, then cap detailed tables at max_design_tables.
            // Tables beyond the budget get a compact summary (name, col count, FK refs)
            // so the LLM still sees the full schema landscape.
            let max_tables = config.max_design_tables();
            let reduced =
                reduce_schema_for_llm(schema, profile, &effective_opts.excluded_tables, max_tables);
            let schema = reduced.schema;
            let profile = reduced.profile;
            let dropped_summary = reduced.dropped_summary;

            // Apply PII masking
            let has_mask = effective_opts
                .pii_decisions
                .iter()
                .any(|d| matches!(d.decision, PiiDecision::Mask | PiiDecision::Exclude));

            let masked_profile = if has_mask {
                let mut masked = profile.clone();
                apply_pii_masking(&mut masked, &effective_opts.pii_decisions);
                masked
            } else {
                profile
            };

            // Apply adaptive compression for large schemas
            let large_threshold = config.large_schema_warning_threshold();
            let is_large = schema.tables.len() >= large_threshold;
            let effective_profile = if is_large {
                compact_profile_for_llm(
                    &masked_profile,
                    config.large_schema_sample_values(),
                    config.large_schema_value_chars(),
                )
            } else {
                masked_profile
            };

            let mut text = format_source_for_llm(&schema, &effective_profile, &warnings, is_large)?;
            if !dropped_summary.is_empty() {
                text.push_str(&dropped_summary);
            }
            Ok(text)
        }
    }
}

/// Result of schema reduction: detailed schema/profile for top-N tables,
/// plus a compact summary of any tables that exceeded the detail budget.
struct ReducedSchema {
    schema: SourceSchema,
    profile: SourceProfile,
    /// Compact text summary of tables not included in full detail.
    /// Empty string when all tables fit within the budget.
    dropped_summary: String,
}

/// Reduce a schema to fit within the LLM context budget.
///
/// Two-phase reduction:
/// 1. Remove user-excluded tables (from DesignOptions).
/// 2. If still over `max_tables`, rank tables by FK connectivity and keep the
///    top N with full column detail. Tables that exceed the budget are NOT
///    discarded -- instead they are summarized compactly (name, column count,
///    FK references) so the LLM still sees the entire schema landscape and can
///    create nodes for summarized tables when relevant.
///
/// Both the schema tables/foreign_keys and profile table_profiles are filtered
/// in lockstep for the detailed portion.
fn reduce_schema_for_llm(
    mut schema: SourceSchema,
    mut profile: SourceProfile,
    excluded_tables: &[String],
    max_tables: usize,
) -> ReducedSchema {
    let original_count = schema.tables.len();

    // Phase 1: remove user-excluded tables
    if !excluded_tables.is_empty() {
        let excluded: HashSet<&str> = excluded_tables.iter().map(|s| s.as_str()).collect();
        schema
            .tables
            .retain(|t| !excluded.contains(t.name.as_str()));
        schema.foreign_keys.retain(|fk| {
            !excluded.contains(fk.from_table.as_str()) && !excluded.contains(fk.to_table.as_str())
        });
        profile
            .table_profiles
            .retain(|tp| !excluded.contains(tp.table_name.as_str()));

        if schema.tables.len() < original_count {
            tracing::info!(
                original = original_count,
                after_exclusion = schema.tables.len(),
                excluded = excluded_tables.len(),
                "Removed user-excluded tables from LLM input"
            );
        }
    }

    // Phase 2: cap detailed tables at max_tables; summarize the rest
    if schema.tables.len() <= max_tables {
        return ReducedSchema {
            schema,
            profile,
            dropped_summary: String::new(),
        };
    }

    let before_cap = schema.tables.len();

    // Count FK connections per table (both directions)
    let mut fk_score: HashMap<&str, usize> = HashMap::new();
    for fk in &schema.foreign_keys {
        *fk_score.entry(fk.from_table.as_str()).or_default() += 1;
        *fk_score.entry(fk.to_table.as_str()).or_default() += 1;
    }

    // Also count _id columns as a secondary signal for tables without declared FKs
    for table in &schema.tables {
        let id_cols = table
            .columns
            .iter()
            .filter(|c| c.name.ends_with("_id"))
            .count();
        *fk_score.entry(table.name.as_str()).or_default() += id_cols;
    }

    // Sort table names by score descending, then alphabetically for stability.
    // Collect owned Strings so we can mutate schema afterwards.
    let mut ranked: Vec<String> = schema.tables.iter().map(|t| t.name.clone()).collect();
    ranked.sort_by(|a, b| {
        let sa = fk_score.get(a.as_str()).copied().unwrap_or(0);
        let sb = fk_score.get(b.as_str()).copied().unwrap_or(0);
        sb.cmp(&sa).then_with(|| a.cmp(b))
    });

    let kept: HashSet<String> = ranked.into_iter().take(max_tables).collect();

    // Build compact summary for dropped tables BEFORE removing them from schema
    let dropped_summary = build_dropped_table_summary(
        &schema.tables,
        &schema.foreign_keys,
        &profile.table_profiles,
        &kept,
    );

    let dropped_count = schema.tables.len() - kept.len();

    schema.tables.retain(|t| kept.contains(&t.name));
    schema
        .foreign_keys
        .retain(|fk| kept.contains(&fk.from_table) && kept.contains(&fk.to_table));
    profile
        .table_profiles
        .retain(|tp| kept.contains(&tp.table_name));

    tracing::info!(
        before = before_cap,
        detailed = schema.tables.len(),
        summarized = dropped_count,
        max_tables,
        "Schema reduced: full detail for top tables, compact summary for the rest"
    );

    ReducedSchema {
        schema,
        profile,
        dropped_summary,
    }
}

/// Build a compact text summary for tables that won't get full column detail.
///
/// For each dropped table, emits one line:
///   table_name (N cols, M rows) refs: col_a_id, col_b_id  fk_targets: tableX, tableY
///
/// This gives the LLM enough context to decide whether a summarized table
/// deserves its own node type, without blowing up the token budget.
fn build_dropped_table_summary(
    tables: &[ox_core::source_schema::SourceTableDef],
    foreign_keys: &[ox_core::source_schema::ForeignKeyDef],
    table_profiles: &[ox_core::source_schema::TableProfile],
    kept: &HashSet<String>,
) -> String {
    use std::fmt::Write;

    // Index row counts for quick lookup
    let row_counts: HashMap<&str, u64> = table_profiles
        .iter()
        .map(|tp| (tp.table_name.as_str(), tp.row_count))
        .collect();

    // Index outgoing FK targets per table
    let mut fk_targets: HashMap<&str, Vec<&str>> = HashMap::new();
    for fk in foreign_keys {
        fk_targets
            .entry(fk.from_table.as_str())
            .or_default()
            .push(fk.to_table.as_str());
    }

    let mut lines: Vec<String> = Vec::new();

    for table in tables {
        if kept.contains(&table.name) {
            continue;
        }

        let mut line = String::new();
        let rows = row_counts.get(table.name.as_str()).copied().unwrap_or(0);
        let col_names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        write!(
            line,
            "{} ({} rows) columns: [{}]",
            table.name,
            rows,
            col_names.join(", ")
        )
        .unwrap();

        // _id reference columns (FK signals)
        let ref_cols: Vec<&str> = table
            .columns
            .iter()
            .filter(|c| c.name.ends_with("_id"))
            .map(|c| c.name.as_str())
            .collect();
        if !ref_cols.is_empty() {
            write!(line, " refs: {}", ref_cols.join(", ")).unwrap();
        }

        // Declared FK targets
        if let Some(targets) = fk_targets.get(table.name.as_str()) {
            let unique: Vec<&str> = {
                let mut v: Vec<&str> = targets.clone();
                v.sort_unstable();
                v.dedup();
                v
            };
            write!(line, " fk_targets: {}", unique.join(", ")).unwrap();
        }

        lines.push(line);
    }

    if lines.is_empty() {
        return String::new();
    }

    format!(
        "\n\n--- Additional tables (summary — create nodes/edges for these if relevant) ---\n\
         The following {} tables have column names listed but no type/profile detail. \
         Use column names to design properties. FK reference columns end with _id.\n\n{}",
        lines.len(),
        lines.join("\n"),
    )
}

/// Create compacted clones of profile for LLM input.
///
/// Strategy:
/// - Enum columns (distinct ≤ 100): keep ALL sample values, truncate each to max_value_chars
/// - Non-enum columns: limit to max_sample_values entries, truncate each
/// - min_value/max_value: always truncate to max_value_chars
fn compact_profile_for_llm(
    profile: &SourceProfile,
    max_sample_values: usize,
    max_value_chars: usize,
) -> SourceProfile {
    SourceProfile {
        table_profiles: profile
            .table_profiles
            .iter()
            .map(|tp| ox_core::source_schema::TableProfile {
                table_name: tp.table_name.clone(),
                row_count: tp.row_count,
                column_stats: tp
                    .column_stats
                    .iter()
                    .map(|cs| {
                        let is_enum = cs.distinct_count > 0
                            && cs.distinct_count <= ENUM_CARDINALITY_THRESHOLD;

                        ox_core::source_schema::ColumnStats {
                            column_name: cs.column_name.clone(),
                            null_count: cs.null_count,
                            distinct_count: cs.distinct_count,
                            sample_values: if is_enum {
                                // Enum: keep ALL values, just truncate each
                                cs.sample_values
                                    .iter()
                                    .map(|v| truncate_str(v, max_value_chars))
                                    .collect()
                            } else {
                                // Non-enum: limit count + truncate
                                cs.sample_values
                                    .iter()
                                    .take(max_sample_values)
                                    .map(|v| truncate_str(v, max_value_chars))
                                    .collect()
                            },
                            min_value: cs
                                .min_value
                                .as_ref()
                                .map(|v| truncate_str(v, max_value_chars)),
                            max_value: cs
                                .max_value
                                .as_ref()
                                .map(|v| truncate_str(v, max_value_chars)),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Format source schema + profile as structured text for the LLM.
fn serialize_json<T: serde::Serialize + ?Sized>(
    val: &T,
    label: &str,
    minified: bool,
) -> Result<String, AppError> {
    let result = if minified {
        serde_json::to_string(val)
    } else {
        serde_json::to_string_pretty(val)
    };
    result.map_err(|e| {
        AppError::from(ox_core::OxError::Runtime {
            message: format!("Failed to serialize {label}: {e}"),
        })
    })
}

fn format_source_for_llm(
    schema: &SourceSchema,
    profile: &SourceProfile,
    analysis_warnings: &[AnalysisWarning],
    minified: bool,
) -> Result<String, AppError> {
    let schema_json = serialize_json(schema, "source schema", minified)?;
    let profile_json = serialize_json(profile, "source profile", minified)?;

    let warnings_section = if analysis_warnings.is_empty() {
        String::new()
    } else {
        let json = serialize_json(analysis_warnings, "analysis warnings", minified)?;
        format!("\n\nAnalysis warnings (partial source analysis; omitted objects/stats):\n{json}")
    };

    let schema_label = match schema.source_type.as_str() {
        "postgresql" | "mysql" => "Source database schema (tables, columns, foreign keys)",
        "mongodb" => "Source database schema (collections, fields, inferred relationships)",
        _ => "Source structure (extracted tables, columns, inferred relationships)",
    };

    Ok(format!(
        "{schema_label}:\n{schema_json}\n\n\
         Data statistics (row counts, distinct values, ranges):\n{profile_json}{warnings_section}"
    ))
}

// ---------------------------------------------------------------------------
// Divide-and-conquer batch helpers
// ---------------------------------------------------------------------------

/// Build LLM input for a single batch cluster.
/// Includes only the tables in the cluster with full schema + profile detail.
pub(crate) fn build_batch_llm_input(
    schema: &SourceSchema,
    profile: &SourceProfile,
    cluster: &TableCluster,
    config: &SystemConfig,
) -> Result<String, AppError> {
    let cluster_tables: HashSet<&str> = cluster.tables.iter().map(|s| s.as_str()).collect();

    let batch_schema = SourceSchema {
        source_type: schema.source_type.clone(),
        tables: schema
            .tables
            .iter()
            .filter(|t| cluster_tables.contains(t.name.as_str()))
            .cloned()
            .collect(),
        foreign_keys: cluster
            .internal_fks
            .iter()
            .chain(cluster.cross_fks.iter())
            .cloned()
            .collect(),
    };

    let batch_profile = SourceProfile {
        table_profiles: profile
            .table_profiles
            .iter()
            .filter(|tp| cluster_tables.contains(tp.table_name.as_str()))
            .cloned()
            .collect(),
    };

    // Always apply profile compression for batch inputs — individual tables can have
    // enormous sample_values (e.g., JSON blobs in API log tables) that would exceed
    // the 200K token limit even for a single table.
    let effective_profile = compact_profile_for_llm(
        &batch_profile,
        config.large_schema_sample_values(),
        config.large_schema_value_chars(),
    );

    format_source_for_llm(&batch_schema, &effective_profile, &[], true)
}

/// Format existing nodes from previous batches for the batch prompt.
/// Compact format: `NodeLabel (source_table)` — no descriptions to save tokens.
pub(crate) fn format_existing_nodes(previous_batches: &[OntologyInputIR]) -> String {
    if previous_batches.is_empty() {
        return "(none — this is the first batch)".to_string();
    }

    let mut labels = Vec::new();
    for batch in previous_batches {
        for node in &batch.node_types {
            let source = node
                .source_table
                .as_deref()
                .map(|s| format!("({s})"))
                .unwrap_or_default();
            labels.push(format!("{}{}", node.label, source));
        }
    }
    labels.join(", ")
}

/// Format cross-cluster FK information for the batch prompt.
/// Shows both outbound (this batch → existing) and inbound (existing → this batch) directions.
pub(crate) fn format_cross_fks(
    cross_fks: &[ForeignKeyDef],
    cluster: &TableCluster,
    previous_batches: &[OntologyInputIR],
) -> String {
    if cross_fks.is_empty() {
        return "(none)".to_string();
    }

    // Build source_table → node label mapping from previous batches
    let table_to_label: HashMap<&str, &str> = previous_batches
        .iter()
        .flat_map(|b| b.node_types.iter())
        .filter_map(|n| n.source_table.as_deref().map(|t| (t, n.label.as_str())))
        .collect();

    let cluster_tables: HashSet<&str> = cluster.tables.iter().map(|s| s.as_str()).collect();

    let mut lines = Vec::new();
    for fk in cross_fks {
        let from_in_batch = cluster_tables.contains(fk.from_table.as_str());
        let to_in_batch = cluster_tables.contains(fk.to_table.as_str());

        if from_in_batch && !to_in_batch {
            // Outbound: this batch references external table
            let label = table_to_label
                .get(fk.to_table.as_str())
                .copied()
                .unwrap_or("(unknown)");
            lines.push(format!(
                "→ outbound: {}.{} → {} [existing: {}]",
                fk.from_table, fk.from_column, fk.to_table, label
            ));
        } else if !from_in_batch && to_in_batch {
            // Inbound: external table references this batch
            let label = table_to_label
                .get(fk.from_table.as_str())
                .copied()
                .unwrap_or("(unknown)");
            lines.push(format!(
                "← inbound: [existing: {}].{} → {}.{}",
                label, fk.from_column, fk.to_table, fk.to_column
            ));
        }
    }

    if lines.is_empty() {
        "(none)".to_string()
    } else {
        lines.join("\n")
    }
}

/// Merge multiple batch OntologyInputIR results into a single InputIR.
/// Deduplicates by label (nodes) and (label, source_type, target_type) (edges).
pub(crate) fn merge_input_irs(
    batches: Vec<OntologyInputIR>,
    name: &str,
    description: Option<&str>,
) -> OntologyInputIR {
    let mut node_types = Vec::new();
    let mut edge_types = Vec::new();
    let mut indexes = Vec::new();

    let mut seen_nodes: HashSet<String> = HashSet::new();
    let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();
    let mut seen_indexes: HashSet<String> = HashSet::new();

    for batch in batches {
        for node in batch.node_types {
            if seen_nodes.insert(node.label.clone()) {
                node_types.push(node);
            } else {
                tracing::warn!(label = %node.label, "Duplicate node label across batches — keeping first");
            }
        }

        for edge in batch.edge_types {
            let key = (
                edge.label.clone(),
                edge.source_type.clone(),
                edge.target_type.clone(),
            );
            if seen_edges.insert(key) {
                edge_types.push(edge);
            } else {
                tracing::warn!(
                    label = %edge.label,
                    source = %edge.source_type,
                    target = %edge.target_type,
                    "Duplicate edge across batches — keeping first"
                );
            }
        }

        for idx in batch.indexes {
            // Serialize to stable JSON for dedup (Debug format is fragile)
            let key = serde_json::to_string(&idx).unwrap_or_default();
            if seen_indexes.insert(key) {
                indexes.push(idx);
            }
        }
    }

    OntologyInputIR {
        format_version: 1,
        id: None,
        name: name.to_string(),
        description: description.map(|d| d.to_string()),
        version: 1,
        node_types,
        edge_types,
        indexes,
    }
}

/// Find cross-cluster FKs not covered by any edge in the merged InputIR.
pub(crate) fn find_uncovered_cross_fks(
    merged: &OntologyInputIR,
    all_cross_fks: &[ForeignKeyDef],
) -> Vec<ForeignKeyDef> {
    // Build source_table → node label mapping
    let table_to_label: HashMap<&str, &str> = merged
        .node_types
        .iter()
        .filter_map(|n| n.source_table.as_deref().map(|t| (t, n.label.as_str())))
        .collect();

    // Collect existing edge (source_type, target_type) pairs
    let existing_edges: HashSet<(&str, &str)> = merged
        .edge_types
        .iter()
        .map(|e| (e.source_type.as_str(), e.target_type.as_str()))
        .collect();

    let mut uncovered = Vec::new();
    let mut seen = HashSet::new();

    for fk in all_cross_fks {
        let from_label = table_to_label.get(fk.from_table.as_str()).copied();
        let to_label = table_to_label.get(fk.to_table.as_str()).copied();

        if let (Some(fl), Some(tl)) = (from_label, to_label) {
            // Check both directions since edge direction may differ from FK direction
            if !existing_edges.contains(&(fl, tl)) && !existing_edges.contains(&(tl, fl)) {
                let key = (fk.from_table.clone(), fk.to_table.clone());
                if seen.insert(key) {
                    uncovered.push(fk.clone());
                }
            }
        }
    }

    uncovered
}

/// Format all node labels for the edge resolution prompt.
pub(crate) fn format_node_labels_for_resolution(merged: &OntologyInputIR) -> String {
    merged
        .node_types
        .iter()
        .map(|n| {
            let source = n
                .source_table
                .as_deref()
                .map(|s| format!(" (source: {s})"))
                .unwrap_or_default();
            format!("- {}{}", n.label, source)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format existing edges for the edge resolution prompt.
pub(crate) fn format_existing_edges_for_resolution(merged: &OntologyInputIR) -> String {
    merged
        .edge_types
        .iter()
        .map(|e| format!("- ({})−[:{}]→({})", e.source_type, e.label, e.target_type))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format uncovered FK relationships for the edge resolution prompt.
pub(crate) fn format_uncovered_fks(
    uncovered: &[ForeignKeyDef],
    merged: &OntologyInputIR,
) -> String {
    let table_to_label: HashMap<&str, &str> = merged
        .node_types
        .iter()
        .filter_map(|n| n.source_table.as_deref().map(|t| (t, n.label.as_str())))
        .collect();

    uncovered
        .iter()
        .map(|fk| {
            let from_label = table_to_label
                .get(fk.from_table.as_str())
                .copied()
                .unwrap_or("?");
            let to_label = table_to_label
                .get(fk.to_table.as_str())
                .copied()
                .unwrap_or("?");
            format!(
                "- {}.{} → {}.{} (nodes: {} → {})",
                fk.from_table, fk.from_column, fk.to_table, fk.to_column, from_label, to_label
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::source_schema::{
        ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef, TableProfile,
    };

    fn make_table(name: &str, id_cols: &[&str]) -> SourceTableDef {
        let mut columns = vec![SourceColumnDef {
            name: "id".to_string(),
            data_type: "int4".to_string(),
            nullable: false,
        }];
        for col in id_cols {
            columns.push(SourceColumnDef {
                name: col.to_string(),
                data_type: "int4".to_string(),
                nullable: true,
            });
        }
        SourceTableDef {
            name: name.to_string(),
            columns,
            primary_key: vec!["id".to_string()],
        }
    }

    fn make_profile(name: &str) -> TableProfile {
        TableProfile {
            table_name: name.to_string(),
            row_count: 100,
            column_stats: vec![ColumnStats {
                column_name: "id".to_string(),
                null_count: 0,
                distinct_count: 100,
                sample_values: vec![],
                min_value: None,
                max_value: None,
            }],
        }
    }

    fn make_fk(from: &str, to: &str) -> ForeignKeyDef {
        ForeignKeyDef {
            from_table: from.to_string(),
            from_column: format!("{to}_id"),
            to_table: to.to_string(),
            to_column: "id".to_string(),
            inferred: false,
        }
    }

    #[test]
    fn no_reduction_when_under_limit() {
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![make_table("users", &[]), make_table("orders", &["user_id"])],
            foreign_keys: vec![make_fk("orders", "users")],
        };
        let profile = SourceProfile {
            table_profiles: vec![make_profile("users"), make_profile("orders")],
        };

        let reduced = reduce_schema_for_llm(schema, profile, &[], 10);

        assert_eq!(reduced.schema.tables.len(), 2);
        assert_eq!(reduced.profile.table_profiles.len(), 2);
        assert_eq!(reduced.schema.foreign_keys.len(), 1);
        assert!(reduced.dropped_summary.is_empty());
    }

    #[test]
    fn excluded_tables_removed() {
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![
                make_table("users", &[]),
                make_table("orders", &["user_id"]),
                make_table("audit_log", &["user_id"]),
            ],
            foreign_keys: vec![make_fk("orders", "users"), make_fk("audit_log", "users")],
        };
        let profile = SourceProfile {
            table_profiles: vec![
                make_profile("users"),
                make_profile("orders"),
                make_profile("audit_log"),
            ],
        };
        let excluded = vec!["audit_log".to_string()];

        let reduced = reduce_schema_for_llm(schema, profile, &excluded, 100);

        assert_eq!(reduced.schema.tables.len(), 2);
        assert!(reduced.schema.tables.iter().all(|t| t.name != "audit_log"));
        assert_eq!(reduced.profile.table_profiles.len(), 2);
        // FK from audit_log -> users should also be removed
        assert_eq!(reduced.schema.foreign_keys.len(), 1);
        assert_eq!(reduced.schema.foreign_keys[0].from_table, "orders");
        // No tables exceeded the budget, so no summary
        assert!(reduced.dropped_summary.is_empty());
    }

    #[test]
    fn cap_keeps_most_connected_tables_with_summary() {
        // Create 5 tables: "hub" has FKs from 3 others, "leaf" has none
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![
                make_table("hub", &[]),
                make_table("spoke_a", &["hub_id"]),
                make_table("spoke_b", &["hub_id"]),
                make_table("spoke_c", &["hub_id"]),
                make_table("leaf", &[]),
            ],
            foreign_keys: vec![
                make_fk("spoke_a", "hub"),
                make_fk("spoke_b", "hub"),
                make_fk("spoke_c", "hub"),
            ],
        };
        let profile = SourceProfile {
            table_profiles: vec![
                make_profile("hub"),
                make_profile("spoke_a"),
                make_profile("spoke_b"),
                make_profile("spoke_c"),
                make_profile("leaf"),
            ],
        };

        // Cap at 3 tables: should keep hub + 2 spokes (all tied), drop leaf + 1 spoke
        let reduced = reduce_schema_for_llm(schema, profile, &[], 3);

        assert_eq!(reduced.schema.tables.len(), 3);
        assert_eq!(reduced.profile.table_profiles.len(), 3);
        // Hub must be kept (highest FK score: 3 incoming FKs)
        assert!(reduced.schema.tables.iter().any(|t| t.name == "hub"));
        // Leaf must be dropped (0 connections) — but should appear in summary
        assert!(!reduced.schema.tables.iter().any(|t| t.name == "leaf"));
        // Summary should mention dropped tables
        assert!(!reduced.dropped_summary.is_empty());
        assert!(reduced.dropped_summary.contains("leaf"));
    }

    #[test]
    fn excluded_then_cap_combined() {
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![
                make_table("users", &[]),
                make_table("orders", &["user_id"]),
                make_table("items", &["order_id"]),
                make_table("audit_log", &["user_id"]),
                make_table("temp_table", &[]),
            ],
            foreign_keys: vec![
                make_fk("orders", "users"),
                make_fk("items", "orders"),
                make_fk("audit_log", "users"),
            ],
        };
        let profile = SourceProfile {
            table_profiles: vec![
                make_profile("users"),
                make_profile("orders"),
                make_profile("items"),
                make_profile("audit_log"),
                make_profile("temp_table"),
            ],
        };
        let excluded = vec!["audit_log".to_string()];

        // After exclusion: 4 tables (users, orders, items, temp_table). Cap at 3.
        let reduced = reduce_schema_for_llm(schema, profile, &excluded, 3);

        assert_eq!(reduced.schema.tables.len(), 3);
        assert_eq!(reduced.profile.table_profiles.len(), 3);
        assert!(!reduced.schema.tables.iter().any(|t| t.name == "audit_log"));
        // temp_table has no connections, should be dropped to summary
        assert!(!reduced.schema.tables.iter().any(|t| t.name == "temp_table"));
        // temp_table should appear in summary (not silently lost)
        assert!(reduced.dropped_summary.contains("temp_table"));
        // audit_log was user-excluded — should NOT appear in summary
        assert!(!reduced.dropped_summary.contains("audit_log"));
    }

    #[test]
    fn fk_filtering_removes_dangling_references() {
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![
                make_table("a", &[]),
                make_table("b", &["a_id"]),
                make_table("c", &[]),
            ],
            foreign_keys: vec![make_fk("b", "a"), make_fk("b", "c")],
        };
        let profile = SourceProfile {
            table_profiles: vec![make_profile("a"), make_profile("b"), make_profile("c")],
        };

        // Cap at 2: b has highest score (2 FK refs + 1 _id col = 3), a has 1 FK ref, c has 1 FK ref
        // Tie-break alphabetically: a < c, so keep b + a
        let reduced = reduce_schema_for_llm(schema, profile, &[], 2);

        assert_eq!(reduced.schema.tables.len(), 2);
        // FK b->c should be removed since c is dropped from detail
        for fk in &reduced.schema.foreign_keys {
            assert_ne!(fk.to_table, "c");
            assert_ne!(fk.from_table, "c");
        }
        // c should appear in summary
        assert!(reduced.dropped_summary.contains("c ("));
    }

    #[test]
    fn dropped_summary_includes_fk_and_ref_info() {
        let schema = SourceSchema {
            source_type: "postgresql".to_string(),
            tables: vec![
                make_table("hub", &[]),
                make_table("detail", &["hub_id", "category_id"]),
                make_table("category", &[]),
            ],
            foreign_keys: vec![make_fk("detail", "hub"), make_fk("detail", "category")],
        };
        let profile = SourceProfile {
            table_profiles: vec![
                make_profile("hub"),
                make_profile("detail"),
                make_profile("category"),
            ],
        };

        // Cap at 1: detail has highest score → keep detail, drop hub + category
        let reduced = reduce_schema_for_llm(schema, profile, &[], 1);

        assert_eq!(reduced.schema.tables.len(), 1);
        assert_eq!(reduced.schema.tables[0].name, "detail");

        // Summary should show hub and category with their metadata
        let summary = &reduced.dropped_summary;
        assert!(summary.contains("hub (100 rows)"));
        assert!(summary.contains("category (100 rows)"));
        // Neither hub nor category have _id ref cols or outgoing FKs
    }

    // -- Batch merge tests ---------------------------------------------------

    fn make_input_ir(
        name: &str,
        nodes: Vec<(&str, Option<&str>)>,
        edges: Vec<(&str, &str, &str)>,
    ) -> OntologyInputIR {
        use ox_core::ontology_input::{InputEdgeTypeDef, InputNodeTypeDef, InputPropertyDef};
        use ox_core::ontology_ir::Cardinality;

        OntologyInputIR {
            format_version: 1,
            id: None,
            name: name.to_string(),
            description: None,
            version: 1,
            node_types: nodes
                .iter()
                .map(|(label, source)| InputNodeTypeDef {
                    id: None,
                    label: label.to_string(),
                    description: None,
                    source_table: source.map(|s| s.to_string()),
                    properties: vec![InputPropertyDef {
                        id: None,
                        name: "id".to_string(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: None,
                        source_column: None,
                    }],
                    constraints: vec![],
                })
                .collect(),
            edge_types: edges
                .iter()
                .map(|(label, src, tgt)| InputEdgeTypeDef {
                    id: None,
                    label: label.to_string(),
                    description: None,
                    source_type: src.to_string(),
                    target_type: tgt.to_string(),
                    properties: vec![],
                    cardinality: Cardinality::ManyToOne,
                })
                .collect(),
            indexes: vec![],
        }
    }

    #[test]
    fn merge_input_irs_deduplicates_nodes() {
        let batch1 = make_input_ir(
            "b1",
            vec![("Customer", Some("customers")), ("Order", Some("orders"))],
            vec![("PLACED", "Customer", "Order")],
        );
        let batch2 = make_input_ir(
            "b2",
            vec![
                ("Product", Some("products")),
                ("Customer", Some("customers")),
            ], // duplicate Customer
            vec![("CONTAINS", "Order", "Product")],
        );

        let merged = merge_input_irs(vec![batch1, batch2], "test", None);

        // Customer should appear only once (first-batch-wins)
        assert_eq!(merged.node_types.len(), 3);
        assert_eq!(merged.edge_types.len(), 2);
        let labels: Vec<&str> = merged.node_types.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, vec!["Customer", "Order", "Product"]);
    }

    #[test]
    fn merge_input_irs_deduplicates_edges() {
        let batch1 = make_input_ir("b1", vec![("A", Some("a"))], vec![("REL", "A", "B")]);
        let batch2 = make_input_ir(
            "b2",
            vec![("B", Some("b"))],
            vec![("REL", "A", "B")], // duplicate edge
        );

        let merged = merge_input_irs(vec![batch1, batch2], "test", None);
        assert_eq!(merged.edge_types.len(), 1);
    }

    #[test]
    fn merge_input_irs_empty_batches() {
        let merged = merge_input_irs(vec![], "empty", Some("Empty merge"));
        assert!(merged.node_types.is_empty());
        assert!(merged.edge_types.is_empty());
        assert_eq!(merged.name, "empty");
        assert_eq!(merged.description.as_deref(), Some("Empty merge"));
    }

    #[test]
    fn find_uncovered_cross_fks_detects_missing_edges() {
        let merged = make_input_ir(
            "test",
            vec![
                ("Customer", Some("customers")),
                ("Order", Some("orders")),
                ("Product", Some("products")),
            ],
            vec![("PLACED", "Customer", "Order")], // Order→Product edge missing
        );

        let cross_fks = vec![
            make_fk("orders", "customers"), // covered by PLACED
            make_fk("orders", "products"),  // NOT covered
        ];

        let uncovered = find_uncovered_cross_fks(&merged, &cross_fks);
        assert_eq!(uncovered.len(), 1);
        assert_eq!(uncovered[0].from_table, "orders");
        assert_eq!(uncovered[0].to_table, "products");
    }

    #[test]
    fn find_uncovered_cross_fks_empty_when_all_covered() {
        let merged = make_input_ir(
            "test",
            vec![("Customer", Some("customers")), ("Order", Some("orders"))],
            vec![("PLACED", "Customer", "Order")],
        );

        let cross_fks = vec![make_fk("orders", "customers")];

        let uncovered = find_uncovered_cross_fks(&merged, &cross_fks);
        assert!(uncovered.is_empty());
    }
}
