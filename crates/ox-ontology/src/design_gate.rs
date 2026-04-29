//! Design-action gating model.
//!
//! Whether an operator may proceed from analysis review to LLM-driven
//! ontology design depends on a small, stable set of conditions —
//! ambiguous columns clarified, partial-analysis acknowledged, large
//! schema acknowledged. Historically each handler re-implemented its
//! own checks ad-hoc and the FE re-implemented them again to render
//! a disabled-button state. The two implementations drifted, the FE
//! had to surface "why is this disabled" by reverse-engineering the
//! backend response, and adding a new gate touched four places.
//!
//! [`evaluate_design_gates`] is the single source of truth. It walks
//! the same inputs the design endpoints already have
//! ([`SchemaSummary`], [`SourceAnalysisReport`], [`DesignOptions`])
//! and emits one gate per condition. The HTTP layer:
//!
//! - rejects the design call when any `blocks_design` gate is
//!   [`GateStatus::Unmet`] (replaces the old `maybe_require_review`),
//! - serialises the same vector onto the project response so the FE
//!   renders a checklist and click-to-anchor without re-deriving
//!   anything.
//!
//! Adding a new gate is one [`GateId`] variant + one match arm in
//! [`evaluate_design_gates`]. Both backend enforcement and FE
//! rendering pick it up automatically.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::source_analysis::{
    DesignOptions, LARGE_SCHEMA_GATE_THRESHOLD, SourceAnalysisReport,
};

/// Stable identifier for a single gate the operator must satisfy
/// before invoking the design action. New variants are additive —
/// the FE i18n catalogue keys gate copy by `GateId`.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GateId {
    /// Every column the analyzer flagged as ambiguous has either an
    /// operator-supplied clarification or a repo-derived hint.
    ColumnClarificationsResolved,
    /// The operator explicitly acknowledged proceeding with an
    /// incomplete source analysis (warnings present).
    PartialAnalysisAcknowledged,
    /// The operator explicitly acknowledged designing against a
    /// schema that exceeds [`LARGE_SCHEMA_GATE_THRESHOLD`].
    LargeSchemaAcknowledged,
}

impl GateId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ColumnClarificationsResolved => "column_clarifications_resolved",
            Self::PartialAnalysisAcknowledged => "partial_analysis_acknowledged",
            Self::LargeSchemaAcknowledged => "large_schema_acknowledged",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Met,
    Unmet,
}

/// One condition the operator must satisfy before designing.
///
/// `blocks_design` is the contract the HTTP layer enforces: any
/// `blocks_design = true` gate with `status = Unmet` rejects the
/// design call. `params` carries interpolation values for the FE
/// i18n catalogue (e.g. `pending_count`, `table_count`); the
/// backend never produces user-facing prose itself. `anchor` is the
/// FE element id to scroll to when the operator clicks the gate
/// row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct DesignGate {
    pub id: GateId,
    pub status: GateStatus,
    pub blocks_design: bool,
    /// FE element id for click-to-anchor scroll. `None` when the
    /// gate has no inline control to focus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl DesignGate {
    fn new(id: GateId, status: GateStatus, blocks_design: bool, anchor: &'static str) -> Self {
        Self {
            id,
            status,
            blocks_design,
            anchor: Some(anchor.to_string()),
            params: BTreeMap::new(),
        }
    }

    fn with_param(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.params.insert(key.to_string(), value.into());
        self
    }
}

/// Evaluate every design gate against the current project state and
/// return only the gates that apply. A gate that has no precondition
/// (e.g. partial-analysis acknowledgement when the report is
/// complete, large-schema acknowledgement when the schema is
/// small) is omitted entirely — the FE renders exactly the
/// applicable checklist without filtering.
pub fn evaluate_design_gates(
    report: &SourceAnalysisReport,
    options: &DesignOptions,
) -> Vec<DesignGate> {
    let mut gates = Vec::new();

    // ── ColumnClarificationsResolved ──────────────────────────────
    if !report.ambiguous_columns.is_empty() {
        let pending = report
            .ambiguous_columns
            .iter()
            .filter(|a| {
                !options.column_clarifications.iter().any(|e| {
                    e.table == a.column.relation && e.column == a.column.column
                })
            })
            .count();
        let status = if pending == 0 {
            GateStatus::Met
        } else {
            GateStatus::Unmet
        };
        gates.push(
            DesignGate::new(
                GateId::ColumnClarificationsResolved,
                status,
                true,
                "review-clarifications",
            )
            .with_param("pending_count", pending.to_string())
            .with_param("total_count", report.ambiguous_columns.len().to_string()),
        );
    }

    // ── PartialAnalysisAcknowledged ───────────────────────────────
    if report.is_partial() {
        let status = if options.partial_analysis_acknowledged {
            GateStatus::Met
        } else {
            GateStatus::Unmet
        };
        gates.push(
            DesignGate::new(
                GateId::PartialAnalysisAcknowledged,
                status,
                true,
                "review-partial-acknowledgement",
            )
            .with_param(
                "warning_count",
                report.analysis_warnings.len().to_string(),
            ),
        );
    }

    // ── LargeSchemaAcknowledged ───────────────────────────────────
    if let Some(warning) = &report.large_schema_warning
        && warning.table_count >= LARGE_SCHEMA_GATE_THRESHOLD
    {
        let status = if options.large_schema_acknowledged {
            GateStatus::Met
        } else {
            GateStatus::Unmet
        };
        gates.push(
            DesignGate::new(
                GateId::LargeSchemaAcknowledged,
                status,
                true,
                "review-large-schema-acknowledgement",
            )
            .with_param("table_count", warning.table_count.to_string())
            .with_param("threshold", LARGE_SCHEMA_GATE_THRESHOLD.to_string()),
        );
    }

    gates
}

/// True when every `blocks_design = true` gate is [`GateStatus::Met`].
pub fn design_allowed(gates: &[DesignGate]) -> bool {
    gates
        .iter()
        .all(|g| !g.blocks_design || g.status == GateStatus::Met)
}

/// IDs of every blocking gate that is currently unmet. Empty when
/// the design action may proceed.
pub fn unmet_blocking_gates(gates: &[DesignGate]) -> Vec<GateId> {
    gates
        .iter()
        .filter(|g| g.blocks_design && g.status == GateStatus::Unmet)
        .map(|g| g.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambiguity::{AmbiguityContext, AmbiguityId, AmbiguityKind};
    use crate::mapping::{ColumnRef, SourceId};
    use crate::source_analysis::{
        AnalysisCompleteness, ColumnClarification, LargeSchemaWarning, SchemaStats,
        SourceAnalysisReport,
    };

    fn empty_report() -> SourceAnalysisReport {
        SourceAnalysisReport {
            schema_stats: SchemaStats {
                table_count: 0,
                column_count: 0,
                declared_fk_count: 0,
                total_row_count: 0,
            },
            implied_relationships: vec![],
            pii_suggestions: vec![],
            ambiguous_columns: vec![],
            table_exclusion_suggestions: vec![],
            large_schema_warning: None,
            repo_suggestions: vec![],
            repo_summary: None,
            analysis_completeness: AnalysisCompleteness::Complete,
            analysis_warnings: vec![],
        }
    }

    fn ambiguous(table: &str, column: &str) -> AmbiguityContext {
        AmbiguityContext {
            id: AmbiguityId::new("amb"),
            source_id: SourceId::new("src:test".to_string()),
            column: ColumnRef::new(table, column),
            kind: AmbiguityKind::NumericCode,
            sample_values: vec![],
            distinct_estimate: None,
            nullable: false,
            clarification_prompt: "what does this code mean?".to_string(),
            detection_source_hash: "hash".to_string(),
            repo_hint: None,
            detected_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn empty_report_emits_no_gates() {
        let gates = evaluate_design_gates(&empty_report(), &Default::default());
        assert!(gates.is_empty());
        assert!(design_allowed(&gates));
    }

    #[test]
    fn unresolved_clarifications_block_design() {
        let mut report = empty_report();
        report.ambiguous_columns = vec![ambiguous("orders", "status")];
        let gates = evaluate_design_gates(&report, &Default::default());
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].id, GateId::ColumnClarificationsResolved);
        assert_eq!(gates[0].status, GateStatus::Unmet);
        assert!(!design_allowed(&gates));
    }

    #[test]
    fn matched_clarifications_unblock_design() {
        let mut report = empty_report();
        report.ambiguous_columns = vec![ambiguous("orders", "status")];
        let mut options = DesignOptions::default();
        options.column_clarifications.push(ColumnClarification {
            table: "orders".to_string(),
            column: "status".to_string(),
            hint: "1=active, 2=cancelled".to_string(),
        });
        let gates = evaluate_design_gates(&report, &options);
        assert_eq!(gates[0].status, GateStatus::Met);
        assert!(design_allowed(&gates));
    }

    #[test]
    fn partial_analysis_requires_acknowledgement() {
        let mut report = empty_report();
        report.analysis_completeness = AnalysisCompleteness::Partial;
        let gates = evaluate_design_gates(&report, &Default::default());
        let gate = gates
            .iter()
            .find(|g| g.id == GateId::PartialAnalysisAcknowledged)
            .expect("partial gate must surface for partial reports");
        assert_eq!(gate.status, GateStatus::Unmet);
        assert!(gate.blocks_design);
    }

    #[test]
    fn partial_analysis_acknowledged_meets_gate() {
        let mut report = empty_report();
        report.analysis_completeness = AnalysisCompleteness::Partial;
        let options = DesignOptions {
            partial_analysis_acknowledged: true,
            ..DesignOptions::default()
        };
        let gates = evaluate_design_gates(&report, &options);
        assert_eq!(
            gates
                .iter()
                .find(|g| g.id == GateId::PartialAnalysisAcknowledged)
                .map(|g| g.status),
            Some(GateStatus::Met)
        );
    }

    #[test]
    fn large_schema_gate_only_fires_above_threshold() {
        let mut report = empty_report();
        // 50 — below LARGE_SCHEMA_GATE_THRESHOLD (100): no gate.
        report.large_schema_warning = Some(LargeSchemaWarning {
            table_count: 50,
            recommended_max: LARGE_SCHEMA_GATE_THRESHOLD,
        });
        let gates = evaluate_design_gates(&report, &Default::default());
        assert!(
            gates
                .iter()
                .find(|g| g.id == GateId::LargeSchemaAcknowledged)
                .is_none(),
            "below threshold ⇒ no gate"
        );

        report.large_schema_warning = Some(LargeSchemaWarning {
            table_count: LARGE_SCHEMA_GATE_THRESHOLD + 5,
            recommended_max: LARGE_SCHEMA_GATE_THRESHOLD,
        });
        let gates = evaluate_design_gates(&report, &Default::default());
        let gate = gates
            .iter()
            .find(|g| g.id == GateId::LargeSchemaAcknowledged)
            .expect("at-or-above threshold ⇒ gate");
        assert_eq!(gate.status, GateStatus::Unmet);
        assert!(gate.blocks_design);
        assert_eq!(
            gate.params.get("table_count").map(String::as_str),
            Some("105")
        );
    }

    #[test]
    fn unmet_blocking_gates_lists_only_blockers() {
        let mut report = empty_report();
        report.ambiguous_columns = vec![ambiguous("orders", "status")];
        report.analysis_completeness = AnalysisCompleteness::Partial;
        let gates = evaluate_design_gates(&report, &Default::default());
        let unmet = unmet_blocking_gates(&gates);
        assert_eq!(unmet.len(), 2);
        assert!(unmet.contains(&GateId::ColumnClarificationsResolved));
        assert!(unmet.contains(&GateId::PartialAnalysisAcknowledged));
    }
}
