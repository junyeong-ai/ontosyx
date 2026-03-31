use std::collections::HashMap;

use ox_core::repo_insights::RepoInsights;
use ox_core::source_analysis::{
    AnalysisCompleteness, DesignOptions, LARGE_SCHEMA_WARNING_THRESHOLD, LargeSchemaWarning,
    PiiDecision, PiiDecisionEntry, RepoAnalysisStatus, RepoAnalysisSummary, RepoColumnSuggestion,
    RepoSuggestion, SchemaStats, SourceAnalysisReport,
};
use ox_core::source_schema::{SourceProfile, SourceSchema};

use super::ambiguous::detect_ambiguous;
use super::exclusions::suggest_exclusions;
use super::fk_inference::infer_implied_fks;
use super::pii::detect_pii;

/// Build a SourceAnalysisReport from schema + profile (no LLM, no I/O).
/// Detects implied FKs, PII columns, ambiguous columns, and table exclusion candidates.
pub fn build_analysis_report(
    schema: &SourceSchema,
    profile: &SourceProfile,
) -> SourceAnalysisReport {
    let table_count = schema.tables.len();
    let column_count = schema.tables.iter().map(|t| t.columns.len()).sum();
    let declared_fk_count = schema.foreign_keys.iter().filter(|fk| !fk.inferred).count();
    let total_row_count = profile.table_profiles.iter().map(|tp| tp.row_count).sum();

    let schema_stats = SchemaStats {
        table_count,
        column_count,
        declared_fk_count,
        total_row_count,
    };

    let implied_relationships = infer_implied_fks(schema);
    let pii_findings = detect_pii(schema, profile);
    let ambiguous_columns = detect_ambiguous(schema, profile);
    let table_exclusion_suggestions = suggest_exclusions(schema, profile);

    let large_schema_warning = if table_count >= LARGE_SCHEMA_WARNING_THRESHOLD {
        Some(LargeSchemaWarning {
            table_count,
            recommended_max: LARGE_SCHEMA_WARNING_THRESHOLD,
            suggestion: format!(
                "Schema has {table_count} tables (recommended max: {LARGE_SCHEMA_WARNING_THRESHOLD}). \
                 Consider using excluded_tables in DesignOptions to scope the ontology, \
                 or split into domain-specific ontologies."
            ),
        })
    } else {
        None
    };

    SourceAnalysisReport {
        schema_stats,
        implied_relationships,
        pii_findings,
        ambiguous_columns,
        table_exclusion_suggestions,
        large_schema_warning,
        repo_suggestions: Vec::new(),
        repo_summary: None,
        analysis_completeness: AnalysisCompleteness::Complete,
        analysis_warnings: Vec::new(),
    }
}

/// Enrich the analysis report with insights from repo analysis.
/// - ORM-confirmed FKs upgrade confidence from 0.85 -> 0.98
/// - Repo enum defs that match ambiguous columns are recorded as repo_suggestions
pub fn enrich_with_repo(report: &mut SourceAnalysisReport, insights: &RepoInsights) {
    let mut upgraded_fk_count = 0;

    // Upgrade implied FK confidence when ORM confirms the relationship
    for rel in &mut report.implied_relationships {
        let confirmed = insights.orm_relationships.iter().any(|orm| {
            // Deterministic match: LLM provides explicit table names — no heuristic conversion.
            let fwd = orm.from_table.eq_ignore_ascii_case(&rel.from_table)
                && orm.to_table.eq_ignore_ascii_case(&rel.to_table);
            // Reverse: ORM inverse association (HasMany: Customer -> Order
            // is the inverse of orders.customer_id -> customers).
            let rev = orm.to_table.eq_ignore_ascii_case(&rel.from_table)
                && orm.from_table.eq_ignore_ascii_case(&rel.to_table);
            fwd || rev
        });

        if confirmed && !rel.repo_confirmed {
            rel.repo_confirmed = true;
            rel.confidence = 0.98;
            upgraded_fk_count += 1;
        }
    }

    // Annotate ambiguous columns with repo suggestions.
    // Columns stay in ambiguous_columns (user must explicitly accept).
    // Also recorded in repo_suggestions for provenance tracking.
    let mut suggestions: Vec<RepoColumnSuggestion> = Vec::new();

    for ambig in &mut report.ambiguous_columns {
        let found = insights.enum_definitions.iter().find(|def| {
            // Deterministic match: LLM provides explicit table_name — no heuristic conversion.
            def.table_name.eq_ignore_ascii_case(&ambig.table)
                && def.field.eq_ignore_ascii_case(&ambig.column)
        });

        if let Some(def) = found {
            let suggested_values = def
                .values
                .iter()
                .map(|cv| format!("{}={}", cv.code, cv.label))
                .collect::<Vec<_>>()
                .join(", ");

            ambig.repo_suggestion = Some(RepoSuggestion {
                suggested_values: suggested_values.clone(),
                source_file: def.source_file.clone(),
            });

            suggestions.push(RepoColumnSuggestion {
                table: ambig.table.clone(),
                column: ambig.column.clone(),
                suggested_values,
                source_file: def.source_file.clone(),
            });
        }
    }

    report.repo_suggestions = suggestions;

    // Attach repo summary
    report.repo_summary = Some(RepoAnalysisSummary {
        status: RepoAnalysisStatus::Complete, // caller may downgrade to Partial
        status_reason: None,
        framework: insights.framework.clone(),
        files_requested: 0, // caller sets this after navigate_repo
        files_analyzed: insights.analyzed_files.len(),
        tree_truncated: false, // caller sets this after generate_tree
        enums_found: insights.enum_definitions.len(),
        relationships_found: insights.orm_relationships.len(),
        columns_with_suggestions: report.repo_suggestions.len(),
        fk_confidence_upgraded: upgraded_fk_count,
        commit_sha: None,
        field_hints: insights.field_hints.clone(),
        domain_notes: insights.domain_notes.clone(),
    });
}

/// Apply PII decisions to a profile clone before it is sent to the LLM.
///
/// Decision semantics:
/// - `Mask`: Replace each sample value with `"[MASKED]"`. Column remains in the ontology;
///   the LLM understands data exists but cannot see real values.
/// - `Exclude`: Clear sample values entirely (`Vec::new()`). The LLM context also instructs
///   the model to omit this column from the ontology (`build_design_context`).
/// - `Allow`:   No-op — values pass through unchanged.
///
/// Both Mask and Exclude prevent raw PII from reaching the LLM prompt.
pub fn apply_pii_masking(profile: &mut SourceProfile, decisions: &[PiiDecisionEntry]) {
    // Pre-build O(1) lookup keyed by (table, column) to avoid O(n*m) inner scan.
    // If the caller supplies duplicate (table, column) entries, the HashMap collect()
    // keeps the last entry (last-write-wins). This is intentional: the caller controls
    // decision precedence by ordering entries; the final entry for a column wins.
    let actionable: HashMap<(&str, &str), &PiiDecisionEntry> = decisions
        .iter()
        .filter(|d| matches!(d.decision, PiiDecision::Mask | PiiDecision::Exclude))
        .map(|d| ((d.table.as_str(), d.column.as_str()), d))
        .collect();

    if actionable.is_empty() {
        return;
    }

    for tp in &mut profile.table_profiles {
        for cs in &mut tp.column_stats {
            let Some(entry) = actionable.get(&(tp.table_name.as_str(), cs.column_name.as_str()))
            else {
                continue;
            };

            match entry.decision {
                PiiDecision::Mask => {
                    cs.sample_values = cs
                        .sample_values
                        .iter()
                        .map(|_| "[MASKED]".to_string())
                        .collect();
                }
                PiiDecision::Exclude => {
                    cs.sample_values.clear();
                }
                PiiDecision::Allow => {} // filtered above — exhaustive match
            }
        }
    }
}

/// Build the LLM design context string incorporating DesignOptions and optional repo insights.
/// This is the text passed to `Brain::design_ontology` as `context`.
pub fn build_design_context(
    base_context: &str,
    options: &DesignOptions,
    repo_summary: Option<&RepoAnalysisSummary>,
) -> String {
    let mut parts = Vec::new();

    if !base_context.trim().is_empty() {
        parts.push(base_context.trim().to_string());
    }

    if !options.confirmed_relationships.is_empty() {
        let rels = options
            .confirmed_relationships
            .iter()
            .map(|r| {
                format!(
                    "  {}.{} → {}.{}",
                    r.from_table, r.from_column, r.to_table, r.to_column
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!(
            "Confirmed relationships (create edges for these):\n{rels}"
        ));
    }

    if !options.excluded_tables.is_empty() {
        parts.push(format!(
            "Excluded tables (do NOT create nodes for these):\n  {}",
            options.excluded_tables.join(", ")
        ));
    }

    if !options.column_clarifications.is_empty() {
        let clarifications = options
            .column_clarifications
            .iter()
            .map(|c| format!("  {}.{}: {}", c.table, c.column, c.hint))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!(
            "Column clarifications (incorporate into property descriptions):\n{clarifications}"
        ));
    }

    let excluded_pii: Vec<String> = options
        .pii_decisions
        .iter()
        .filter(|d| matches!(d.decision, PiiDecision::Exclude))
        .map(|d| format!("{}.{}", d.table, d.column))
        .collect();

    if !excluded_pii.is_empty() {
        parts.push(format!(
            "Excluded PII columns (do NOT include these properties):\n  {}",
            excluded_pii.join(", ")
        ));
    }

    // Include repo field hints and domain notes when available
    if let Some(summary) = repo_summary {
        if !summary.field_hints.is_empty() {
            let hints = summary
                .field_hints
                .iter()
                .map(|h| {
                    format!(
                        "- {}.{}: {} (source: {})",
                        h.model, h.field, h.hint, h.source
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("## Repository Field Hints\n{hints}"));
        }

        if !summary.domain_notes.is_empty() {
            let notes = summary
                .domain_notes
                .iter()
                .map(|n| format!("- {n}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("## Domain Context from Repository\n{notes}"));
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::test_utils::make_schema;
    use ox_core::source_schema::{ColumnStats, TableProfile};

    fn make_profile(table: &str, column: &str, values: &[&str]) -> SourceProfile {
        SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: table.to_string(),
                row_count: values.len() as u64,
                column_stats: vec![ColumnStats {
                    column_name: column.to_string(),
                    null_count: 0,
                    distinct_count: values.len() as u64,
                    sample_values: values.iter().map(|v| v.to_string()).collect(),
                    min_value: None,
                    max_value: None,
                }],
            }],
        }
    }

    #[test]
    fn pii_masking_mask_replaces_with_placeholder() {
        let mut profile = make_profile("users", "email", &["hong@example.com", "kim@example.com"]);
        let decisions = vec![PiiDecisionEntry {
            table: "users".to_string(),
            column: "email".to_string(),
            decision: PiiDecision::Mask,
        }];
        apply_pii_masking(&mut profile, &decisions);
        let values = &profile.table_profiles[0].column_stats[0].sample_values;
        assert_eq!(values.len(), 2, "Mask preserves cardinality");
        assert!(
            values.iter().all(|v| v == "[MASKED]"),
            "all values must be [MASKED]"
        );
    }

    #[test]
    fn pii_masking_exclude_clears_all_values() {
        let mut profile = make_profile("users", "ssn", &["123456-7890123", "234567-8901234"]);
        let decisions = vec![PiiDecisionEntry {
            table: "users".to_string(),
            column: "ssn".to_string(),
            decision: PiiDecision::Exclude,
        }];
        apply_pii_masking(&mut profile, &decisions);
        let values = &profile.table_profiles[0].column_stats[0].sample_values;
        assert!(
            values.is_empty(),
            "Exclude must produce empty sample_values"
        );
    }

    #[test]
    fn pii_masking_allow_is_noop() {
        let mut profile = make_profile("products", "name", &["Widget A", "Widget B"]);
        let decisions = vec![PiiDecisionEntry {
            table: "products".to_string(),
            column: "name".to_string(),
            decision: PiiDecision::Allow,
        }];
        apply_pii_masking(&mut profile, &decisions);
        let values = &profile.table_profiles[0].column_stats[0].sample_values;
        assert_eq!(
            values,
            &["Widget A", "Widget B"],
            "Allow must not alter values"
        );
    }

    #[test]
    fn enrich_with_repo_priority1_explicit_table_name() {
        use ox_core::repo_insights::{CodeLabel, RepoEnumDef};

        let schema = make_schema(&[("tb_stores", &["id", "store_type"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "tb_stores".to_string(),
                row_count: 3,
                column_stats: vec![ColumnStats {
                    column_name: "store_type".to_string(),
                    null_count: 0,
                    distinct_count: 2,
                    sample_values: vec!["N".to_string(), "Regular".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let mut report = build_analysis_report(&schema, &profile);
        // Verify ambiguous column was detected
        assert_eq!(report.ambiguous_columns.len(), 1);

        // LLM provides explicit table_name (non-conventional: "tb_stores")
        let insights = RepoInsights {
            framework: Some("Django".to_string()),
            enum_definitions: vec![RepoEnumDef {
                model: "Store".to_string(),
                field: "store_type".to_string(),
                table_name: "tb_stores".to_string(), // explicit, non-conventional
                values: vec![
                    CodeLabel {
                        code: "N".to_string(),
                        label: "야간매장".to_string(),
                    },
                    CodeLabel {
                        code: "Regular".to_string(),
                        label: "일반매장".to_string(),
                    },
                ],
                confidence: 0.95,
                source_file: "models.py".to_string(),
            }],
            orm_relationships: vec![],
            field_hints: vec![],
            domain_notes: vec![],
            analyzed_files: vec!["models.py".to_string()],
        };

        enrich_with_repo(&mut report, &insights);

        // Column stays in ambiguous_columns with repo_suggestion (user must accept)
        assert_eq!(report.ambiguous_columns.len(), 1);
        assert!(report.ambiguous_columns[0].repo_suggestion.is_some());
        assert!(
            report.ambiguous_columns[0]
                .repo_suggestion
                .as_ref()
                .unwrap()
                .suggested_values
                .contains("N=야간매장")
        );
        assert_eq!(report.repo_suggestions.len(), 1);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.columns_with_suggestions, 1);
    }

    #[test]
    fn enrich_with_repo_heuristic_matching() {
        use ox_core::repo_insights::{CodeLabel, RepoEnumDef};

        // table_name = None -> falls back to heuristic (Rails: Order model -> orders table)
        let schema = make_schema(&[("orders", &["id", "status"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "orders".to_string(),
                row_count: 3,
                column_stats: vec![ColumnStats {
                    column_name: "status".to_string(),
                    null_count: 0,
                    distinct_count: 3,
                    sample_values: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let mut report = build_analysis_report(&schema, &profile);
        assert_eq!(report.ambiguous_columns.len(), 1);

        let insights = RepoInsights {
            framework: Some("Rails".to_string()),
            enum_definitions: vec![RepoEnumDef {
                model: "Order".to_string(),
                field: "status".to_string(),
                table_name: "orders".to_string(), // LLM applies framework convention
                values: vec![
                    CodeLabel {
                        code: "1".to_string(),
                        label: "pending".to_string(),
                    },
                    CodeLabel {
                        code: "2".to_string(),
                        label: "confirmed".to_string(),
                    },
                    CodeLabel {
                        code: "3".to_string(),
                        label: "shipped".to_string(),
                    },
                ],
                confidence: 0.95,
                source_file: "order.rb".to_string(),
            }],
            orm_relationships: vec![],
            field_hints: vec![],
            domain_notes: vec![],
            analyzed_files: vec!["order.rb".to_string()],
        };

        enrich_with_repo(&mut report, &insights);

        // Column stays in ambiguous_columns with repo_suggestion (user must accept)
        assert_eq!(report.ambiguous_columns.len(), 1);
        assert!(report.ambiguous_columns[0].repo_suggestion.is_some());
        assert!(
            report.ambiguous_columns[0]
                .repo_suggestion
                .as_ref()
                .unwrap()
                .suggested_values
                .contains("1=pending")
        );
        // Also tracked in repo_suggestions for provenance
        assert_eq!(report.repo_suggestions.len(), 1);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.columns_with_suggestions, 1);
    }

    #[test]
    fn enrich_with_repo_fk_confidence_upgrade() {
        use ox_core::repo_insights::{OrmRelationType, OrmRelationship, RepoInsights};

        let schema = make_schema(
            &[("orders", &["id", "customer_id"]), ("customers", &["id"])],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let mut report = build_analysis_report(&schema, &profile);

        assert_eq!(report.implied_relationships.len(), 1);
        assert_eq!(report.implied_relationships[0].confidence, 0.85);

        let insights = RepoInsights {
            framework: Some("Rails".to_string()),
            enum_definitions: vec![],
            orm_relationships: vec![OrmRelationship {
                from_model: "Order".to_string(),
                to_model: "Customer".to_string(),
                from_table: "orders".to_string(),
                to_table: "customers".to_string(),
                relation_type: OrmRelationType::BelongsTo,
                through: None,
                confidence: 1.0,
                source_file: "order.rb".to_string(),
            }],
            field_hints: vec![],
            domain_notes: vec![],
            analyzed_files: vec!["order.rb".to_string()],
        };

        enrich_with_repo(&mut report, &insights);

        assert!(report.implied_relationships[0].repo_confirmed);
        assert_eq!(report.implied_relationships[0].confidence, 0.98);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.fk_confidence_upgraded, 1);
    }

    #[test]
    fn enrich_with_repo_reverse_orm_direction() {
        use ox_core::repo_insights::{OrmRelationType, OrmRelationship};
        // HasMany on Customer side is the inverse of orders.customer_id -> customers.
        // Reverse matching must upgrade the FK confidence regardless of direction.
        let schema = make_schema(
            &[("orders", &["id", "customer_id"]), ("customers", &["id"])],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let mut report = build_analysis_report(&schema, &profile);
        assert_eq!(report.implied_relationships[0].confidence, 0.85);

        let insights = RepoInsights {
            framework: Some("Rails".to_string()),
            enum_definitions: vec![],
            orm_relationships: vec![OrmRelationship {
                from_model: "Customer".to_string(),
                to_model: "Order".to_string(),
                from_table: "customers".to_string(),
                to_table: "orders".to_string(),
                relation_type: OrmRelationType::HasMany,
                through: None,
                confidence: 1.0,
                source_file: "customer.rb".to_string(),
            }],
            field_hints: vec![],
            domain_notes: vec![],
            analyzed_files: vec!["customer.rb".to_string()],
        };

        enrich_with_repo(&mut report, &insights);

        assert!(
            report.implied_relationships[0].repo_confirmed,
            "reverse HasMany must upgrade implied FK confidence"
        );
        assert_eq!(report.implied_relationships[0].confidence, 0.98);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.fk_confidence_upgraded, 1);
    }

    #[test]
    fn build_design_context_empty_options() {
        let ctx = build_design_context("", &DesignOptions::default(), None);
        assert!(
            ctx.is_empty(),
            "no context + empty options must produce empty string"
        );
    }

    #[test]
    fn build_design_context_all_sections() {
        use ox_core::source_analysis::{ColumnClarification, ConfirmedRelationship};
        let options = DesignOptions {
            confirmed_relationships: vec![ConfirmedRelationship {
                from_table: "orders".to_string(),
                from_column: "customer_id".to_string(),
                to_table: "customers".to_string(),
                to_column: "id".to_string(),
            }],
            excluded_tables: vec!["audit_log".to_string()],
            column_clarifications: vec![ColumnClarification {
                table: "orders".to_string(),
                column: "status".to_string(),
                hint: "1=active, 2=cancelled".to_string(),
            }],
            pii_decisions: vec![PiiDecisionEntry {
                table: "users".to_string(),
                column: "email".to_string(),
                decision: PiiDecision::Exclude,
            }],
            allow_partial_source_analysis: false,
        };
        let ctx = build_design_context("base hint", &options, None);
        assert!(ctx.contains("base hint"));
        assert!(ctx.contains("orders.customer_id → customers.id"));
        assert!(ctx.contains("audit_log"));
        assert!(ctx.contains("orders.status"));
        assert!(ctx.contains("users.email"));
    }
}
