//! Evaluation entities — `EvaluationRun`, `EvaluationCase`,
//! `EvaluationMetric`. The platform's first-class metric loop for
//! LLM-driven flows (NL→Cypher translation, GraphRAG retrieval,
//! agent tool use).
//!
//! ## Mental model
//!
//! - A **Run** is one batch of evaluation. Captures the wider
//!   context (which model, which dataset, which ontology version)
//!   plus a status / lifecycle (`Running` → `Succeeded` / `Failed`
//!   / `Cancelled`). Tenant-scoped via `workspace_id`.
//! - A **Case** is a single (input, expected, actual) tuple inside
//!   a run. Stable per-run via `case_key` so re-running a dataset
//!   replaces previous rows on the natural key. Latency and the
//!   error path are first-class fields rather than rolled into the
//!   metric set — operators triaging a regression want them
//!   separate from rubric scores.
//! - A **Metric** is a 0..1 score on a single axis (faithfulness,
//!   answer_relevance, context_precision, context_recall, latency
//!   p95, …) attached to one case. RAGAS / DeepEval are the
//!   reference rubric; tenant-defined metrics ride on the same
//!   `name` column without DDL.
//!
//! Every entity carries `workspace_id` and is enforced via the
//! 4-clause RLS pattern (`ENABLE` + `FORCE` + `ws_isolation` +
//! `system_bypass`) — `tests/rls_invariants.rs` catalog scan
//! verifies on every migration.
//!
//! ## Why three tables, not one wide table
//!
//! Spreading the metric axes across columns forces a migration on
//! every new rubric and bakes the axis set into the schema.
//! Stage-one design lands the `(case_id, name)` long shape so the
//! evaluator can record an arbitrary mix of rubric scores per
//! case, the operator can pivot at query time, and adding a
//! tenant-defined metric is one INSERT rather than a DDL change.
//! Wide aggregates (per-run averages, percentiles) live in
//! materialised views that ride on the same long shape.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::error::OxError;

/// Status of an [`EvaluationRun`]. Wire shape is the snake_case
/// string ("running" / "succeeded" / …) so adding a future variant
/// is a Rust-side change with no migration; the catalog parity
/// test pins the string set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationRunStatus {
    /// The run is in progress — cases are being recorded.
    Running,
    /// Every case completed and at least one metric landed; the
    /// run is sealed and downstream dashboards consume the rows.
    Succeeded,
    /// The run aborted before completion (judge timeout, infra
    /// outage, dataset error). The error envelope lives in
    /// `metadata.failure` rather than a dedicated column so
    /// future failure-shape changes ride on JSONB.
    Failed,
    /// The run was deliberately stopped by an operator before it
    /// finished. Distinct from `Failed` because the cases that
    /// did record stay valid for analysis.
    Cancelled,
}

impl EvaluationRunStatus {
    /// Stable wire string — used by the SQL persistence layer and
    /// the parity test in `crates/ox-store/tests` to assert that
    /// every variant round-trips.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Inverse of [`Self::as_str`]. Returns `None` on an
    /// unrecognised tag — the caller decides whether that is a
    /// store-corruption error or a forward-compat skip.
    pub fn from_wire_str(s: &str) -> Option<Self> {
        Some(match s {
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    /// True for the two terminal states (`Succeeded`, `Failed`,
    /// `Cancelled`). The dashboard uses this to dim still-running
    /// rows.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Convert from the persisted TEXT column to the typed enum,
/// raising a structured error on an unrecognised tag. The error
/// surfaces to the caller as `OxError::Conflict` so a future-shape
/// row from a forward deploy fails fast rather than silently
/// downgrading to a default.
pub fn parse_run_status(raw: &str) -> Result<EvaluationRunStatus, OxError> {
    EvaluationRunStatus::from_wire_str(raw).ok_or_else(|| OxError::Conflict {
        message: format!("unknown evaluation_runs.status `{raw}`"),
    })
}

/// One evaluation batch. Mirrors the `evaluation_runs` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRun {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Optional pin to a committed ontology version. `None`
    /// during draft-stage evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_version_id: Option<Uuid>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub status: EvaluationRunStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Run-level configuration envelope (model id, dataset
    /// reference, judge id, …). Schema-less so a new
    /// run-level dimension never needs DDL.
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

/// One (input, expected, actual) tuple inside a run. Mirrors the
/// `evaluation_cases` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationCase {
    pub id: Uuid,
    pub run_id: Uuid,
    pub workspace_id: Uuid,
    /// Stable per-run identifier. UPSERT key alongside `run_id`.
    pub case_key: String,
    /// Prompt / context envelope.
    pub input: serde_json::Value,
    /// Golden / reference outcome. Absent for unsupervised
    /// evaluations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    /// Observed outcome. Absent until the evaluator records;
    /// presence of `error` with a `None` `actual` indicates the
    /// case threw before producing output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock latency from invocation to completion. None
    /// when the case has not yet run or threw before timing
    /// could be measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// One score on one rubric axis for one case. Mirrors the
/// `evaluation_metrics` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationMetric {
    pub id: Uuid,
    pub case_id: Uuid,
    pub workspace_id: Uuid,
    /// Free-form rubric name. RAGAS canonicals
    /// (`faithfulness`, `answer_relevance`, `context_precision`,
    /// `context_recall`) plus tenant-defined names.
    pub name: String,
    /// Normalised to `[0.0, 1.0]`. Validation is the caller's
    /// concern — the storage layer keeps the column unbounded so
    /// rubrics with a different domain (latency p95 ms,
    /// hallucination count) can ride on the same column without
    /// a forced rescale.
    pub score: f64,
    /// Optional natural-language reasoning emitted by an LLM
    /// judge. Absent for code-side rubrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Per-metric configuration (judge model, prompt version,
    /// rubric template, …). Schema-less.
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

fn default_metadata() -> serde_json::Value {
    serde_json::json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_round_trips_through_wire_string() {
        for s in [
            EvaluationRunStatus::Running,
            EvaluationRunStatus::Succeeded,
            EvaluationRunStatus::Failed,
            EvaluationRunStatus::Cancelled,
        ] {
            let wire = s.as_str();
            let back = EvaluationRunStatus::from_wire_str(wire);
            assert_eq!(back, Some(s), "round-trip failed for {wire}");
        }
    }

    #[test]
    fn run_status_unknown_tag_is_rejected() {
        let err = parse_run_status("bogus");
        assert!(err.is_err());
        let e = err.unwrap_err();
        let s = format!("{e:?}");
        assert!(s.contains("bogus"), "diagnostic must name the tag: {s}");
    }

    #[test]
    fn run_status_terminal_states() {
        assert!(!EvaluationRunStatus::Running.is_terminal());
        assert!(EvaluationRunStatus::Succeeded.is_terminal());
        assert!(EvaluationRunStatus::Failed.is_terminal());
        assert!(EvaluationRunStatus::Cancelled.is_terminal());
    }

    #[test]
    fn run_serialises_with_status_as_snake_case() {
        let run = EvaluationRun {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            ontology_version_id: None,
            name: "rag-baseline".into(),
            description: String::new(),
            status: EvaluationRunStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            metadata: serde_json::json!({"model": "gpt"}),
        };
        let v = serde_json::to_value(&run).unwrap();
        assert_eq!(v.get("status").and_then(|s| s.as_str()), Some("running"));
        let back: EvaluationRun = serde_json::from_value(v).unwrap();
        assert_eq!(back.status, EvaluationRunStatus::Running);
    }

    #[test]
    fn case_omits_absent_optional_fields_on_wire() {
        let case = EvaluationCase {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            case_key: "q01".into(),
            input: serde_json::json!({"q": "?"}),
            expected: None,
            actual: None,
            error: None,
            latency_ms: None,
            created_at: Utc::now(),
        };
        let v = serde_json::to_value(&case).unwrap();
        assert!(v.get("expected").is_none(), "absent expected must be skipped on wire");
        assert!(v.get("actual").is_none());
        assert!(v.get("error").is_none());
        assert!(v.get("latency_ms").is_none());
    }

    #[test]
    fn metric_round_trips_with_optional_reasoning() {
        let m = EvaluationMetric {
            id: Uuid::new_v4(),
            case_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: "faithfulness".into(),
            score: 0.92,
            reasoning: Some("supports every claim".into()),
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        };
        let v = serde_json::to_value(&m).unwrap();
        let back: EvaluationMetric = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
    }
}
