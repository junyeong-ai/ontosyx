use std::collections::{HashMap, HashSet};

use ox_core::source_analysis::{DesignOptions, SourceAnalysisReport};
use ox_core::source_schema::SourceSchema;

use crate::error::AppError;

/// Validate that all decisions reference tables/columns that exist in the schema.
/// This catches client bugs, stale state, and typos before persisting invalid decisions.
pub(crate) fn validate_decisions(
    options: &DesignOptions,
    schema: &SourceSchema,
) -> Result<(), AppError> {
    // Build lookup: table_name -> set of column names
    let table_columns: HashMap<&str, HashSet<&str>> = schema
        .tables
        .iter()
        .map(|t| {
            let cols: HashSet<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
            (t.name.as_str(), cols)
        })
        .collect();

    let mut invalid = Vec::new();

    for d in &options.pii_decisions {
        match table_columns.get(d.table.as_str()) {
            None => invalid.push(format!("pii_decision: table '{}' not found", d.table)),
            Some(cols) if !cols.contains(d.column.as_str()) => {
                invalid.push(format!(
                    "pii_decision: column '{}.{}' not found",
                    d.table, d.column
                ));
            }
            _ => {}
        }
    }

    for d in &options.column_clarifications {
        match table_columns.get(d.table.as_str()) {
            None => invalid.push(format!(
                "column_clarification: table '{}' not found",
                d.table
            )),
            Some(cols) if !cols.contains(d.column.as_str()) => {
                invalid.push(format!(
                    "column_clarification: column '{}.{}' not found",
                    d.table, d.column
                ));
            }
            _ => {}
        }
    }

    for r in &options.confirmed_relationships {
        match table_columns.get(r.from_table.as_str()) {
            None => invalid.push(format!(
                "confirmed_relationship: table '{}' not found",
                r.from_table
            )),
            Some(cols) if !cols.contains(r.from_column.as_str()) => {
                invalid.push(format!(
                    "confirmed_relationship: column '{}.{}' not found",
                    r.from_table, r.from_column
                ));
            }
            _ => {}
        }
        match table_columns.get(r.to_table.as_str()) {
            None => invalid.push(format!(
                "confirmed_relationship: table '{}' not found",
                r.to_table
            )),
            Some(cols) if !cols.contains(r.to_column.as_str()) => {
                invalid.push(format!(
                    "confirmed_relationship: column '{}.{}' not found",
                    r.to_table, r.to_column
                ));
            }
            _ => {}
        }
    }

    for t in &options.excluded_tables {
        if !table_columns.contains_key(t.as_str()) {
            invalid.push(format!("excluded_table: table '{t}' not found"));
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        Err(AppError::bad_request(format!(
            "Invalid decisions referencing nonexistent schema elements: {}",
            invalid.join("; ")
        )))
    }
}

/// Review gate: ensure all required decisions have been made.
pub(crate) fn maybe_require_review(
    report: &SourceAnalysisReport,
    options: &DesignOptions,
) -> Result<(), AppError> {
    let pii_pending: Vec<String> = report
        .pii_findings
        .iter()
        .filter(|f| {
            !options
                .pii_decisions
                .iter()
                .any(|e| e.table == f.table && e.column == f.column)
        })
        .map(|f| format!("{}.{}", f.table, f.column))
        .collect();

    let clarifications_pending: Vec<String> = report
        .ambiguous_columns
        .iter()
        .filter(|a| {
            !options
                .column_clarifications
                .iter()
                .any(|e| e.table == a.table && e.column == a.column)
        })
        .map(|a| format!("{}.{}", a.table, a.column))
        .collect();

    let partial_ack_needed = report.is_partial() && !options.allow_partial_source_analysis;

    if pii_pending.is_empty() && clarifications_pending.is_empty() && !partial_ack_needed {
        return Ok(());
    }

    Err(AppError::unprocessable_with_details(
        "review_required",
        "Resolve all review items before designing",
        serde_json::json!({
            "code": "review_required",
            "report": report,
            "pending_review": {
                "pii_decisions_required": pii_pending,
                "column_clarifications_required": clarifications_pending,
                "partial_analysis_acknowledgement_required": partial_ack_needed,
            },
        }),
    ))
}

/// Prune decisions invalidated by a new source snapshot.
///
/// Structural decisions (excluded_tables, confirmed_relationships) are pruned
/// when the referenced tables no longer exist in the new schema, or fully reset
/// when the source identity changed (different fingerprint = different system).
///
/// PII decisions are retained for columns that still exist in the new schema
/// (the user already reviewed those columns; their PII classification is stable).
/// Column clarifications are always reset because even if the same column name
/// exists, the underlying data semantics may have changed.
pub(crate) fn prune_decisions(
    mut opts: DesignOptions,
    schema: Option<&SourceSchema>,
    source_identity_changed: bool,
) -> (DesignOptions, Vec<String>) {
    let mut invalidated = Vec::new();

    // Build column lookup for PII decision retention
    let new_columns: HashSet<String> = schema
        .map(|s| {
            s.tables
                .iter()
                .flat_map(|t| {
                    t.columns
                        .iter()
                        .map(move |c| format!("{}.{}", t.name, c.name))
                })
                .collect()
        })
        .unwrap_or_default();

    // PII decisions: retain for columns that still exist (unless source identity changed)
    if source_identity_changed || new_columns.is_empty() {
        for d in &opts.pii_decisions {
            invalidated.push(format!("pii:{}.{}", d.table, d.column));
        }
        opts.pii_decisions.clear();
    } else {
        opts.pii_decisions.retain(|d| {
            let key = format!("{}.{}", d.table, d.column);
            let exists = new_columns.contains(&key);
            if !exists {
                invalidated.push(format!("pii:{key}"));
            }
            exists
        });
    }

    // Column clarifications: always reset (data semantics may have changed)
    for d in &opts.column_clarifications {
        invalidated.push(format!("clarification:{}.{}", d.table, d.column));
    }
    opts.column_clarifications.clear();

    if source_identity_changed {
        // Different source instance: invalidate ALL decisions
        for t in &opts.excluded_tables {
            invalidated.push(format!("exclusion:{t}"));
        }
        opts.excluded_tables.clear();

        for r in &opts.confirmed_relationships {
            invalidated.push(format!("relationship:{}.{}", r.from_table, r.to_table));
        }
        opts.confirmed_relationships.clear();
    } else if let Some(schema) = schema {
        // Same source instance: prune if referenced tables/columns no longer exist
        let table_columns: HashMap<&str, HashSet<&str>> = schema
            .tables
            .iter()
            .map(|t| {
                let cols: HashSet<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
                (t.name.as_str(), cols)
            })
            .collect();

        opts.excluded_tables.retain(|t| {
            let exists = table_columns.contains_key(t.as_str());
            if !exists {
                invalidated.push(format!("exclusion:{t}"));
            }
            exists
        });

        opts.confirmed_relationships.retain(|r| {
            let valid = table_columns
                .get(r.from_table.as_str())
                .is_some_and(|cols| cols.contains(r.from_column.as_str()))
                && table_columns
                    .get(r.to_table.as_str())
                    .is_some_and(|cols| cols.contains(r.to_column.as_str()));
            if !valid {
                invalidated.push(format!("relationship:{}.{}", r.from_table, r.to_table));
            }
            valid
        });
    }

    // Reset partial analysis acknowledgement since snapshot changed
    opts.allow_partial_source_analysis = false;

    (opts, invalidated)
}

/// Build a concise source schema summary for refinement when no graph runtime
/// is available. Provides table names, column listings, and FK relationships
/// so the LLM can refine property descriptions and suggest new relationships.
pub(crate) fn build_source_schema_summary(schema: &ox_core::source_schema::SourceSchema) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "Source schema context (no graph profile available):\n\
         Source type: {}\n\
         Tables: {}\n",
        schema.source_type,
        schema.tables.len()
    )
    .unwrap();

    for table in &schema.tables {
        let cols: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        writeln!(out, "  {} — columns: [{}]", table.name, cols.join(", ")).unwrap();
    }

    if !schema.foreign_keys.is_empty() {
        writeln!(out, "\nForeign keys:").unwrap();
        for fk in &schema.foreign_keys {
            writeln!(
                out,
                "  {}.{} -> {}.{}",
                fk.from_table, fk.from_column, fk.to_table, fk.to_column
            )
            .unwrap();
        }
    }

    out
}

/// Combine optional graph profile and additional context for the refinement LLM.
pub(crate) fn build_refinement_context(
    graph_profile: Option<&str>,
    additional_context: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(profile) = graph_profile {
        parts.push(format!("Graph data profile:\n{profile}"));
    }
    if let Some(ctx) = additional_context {
        let trimmed = ctx.trim();
        if !trimmed.is_empty() {
            parts.push(format!(
                "Additional context (resolves quality gaps):\n{trimmed}"
            ));
        }
    }
    parts.join("\n\n")
}
