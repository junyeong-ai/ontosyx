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

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

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
    /// Lineage to the [`EvaluationDataset`] the run materialised
    /// from. `None` for ad-hoc runs whose cases were inserted
    /// directly via the bulk-upsert path. Run comparison + CI
    /// regression require this — a diff between two runs only
    /// makes sense when both reference the same dataset id.
    /// `ON DELETE SET NULL` so historical runs survive a dataset
    /// purge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<Uuid>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub status: EvaluationRunStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Run-level configuration envelope (model id, judge id, …).
    /// Schema-less so a new run-level dimension never needs DDL.
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

/// Frozen header for a reusable evaluation dataset. Mirrors the
/// `evaluation_datasets` row. Datasets are the unit of input
/// authoring — a curated collection of `(input, expected)`
/// pairs reused across runs (model A vs B regression diff, CI
/// golden gate, baseline pinning). The header carries no items
/// directly; items live in [`EvaluationDatasetItem`] keyed on
/// `dataset_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationDataset {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Workspace-unique identifier. The UPSERT-by-name flow on
    /// dataset re-import collapses on this column.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub created_at: DateTime<Utc>,
}

/// One frozen `(input, expected)` pair inside a dataset.
/// Mirrors the `evaluation_dataset_items` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationDatasetItem {
    pub id: Uuid,
    pub dataset_id: Uuid,
    pub workspace_id: Uuid,
    /// Stable per-dataset identifier. UPSERT key alongside
    /// `dataset_id`; re-importing the dataset from CSV / JSON
    /// replaces previous rows on this key.
    pub item_key: String,
    /// Prompt / context envelope. Mirrors `EvaluationCase.input`
    /// shape so `create_run_from_dataset` is a straight copy.
    pub input: serde_json::Value,
    /// Reference outcome. `None` for unsupervised datasets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    /// Free-form authoring metadata (tags, difficulty, locale, …).
    /// Renders verbatim on the dataset detail panel.
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
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
    /// Free-form audit envelope stamped by the executor. The
    /// canonical key is `call_provenance` carrying the
    /// `CallProvenance` shape (prompt_id + prompt_version +
    /// prompt_render_hash + model_id + max_tokens +
    /// temperature) so eval-failure drill-down resolves to the
    /// exact LLM call without re-running. Unused keys here are
    /// rendered verbatim by the FE detail panel; future
    /// case-execute kinds (retrieval, action invocation) land
    /// their own envelope keys without a schema migration.
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Deterministic IR scoring for retrieval evaluation cases.
/// Operates on a ranked list of returned anchor ids + the gold-
/// standard set; emits the four canonical IR axes the literature
/// agrees on (precision@k, recall@k, MRR, NDCG@k). Pure function,
/// no LLM round-trip — retrieval cases land their metrics
/// synchronously inside the case-execute handler.
pub fn score_retrieval_metrics(
    actual_anchors: &[String],
    expected_anchors: &[String],
    k: usize,
) -> RetrievalMetrics {
    let k_capped = actual_anchors.len().min(k.max(1));
    let topk: &[String] = &actual_anchors[..k_capped];
    let expected_set: std::collections::BTreeSet<&String> = expected_anchors.iter().collect();
    let expected_count = expected_set.len() as f64;

    // precision@k = |topk ∩ expected| / k_capped
    let topk_hits = topk.iter().filter(|a| expected_set.contains(a)).count();
    let precision = if k_capped == 0 {
        0.0
    } else {
        topk_hits as f64 / k_capped as f64
    };
    // recall@k = |topk ∩ expected| / |expected|
    let recall = if expected_count == 0.0 {
        // No gold-standard anchors authored — vacuously 1.0
        // (every expected item retrieved). Mirrors the
        // information-retrieval convention for empty relevance
        // sets; the FE renders the case as "no expected
        // anchors authored" so the score doesn't mislead.
        1.0
    } else {
        topk_hits as f64 / expected_count
    };
    // MRR = 1 / rank_of_first_relevant. 0.0 when no relevant in
    // topk. Iterate full `actual_anchors` (not capped) so a
    // relevant anchor at position k+1 still scores nothing,
    // matching `mrr@k` semantics.
    let mrr = topk
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            if expected_set.contains(a) {
                Some(1.0 / (i + 1) as f64)
            } else {
                None
            }
        })
        .unwrap_or(0.0);
    // NDCG@k with binary relevance — relevance = 1 if
    // anchor ∈ expected, 0 otherwise. DCG = Σ rel_i / log2(i + 2).
    // IDCG = ideal ordering (every relevant first).
    let dcg: f64 = topk
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if expected_set.contains(a) {
                1.0 / ((i as f64 + 2.0).log2())
            } else {
                0.0
            }
        })
        .sum();
    let ideal_hits = expected_count.min(k_capped as f64) as usize;
    let idcg: f64 = (0..ideal_hits)
        .map(|i| 1.0 / ((i as f64 + 2.0).log2()))
        .sum();
    let ndcg = if idcg > 0.0 { dcg / idcg } else { 0.0 };

    RetrievalMetrics {
        k: k_capped as u32,
        topk_hit_count: topk_hits as u32,
        expected_count: expected_count as u32,
        precision_at_k: precision,
        recall_at_k: recall,
        mrr,
        ndcg_at_k: ndcg,
    }
}

/// IR-flavoured deterministic retrieval metrics. Each axis lands
/// as a separate `evaluation_metrics` row keyed
/// `retrieval.<axis>` so the existing dashboard / diff surfaces
/// pick them up uniformly with the LLM-judged axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalMetrics {
    pub k: u32,
    pub topk_hit_count: u32,
    pub expected_count: u32,
    pub precision_at_k: f64,
    pub recall_at_k: f64,
    pub mrr: f64,
    pub ndcg_at_k: f64,
}

/// One per-case axis-level diff between two runs over the same
/// dataset. Carries both raw scores so the FE diff panel renders
/// the comparison without re-fetching either side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetricDelta {
    /// Stable per-dataset identifier — the same `case_key` is
    /// present in both runs because both materialised from the
    /// same dataset. Renders as the row label on the FE diff
    /// table.
    pub case_key: String,
    /// Rubric axis (`faithfulness`, `answer_relevance`, …).
    /// Mirrors `EvaluationMetric.name`.
    pub axis: String,
    pub baseline_score: f64,
    pub candidate_score: f64,
    /// `candidate_score - baseline_score`. Positive = candidate
    /// improved; negative = regression. Sign convention chosen so
    /// the FE green-on-positive / red-on-negative styling is
    /// trivial.
    pub delta: f64,
}

/// Per-axis aggregate roll-up across every `(case_key, axis)`
/// pair both runs share. Surfaces the operator-facing summary on
/// the diff page header — "candidate beats baseline by 0.04 mean
/// faithfulness with Cohen's d 0.62 and 73% win rate over 30
/// cases".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAxisSummary {
    pub axis: String,
    /// Number of cases where both runs produced a score on this
    /// axis. The denominator for `mean_delta` / `win_rate_pct` /
    /// `cohen_d`.
    pub paired_case_count: u64,
    pub baseline_mean: f64,
    pub candidate_mean: f64,
    /// `candidate_mean - baseline_mean`. Positive = candidate
    /// improved.
    pub mean_delta: f64,
    /// Percentage of paired cases where `candidate_score >
    /// baseline_score`. `[0.0, 100.0]`. Ties (delta == 0.0) count
    /// as half a win — symmetric "beats baseline" measure.
    pub win_rate_pct: f64,
    /// Cohen's d effect size — `(mean_c - mean_b) / pooled_std`.
    /// Industry interpretation: `|d| < 0.2` negligible, `0.5`
    /// medium, `0.8` large. `None` when both runs produced
    /// identical scores (zero variance — `pooled_std == 0`); the
    /// FE renders a "—" in that cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohen_d: Option<f64>,
}

/// One axis aggregate for [`RunSummary`]. Per-axis mean across
/// every case scored on that axis. `count` is the denominator the
/// FE renders alongside the mean ("0.78 over 12 cases") so the
/// number isn't read in isolation — a 1.0 mean from one case is
/// not the same signal as a 0.78 mean from twelve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisAggregate {
    pub axis: String,
    pub mean: f64,
    pub count: u64,
}

/// Run-level summary returned by
/// [`crate::EvaluationStore::evaluation_run_summary`]. Drives
/// the run-list badge and the run-detail header card without
/// fanning out into per-case + per-metric round trips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: Uuid,
    /// Every case attached to the run.
    pub total_cases: u64,
    /// Cases with at least one metric tagged
    /// `metadata.kind = 'judge'` (RAGAS rubric). Operators
    /// surfacing "judged 5/12" want the RAGAS denominator,
    /// not the safety one — the safety judge is opt-in and
    /// reading both would conflate distinct signals.
    pub judged_cases: u64,
    /// Cases with `error IS NOT NULL` — the case-execute path
    /// raised before producing `actual`. The judge can't
    /// score them, the dashboard renders them in red.
    pub failed_cases: u64,
    /// Per-axis aggregate, sorted by `axis ASC` for stable
    /// FE rendering. Includes both RAGAS axes
    /// (`faithfulness` / `answer_relevance` / …) and safety
    /// axes (`safety.toxicity_safe` / …) when present.
    pub axis_means: Vec<AxisAggregate>,
}

/// Two-run comparison report. Returned by
/// [`crate::EvaluationStore::compare_evaluation_runs`]. Per-case
/// row diffs ride on `per_case`; aggregate summaries on `per_axis`.
/// Both ordered for deterministic FE rendering — case_key ASC,
/// axis ASC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunComparisonReport {
    pub baseline_run_id: Uuid,
    pub candidate_run_id: Uuid,
    /// Pinned dataset both runs reference. The store enforces
    /// matching `dataset_id` and rejects with `OxError::Validation`
    /// otherwise — diff between runs over different datasets is
    /// not meaningful.
    pub dataset_id: Uuid,
    pub per_case: Vec<RunMetricDelta>,
    pub per_axis: Vec<RunAxisSummary>,
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

// ---------------------------------------------------------------------------
// EvaluationContext — task-local capture handle
// ---------------------------------------------------------------------------

/// Per-task evaluation context. When set, callers along the
/// task's async path can read it through
/// [`current_evaluation_context`] and route latency / token
/// observations into [`EvaluationCapture::record_latency`] without
/// passing the run/case ids by hand.
///
/// Only the API endpoint that runs an evaluation case sets this —
/// production traffic does NOT carry an `EvaluationContext`, so the
/// capture path is silent for every other request.
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub run_id: Uuid,
    /// Stable per-run case identifier — pairs with `case_id` after
    /// the case row is created so the capture write keys the metric
    /// row correctly. Carried as the case's natural key so the
    /// context is meaningful even before a row exists in
    /// `evaluation_cases`.
    pub case_key: String,
    /// `evaluation_cases.id` for the row this case currently
    /// occupies. The capture call uses this as the metric's
    /// `case_id` foreign key. Carried alongside `case_key` because
    /// the case row may have been UPSERT-replaced between scope
    /// entry and the capture call — the runtime's responsibility
    /// is to keep these in lockstep when it re-emits a case mid-
    /// scope.
    pub case_id: Uuid,
}

tokio::task_local! {
    static EVALUATION_CONTEXT: EvaluationContext;
}

/// Read the current evaluation context, or `None` when the call
/// path is not inside an evaluation scope. Callers in production
/// hot paths should treat the `None` branch as fall-through —
/// no metric, no overhead.
pub fn current_evaluation_context() -> Option<EvaluationContext> {
    EVALUATION_CONTEXT.try_with(|c| c.clone()).ok()
}

/// Run `fut` inside an evaluation scope. The future and every
/// task it awaits sees the supplied [`EvaluationContext`] via
/// [`current_evaluation_context`] until the future resolves.
///
/// The scope is per-task — a `tokio::spawn` inside the future
/// does NOT inherit it (use `spawn_with_evaluation_context`
/// when a child task must inherit). The same isolation
/// contract `WORKSPACE_ID` follows.
pub async fn scope_evaluation_context<F, T>(ctx: EvaluationContext, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    EVALUATION_CONTEXT.scope(ctx, fut).await
}

// ---------------------------------------------------------------------------
// EvaluationCapture — pluggable metric emission for runtime hooks
// ---------------------------------------------------------------------------

/// Pluggable hook that records observations made inside an
/// evaluation scope. Implementations land where the storage
/// dependency makes sense (the canonical `EvaluationStore`-backed
/// impl lives on `PostgresStore`); consumers further up the stack
/// (e.g. `ox-brain`) hold an `Arc<dyn EvaluationCapture>` and
/// call through it without knowing whether the bytes flow to a
/// real DB or a test stub.
///
/// The trait deliberately mirrors a *single* metric kind today
/// (`record_latency`). Future axes (`record_tokens`,
/// `record_judge_output`) ride on additional methods with
/// default `noop` impls so a consumer never has to care about
/// the full surface — only the calls it cares about.
#[async_trait]
pub trait EvaluationCapture: Send + Sync {
    /// Record an LLM-call latency observation against the active
    /// case under the given `operation` axis. The default impl
    /// is a noop so a `NullCapture`-style harness can be built
    /// without writing the full table.
    async fn record_latency(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        latency_ms: i64,
    ) -> OxResult<()> {
        let _ = (ctx, operation, latency_ms);
        Ok(())
    }

    /// Record token-count observations against the active case
    /// for one LLM call. Two metrics land per call:
    /// `tokens.prompt.<operation>` (input) and
    /// `tokens.completion.<operation>` (output). Production cost
    /// + utilisation dashboards roll these up as the canonical
    /// throughput axis. The default impl is a noop so the trait
    /// stays drop-in for harnesses that don't care about token
    /// observability.
    async fn record_tokens(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> OxResult<()> {
        let _ = (ctx, operation, prompt_tokens, completion_tokens);
        Ok(())
    }

    /// Record an LLM-call cost observation in micro-USD against
    /// the active case under `cost_usd.<operation>`. Cost is
    /// stored as a numeric metric so the FE can sum across cases
    /// without re-deriving from per-call token counts × tariff
    /// (per-model cost tables drift; the captured value is the
    /// historical truth). The default impl is a noop.
    async fn record_cost_usd(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        cost_micro_usd: u64,
    ) -> OxResult<()> {
        let _ = (ctx, operation, cost_micro_usd);
        Ok(())
    }
}

/// `Arc<dyn EvaluationCapture>` that drops every observation on
/// the floor. The shared default for environments that haven't
/// wired the storage-backed implementation — production traffic
/// in workspaces that haven't enabled evaluation, embedded test
/// harnesses, etc. The `current_evaluation_context()` returning
/// `None` is the primary silence mechanism; this is the
/// secondary belt-and-suspenders so a consumer that holds an
/// `Arc<dyn EvaluationCapture>` always has a valid value.
pub struct NullEvaluationCapture;

#[async_trait]
impl EvaluationCapture for NullEvaluationCapture {}

impl NullEvaluationCapture {
    pub fn arc() -> Arc<dyn EvaluationCapture> {
        Arc::new(Self)
    }
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
            dataset_id: None,
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
            metadata: serde_json::Value::Object(Default::default()),
            created_at: Utc::now(),
        };
        let v = serde_json::to_value(&case).unwrap();
        assert!(v.get("expected").is_none(), "absent expected must be skipped on wire");
        assert!(v.get("actual").is_none());
        assert!(v.get("error").is_none());
        assert!(v.get("latency_ms").is_none());
    }

    #[tokio::test]
    async fn evaluation_context_visible_inside_scope() {
        assert!(current_evaluation_context().is_none());
        let ctx = EvaluationContext {
            run_id: Uuid::new_v4(),
            case_key: "q01".into(),
            case_id: Uuid::new_v4(),
        };
        let observed = scope_evaluation_context(ctx.clone(), async {
            current_evaluation_context()
        })
        .await;
        let observed = observed.expect("inside scope");
        assert_eq!(observed.run_id, ctx.run_id);
        assert_eq!(observed.case_key, ctx.case_key);
        assert_eq!(observed.case_id, ctx.case_id);
        // Outer scope: clean again.
        assert!(current_evaluation_context().is_none());
    }

    #[tokio::test]
    async fn null_capture_swallows_observations_silently() {
        let cap = NullEvaluationCapture::arc();
        let ctx = EvaluationContext {
            run_id: Uuid::new_v4(),
            case_key: "q01".into(),
            case_id: Uuid::new_v4(),
        };
        // Default `record_latency` is a noop — test pins the
        // contract so a future `EvaluationCapture` extension that
        // forgets to default to noop trips this test.
        cap.record_latency(&ctx, "translate_query", 42).await.unwrap();
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

    #[test]
    fn retrieval_perfect_top_k_yields_all_ones() {
        let actual = vec!["a".into(), "b".into(), "c".into()];
        let expected = vec!["a".into(), "b".into(), "c".into()];
        let m = score_retrieval_metrics(&actual, &expected, 3);
        assert_eq!(m.precision_at_k, 1.0);
        assert_eq!(m.recall_at_k, 1.0);
        assert_eq!(m.mrr, 1.0);
        // NDCG@3 with all 3 relevant in ideal order = 1.0
        assert!((m.ndcg_at_k - 1.0).abs() < 1e-9);
    }

    #[test]
    fn retrieval_partial_match_scores_correctly() {
        // top-3 = [hit, miss, hit]; expected = {hit, hit, hit-not-in-topk}
        let actual = vec!["a".into(), "miss".into(), "c".into()];
        let expected = vec!["a".into(), "c".into(), "z".into()];
        let m = score_retrieval_metrics(&actual, &expected, 3);
        assert!((m.precision_at_k - 2.0 / 3.0).abs() < 1e-9);
        assert!((m.recall_at_k - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(m.mrr, 1.0); // first hit at position 1
        assert_eq!(m.topk_hit_count, 2);
    }

    #[test]
    fn retrieval_no_overlap_yields_zeros() {
        let actual = vec!["a".into(), "b".into(), "c".into()];
        let expected = vec!["x".into(), "y".into()];
        let m = score_retrieval_metrics(&actual, &expected, 3);
        assert_eq!(m.precision_at_k, 0.0);
        assert_eq!(m.recall_at_k, 0.0);
        assert_eq!(m.mrr, 0.0);
        assert_eq!(m.ndcg_at_k, 0.0);
    }

    #[test]
    fn retrieval_empty_expected_set_treats_recall_as_one() {
        // No gold-standard anchors — vacuously perfect recall.
        let actual = vec!["a".into()];
        let expected: Vec<String> = vec![];
        let m = score_retrieval_metrics(&actual, &expected, 1);
        assert_eq!(m.recall_at_k, 1.0);
        assert_eq!(m.precision_at_k, 0.0);
        assert_eq!(m.expected_count, 0);
    }

    #[test]
    fn retrieval_first_match_at_position_2_yields_mrr_half() {
        let actual = vec!["miss".into(), "hit".into(), "miss2".into()];
        let expected = vec!["hit".into()];
        let m = score_retrieval_metrics(&actual, &expected, 3);
        assert!((m.mrr - 0.5).abs() < 1e-9);
    }

    #[test]
    fn run_axis_summary_round_trips() {
        let s = RunAxisSummary {
            axis: "faithfulness".into(),
            paired_case_count: 30,
            baseline_mean: 0.82,
            candidate_mean: 0.86,
            mean_delta: 0.04,
            win_rate_pct: 73.0,
            cohen_d: Some(0.62),
        };
        let v = serde_json::to_value(&s).unwrap();
        let back: RunAxisSummary = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn run_summary_round_trips_with_axis_aggregates() {
        let s = RunSummary {
            run_id: Uuid::new_v4(),
            total_cases: 12,
            judged_cases: 5,
            failed_cases: 1,
            axis_means: vec![
                AxisAggregate {
                    axis: "answer_relevance".into(),
                    mean: 0.83,
                    count: 5,
                },
                AxisAggregate {
                    axis: "faithfulness".into(),
                    mean: 0.78,
                    count: 5,
                },
                AxisAggregate {
                    axis: "safety.toxicity_safe".into(),
                    mean: 0.95,
                    count: 3,
                },
            ],
        };
        let v = serde_json::to_value(&s).unwrap();
        // Wire shape pin: counts are u64, axis_means is an
        // array of {axis, mean, count}. The FE renders these
        // verbatim — drift here would silently break the
        // run-list badge.
        assert_eq!(v["total_cases"], 12);
        assert_eq!(v["judged_cases"], 5);
        assert_eq!(v["failed_cases"], 1);
        assert_eq!(v["axis_means"][0]["axis"], "answer_relevance");
        assert_eq!(v["axis_means"][2]["axis"], "safety.toxicity_safe");
        let back: RunSummary = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn run_axis_summary_omits_cohen_d_when_undefined() {
        let s = RunAxisSummary {
            axis: "exact_match".into(),
            paired_case_count: 1,
            baseline_mean: 1.0,
            candidate_mean: 1.0,
            mean_delta: 0.0,
            win_rate_pct: 50.0,
            cohen_d: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("cohen_d").is_none(), "absent cohen_d skipped on wire");
    }

    #[test]
    fn run_comparison_report_round_trips() {
        let r = RunComparisonReport {
            baseline_run_id: Uuid::new_v4(),
            candidate_run_id: Uuid::new_v4(),
            dataset_id: Uuid::new_v4(),
            per_case: vec![RunMetricDelta {
                case_key: "q01".into(),
                axis: "faithfulness".into(),
                baseline_score: 0.8,
                candidate_score: 0.9,
                delta: 0.1,
            }],
            per_axis: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        let back: RunComparisonReport = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }
}
