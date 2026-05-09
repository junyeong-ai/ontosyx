use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_ontology::ambiguity::RepoHint;
use ox_ontology::mapping::refs::SourceId;
use ox_ontology::pii::RegexPiiClassifier;
use ox_ontology::repo_insights::RepoInsights;
use ox_ontology::source_analysis::{
    AnalysisCompleteness, DesignOptions, LARGE_SCHEMA_WARNING_THRESHOLD, LargeSchemaWarning,
    RepoAnalysisStatus, RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats,
    SourceAnalysisReport,
};

use super::ambiguous::detect_ambiguous;
use super::exclusions::suggest_exclusions;
use super::fk_inference::infer_implied_fks;
use super::pii_scan::scan_for_pii;

/// Build a [`SourceAnalysisReport`] from schema + profile (no LLM,
/// no I/O). Detects implied FKs, PII suggestions, ambiguous
/// columns, and table exclusion candidates. `source_id` +
/// `source_hash` flow through into every
/// [`ox_ontology::ambiguity::AmbiguityContext`] so a later schema
/// change invalidates stale resolutions deterministically (hash
/// mismatch ⇒ re-ask the operator).
pub fn build_analysis_report(
    source_id: &SourceId,
    source_hash: &str,
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
    let pii_suggestions = scan_for_pii(schema, profile, &RegexPiiClassifier::new());
    let ambiguous_columns = detect_ambiguous(source_id, source_hash, schema, profile);
    let table_exclusion_suggestions = suggest_exclusions(schema, profile);

    let large_schema_warning = if table_count >= LARGE_SCHEMA_WARNING_THRESHOLD {
        Some(LargeSchemaWarning {
            table_count,
            recommended_max: LARGE_SCHEMA_WARNING_THRESHOLD,
        })
    } else {
        None
    };

    SourceAnalysisReport {
        schema_stats,
        implied_relationships,
        pii_suggestions,
        ambiguous_columns,
        table_exclusion_suggestions,
        large_schema_warning,
        repo_suggestions: Vec::new(),
        repo_summary: None,
        analysis_completeness: AnalysisCompleteness::Complete,
        analysis_warnings: Vec::new(),
    }
}

/// Enrich the analysis report with insights from repo analysis:
/// ORM-confirmed FKs upgrade implied-FK confidence from 0.85 to
/// 0.98, and repo enum definitions hint at ambiguous columns.
pub fn enrich_with_repo(report: &mut SourceAnalysisReport, insights: &RepoInsights) {
    let mut upgraded_fk_count = 0;

    for rel in &mut report.implied_relationships {
        let confirmed = insights.orm_relationships.iter().any(|orm| {
            let fwd = orm.from_table.eq_ignore_ascii_case(&rel.from_table)
                && orm.to_table.eq_ignore_ascii_case(&rel.to_table);
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

    let mut suggestion_columns = 0usize;
    for ambiguous in &mut report.ambiguous_columns {
        let table = &ambiguous.column.relation;
        let column = &ambiguous.column.column;
        let matched = insights.enum_definitions.iter().find(|e| {
            e.table_name.eq_ignore_ascii_case(table) && e.field.eq_ignore_ascii_case(column)
        });
        if let Some(enum_def) = matched {
            let suggested_values = enum_def
                .values
                .iter()
                .map(|cl| format!("{}={}", cl.code, cl.label))
                .collect::<Vec<_>>()
                .join(", ");
            ambiguous.repo_hint = Some(RepoHint {
                suggested_values: suggested_values.clone(),
                source_file: enum_def.source_file.clone(),
            });
            report.repo_suggestions.push(RepoColumnSuggestion {
                table: table.clone(),
                column: column.clone(),
                suggested_values,
                source_file: enum_def.source_file.clone(),
            });
            suggestion_columns += 1;
        }
    }

    report.repo_summary = Some(RepoAnalysisSummary {
        status: RepoAnalysisStatus::Complete,
        failure_reason: None,
        framework: insights.framework.clone(),
        files_requested: insights.analyzed_files.len(),
        files_analyzed: insights.analyzed_files.len(),
        tree_truncated: false,
        enums_found: insights.enum_definitions.len(),
        relationships_found: insights.orm_relationships.len(),
        columns_with_suggestions: suggestion_columns,
        fk_confidence_upgraded: upgraded_fk_count,
        commit_sha: None,
        field_hints: insights.field_hints.clone(),
        domain_notes: insights.domain_notes.clone(),
    });
}

/// Build the LLM design context string. Drops every section the
/// operator hasn't filled in, so an empty `DesignOptions` produces
/// an empty string.
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

    if !options.excluded_columns.is_empty() {
        let listed = options
            .excluded_columns
            .iter()
            .map(|c| format!("{}.{}", c.table, c.column))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!(
            "Excluded columns (do NOT include these properties):\n  {listed}"
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

    if !options.pii_annotations.is_empty() {
        let listed = options
            .pii_annotations
            .iter()
            .map(|a| format!("  {}.{}: {:?}", a.table, a.column, a.kind))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!(
            "PII annotations (set pii_kind on the resulting property):\n{listed}"
        ));
    }

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
    use ox_ontology::pii::{ExcludedColumn, PiiAnnotation};

    #[test]
    fn enrich_with_repo_priority1_explicit_table_name() {
        use ox_ontology::repo_insights::{CodeLabel, RepoEnumDef};

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
                    pii_redacted: None,
                }],
            }],
        };
        let mut report =
            build_analysis_report(&SourceId::new("src-test"), "sha256:test", &schema, &profile);
        assert_eq!(report.ambiguous_columns.len(), 1);

        let insights = RepoInsights {
            framework: Some("Django".to_string()),
            enum_definitions: vec![RepoEnumDef {
                model: "Store".to_string(),
                field: "store_type".to_string(),
                table_name: "tb_stores".to_string(),
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

        assert_eq!(report.ambiguous_columns.len(), 1);
        assert!(report.ambiguous_columns[0].repo_hint.is_some());
        assert!(
            report.ambiguous_columns[0]
                .repo_hint
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
        use ox_ontology::repo_insights::{CodeLabel, RepoEnumDef};

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
                    pii_redacted: None,
                }],
            }],
        };
        let mut report =
            build_analysis_report(&SourceId::new("src-test"), "sha256:test", &schema, &profile);
        assert_eq!(report.ambiguous_columns.len(), 1);

        let insights = RepoInsights {
            framework: Some("Rails".to_string()),
            enum_definitions: vec![RepoEnumDef {
                model: "Order".to_string(),
                field: "status".to_string(),
                table_name: "orders".to_string(),
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

        assert_eq!(report.ambiguous_columns.len(), 1);
        assert!(report.ambiguous_columns[0].repo_hint.is_some());
        assert!(
            report.ambiguous_columns[0]
                .repo_hint
                .as_ref()
                .unwrap()
                .suggested_values
                .contains("1=pending")
        );
        assert_eq!(report.repo_suggestions.len(), 1);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.columns_with_suggestions, 1);
    }

    #[test]
    fn enrich_with_repo_fk_confidence_upgrade() {
        use ox_ontology::repo_insights::{OrmRelationType, OrmRelationship, RepoInsights};

        let schema = make_schema(
            &[("orders", &["id", "customer_id"]), ("customers", &["id"])],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let mut report =
            build_analysis_report(&SourceId::new("src-test"), "sha256:test", &schema, &profile);

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
        use ox_ontology::repo_insights::{OrmRelationType, OrmRelationship};
        let schema = make_schema(
            &[("orders", &["id", "customer_id"]), ("customers", &["id"])],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let mut report =
            build_analysis_report(&SourceId::new("src-test"), "sha256:test", &schema, &profile);
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

        assert!(report.implied_relationships[0].repo_confirmed);
        assert_eq!(report.implied_relationships[0].confidence, 0.98);
        let summary = report.repo_summary.as_ref().unwrap();
        assert_eq!(summary.fk_confidence_upgraded, 1);
    }

    #[test]
    fn build_design_context_empty_options() {
        let ctx = build_design_context("", &DesignOptions::default(), None);
        assert!(ctx.is_empty());
    }

    #[test]
    fn build_design_context_all_sections() {
        use ox_ontology::ir::PiiKind;
        use ox_ontology::source_analysis::{ColumnClarification, ConfirmedRelationship};
        let options = DesignOptions {
            confirmed_relationships: vec![ConfirmedRelationship {
                from_table: "orders".to_string(),
                from_column: "customer_id".to_string(),
                to_table: "customers".to_string(),
                to_column: "id".to_string(),
            }],
            excluded_tables: vec!["audit_log".to_string()],
            excluded_columns: vec![ExcludedColumn {
                table: "users".to_string(),
                column: "ssn".to_string(),
            }],
            column_clarifications: vec![ColumnClarification {
                table: "orders".to_string(),
                column: "status".to_string(),
                hint: "1=active, 2=cancelled".to_string(),
            }],
            pii_annotations: vec![PiiAnnotation {
                table: "users".to_string(),
                column: "email".to_string(),
                kind: PiiKind::Email,
            }],
            partial_analysis_acknowledged: false,
            large_schema_acknowledged: false,
        };
        let ctx = build_design_context("base hint", &options, None);
        assert!(ctx.contains("base hint"));
        assert!(ctx.contains("orders.customer_id → customers.id"));
        assert!(ctx.contains("audit_log"));
        assert!(ctx.contains("users.ssn"));
        assert!(ctx.contains("orders.status"));
        assert!(ctx.contains("users.email"));
    }
}
