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
use ox_ontology::{EvaluationFingerprint, ModelCall};
use ox_query_ir::query::QueryIR;

/// Status of an [`EvaluationRun`]. Wire shape is the snake_case
/// string ("running" / "succeeded" / …) so adding a future variant
/// is a Rust-side change with no migration; the catalog parity
/// test pins the string set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
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
///
/// Reproducibility components (ontology version, dataset, model,
/// prompt template, decoding config, retrieval profile) live in
/// [`EvaluationFingerprint`] — the typed bundle every run pins at
/// construction. The legacy nullable `ontology_version_id` /
/// `dataset_id` columns are gone: a run that cannot answer the
/// question "which ontology version did I score against?" is
/// uninterpretable, and the platform refuses to author one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EvaluationRun {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Typed reproducibility bundle. Persisted as
    /// `evaluation_runs.fingerprint_components JSONB NOT NULL`;
    /// the digest below carries the equality token for fast
    /// "same configuration" lookups.
    pub fingerprint: EvaluationFingerprint,
    /// SHA-256 of [`EvaluationFingerprint::digest`], persisted as
    /// `evaluation_runs.fingerprint_digest VARCHAR(64) NOT NULL`.
    /// Two runs are configured identically iff their digests match.
    pub fingerprint_digest: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub status: EvaluationRunStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Free-form audit envelope. Reproducibility components live
    /// on [`Self::fingerprint`] — operator notes, run labels, and
    /// other non-configuration tags ride here.
    #[serde(default = "default_metadata")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
}

/// Frozen header for a reusable evaluation dataset. Mirrors the
/// `evaluation_datasets` row. Datasets are the unit of input
/// authoring — a curated collection of `(input, expected)`
/// pairs reused across runs (model A vs B regression diff, CI
/// golden gate, baseline pinning). The header carries no items
/// directly; items live in [`EvaluationDatasetItem`] keyed on
/// `dataset_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    pub input: EvaluationCaseInput,
    /// Reference outcome. `None` for unsupervised datasets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<EvaluationExpected>,
    /// Free-form authoring metadata (tags, difficulty, locale, …).
    /// Renders verbatim on the dataset detail panel.
    #[serde(default = "default_metadata")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// One (input, expected, actual) tuple inside a run. Mirrors the
/// `evaluation_cases` row.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EvaluationCase {
    pub id: Uuid,
    pub run_id: Uuid,
    pub workspace_id: Uuid,
    /// Stable per-run identifier. UPSERT key alongside `run_id`.
    pub case_key: String,
    /// Prompt / context envelope.
    pub input: EvaluationCaseInput,
    /// Golden / reference outcome. Absent for unsupervised
    /// evaluations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<EvaluationExpected>,
    /// Observed outcome. Absent until the evaluator records;
    /// presence of `error` with a `None` `actual` indicates the
    /// case threw before producing output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<EvaluationActual>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock latency from invocation to completion. None
    /// when the case has not yet run or threw before timing
    /// could be measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    /// Audit envelope stamped by the executor or sampler.
    #[serde(default)]
    pub metadata: EvaluationCaseMetadata,
    pub created_at: DateTime<Utc>,
}

/// Case-level metadata. Kept closed because these envelopes drive
/// replay/audit UI, not arbitrary user annotations.
///
/// Per-case audit data is the *render hash* — the SHA-256 of the
/// fully-interpolated prompt body that fed the model for THIS case.
/// Run-level pins (prompt template id, template version, model id,
/// decoding config) live on [`EvaluationRun::fingerprint`] because
/// they're invariant across every case in the run; per-case Call
/// metadata only carries the dimension that actually varies per
/// case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationCaseMetadata {
    #[default]
    None,
    Call {
        /// SHA-256 of the system + user prompt after template
        /// interpolation. Replays the exact bytes that fed the
        /// model for this case; the run-level fingerprint pins
        /// the template id + version, this pins the rendered
        /// output.
        prompt_render_hash: String,
    },
    OnlineSampler,
}

/// Closed set of executable evaluation inputs. This is the
/// persisted case input and the dataset authoring surface, not just
/// an HTTP request DTO, so stored cases remain self-describing.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationCaseInput {
    /// Translate a natural-language question into `QueryIR`.
    TranslateQuery {
        question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_query_ir: Option<Box<QueryIR>>,
    },
    /// Free-form natural-language explanation / answer quality case.
    Explain {
        question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_answer: Option<String>,
    },
    /// Graph RAG anchor retrieval case over ontology navigation
    /// entry points.
    RetrieveAnchors {
        question: String,
        top_k: u32,
        #[serde(default)]
        expected_anchor_ids: Vec<String>,
    },
    /// Hybrid-vs-baseline retrieval comparison. Same question
    /// runs through both the hybrid (RRF 3-or-more ranker) path
    /// and the trigram-only baseline against the chosen
    /// retrieval surface; precision@k / recall@k / MRR / NDCG@k
    /// land for each leg, plus per-axis lift on the
    /// [`EvaluationActual::RetrievalComparison`] envelope.
    /// Drives the dashboard chart that surfaces "where does
    /// hybrid actually win?" without re-running the prompt.
    RetrievalComparison {
        question: String,
        surface: RetrievalSurface,
        top_k: u32,
        #[serde(default)]
        expected_ids: Vec<String>,
    },
}

/// The retrieval bank a [`EvaluationCaseInput::RetrievalComparison`]
/// targets. Each surface ships a hybrid path (RRF over trigram +
/// FTS + optional pgvector) and a trigram-only baseline; the
/// comparison runs both legs and captures the lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSurface {
    /// Verified-query bank — `(question, query_ir)` pairs the
    /// Brain consults as ICL exemplars (Φ11). Hybrid +
    /// `search_verified_queries_for_icl` (trigram baseline).
    VerifiedQuery,
    /// Microsoft GraphRAG community summaries (Φ10). Hybrid +
    /// `search_community_summaries_trigram_only` (baseline).
    CommunitySummary,
    /// Knowledge corrections / hints. Hybrid +
    /// `search_knowledge_entries_trigram_only` (baseline).
    KnowledgeEntry,
}

impl RetrievalSurface {
    /// Stable wire string; powers metric naming convention
    /// (`<surface>.<leg>.<axis>`) so dashboard pivots stay sortable.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedQuery => "verified_query",
            Self::CommunitySummary => "community_summary",
            Self::KnowledgeEntry => "knowledge_entry",
        }
    }
}

impl EvaluationCaseInput {
    pub fn question(&self) -> &str {
        match self {
            Self::TranslateQuery { question, .. }
            | Self::Explain { question, .. }
            | Self::RetrieveAnchors { question, .. }
            | Self::RetrievalComparison { question, .. } => question,
        }
    }

    pub fn expected(&self) -> Option<EvaluationExpected> {
        match self {
            Self::TranslateQuery {
                expected_query_ir, ..
            } => expected_query_ir
                .clone()
                .map(|query_ir| EvaluationExpected::QueryIr { query_ir }),
            Self::Explain {
                expected_answer, ..
            } => expected_answer
                .clone()
                .map(|answer| EvaluationExpected::Answer { answer }),
            Self::RetrieveAnchors {
                expected_anchor_ids,
                ..
            } => Some(EvaluationExpected::AnchorSet {
                anchor_ids: expected_anchor_ids.clone(),
            }),
            Self::RetrievalComparison { expected_ids, .. } => Some(EvaluationExpected::AnchorSet {
                anchor_ids: expected_ids.clone(),
            }),
        }
    }

    pub fn requires_canonical_ontology(&self) -> bool {
        matches!(
            self,
            Self::TranslateQuery { .. }
                | Self::RetrieveAnchors { .. }
                | Self::RetrievalComparison { .. }
        )
    }
}

/// Golden/reference outcome attached to an evaluation case.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationExpected {
    QueryIr { query_ir: Box<QueryIR> },
    Answer { answer: String },
    AnchorSet { anchor_ids: Vec<String> },
}

impl EvaluationExpected {
    pub fn from_actual(actual: &EvaluationActual) -> Self {
        match actual {
            EvaluationActual::QueryIr { query_ir } => Self::QueryIr {
                query_ir: query_ir.clone(),
            },
            EvaluationActual::Explanation { content, .. } => Self::Answer {
                answer: content.clone(),
            },
            EvaluationActual::RetrievedAnchors { anchor_ids, .. } => Self::AnchorSet {
                anchor_ids: anchor_ids.clone(),
            },
            EvaluationActual::RetrievalComparison { hybrid, .. } => Self::AnchorSet {
                anchor_ids: hybrid.anchor_ids.clone(),
            },
        }
    }
}

/// Observed outcome attached to an evaluation case after execution.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationActual {
    QueryIr {
        query_ir: Box<QueryIR>,
    },
    Explanation {
        content: String,
        model: String,
    },
    RetrievedAnchors {
        anchor_ids: Vec<String>,
        hits: Vec<EvaluationRetrievedAnchor>,
        metrics: RetrievalMetrics,
    },
    /// Side-by-side hybrid + trigram baseline retrieval over
    /// the chosen surface. Each leg carries its own ranked list
    /// alongside IR metrics; the FE dashboard pivots
    /// `hybrid - trigram` per axis to surface where hybrid
    /// actually moves the needle.
    RetrievalComparison {
        surface: RetrievalSurface,
        hybrid: RetrievalLeg,
        trigram: RetrievalLeg,
    },
}

/// One leg of an [`EvaluationActual::RetrievalComparison`] —
/// either the hybrid path or the trigram-only baseline.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalLeg {
    pub anchor_ids: Vec<String>,
    pub hits: Vec<EvaluationRetrievedAnchor>,
    pub metrics: RetrievalMetrics,
}

/// One ranked retrieval hit captured on a graph-RAG evaluation case.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EvaluationRetrievedAnchor {
    pub entity_kind: String,
    pub logical_id: String,
    pub doc: String,
    pub score: f64,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalMetrics {
    pub k: u32,
    pub topk_hit_count: u32,
    pub expected_count: u32,
    pub precision_at_k: f64,
    pub recall_at_k: f64,
    pub mrr: f64,
    pub ndcg_at_k: f64,
}

/// Dataset header + per-row aggregate. The list endpoint
/// returns this shape rather than bare [`EvaluationDataset`] so
/// the FE can surface "12 items" inline next to each row
/// without an N+1 fetch. Headers stay separate from the
/// aggregate; the canonical [`EvaluationDataset`] shape is
/// unchanged for every other caller (get / upsert / delete).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EvaluationDatasetSummary {
    pub dataset: EvaluationDataset,
    /// Total `evaluation_dataset_items` rows under this
    /// dataset. `0` for freshly-declared headers; the FE
    /// renders a muted-tone "Empty" pill when zero.
    pub item_count: u64,
}

/// One per-case axis-level diff between two runs over the same
/// dataset. Carries both raw scores so the FE diff panel renders
/// the comparison without re-fetching either side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AxisAggregate {
    pub axis: String,
    pub mean: f64,
    pub count: u64,
}

/// Run-level summary returned by
/// [`crate::EvaluationStore::evaluation_run_summary`]. Drives
/// the run-list badge and the run-detail header card without
/// fanning out into per-case + per-metric round trips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// Per-metric provenance and capture context.
    #[serde(default)]
    pub metadata: EvaluationMetricMetadata,
    /// PROV-O activity that produced this score. `Some` for
    /// LLM-judged rows (RAGAS / safety judge); `None` for
    /// capture-axis observations (latency / tokens / cost) whose
    /// provenance is attached to the underlying case via
    /// [`EvaluationCaseMetadata::Call`]. `ON DELETE RESTRICT` —
    /// the audit trail outlives the metric.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Metric-level provenance. The enum is intentionally finite so
/// dashboards can group and filter metric rows without probing
/// arbitrary JSON keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationMetricMetadata {
    #[default]
    None,
    Judge {
        run_id: Uuid,
        case_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<EvaluationJudgeSource>,
    },
    SafetyJudge {
        run_id: Uuid,
        case_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<EvaluationJudgeSource>,
    },
    Retrieval {
        k: u32,
        topk_hit_count: u32,
        expected_count: u32,
    },
    Capture {
        axis: EvaluationCaptureAxis,
        operation: String,
        run_id: Uuid,
        case_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationJudgeSource {
    AsyncWorker,
}

/// Axis tag for [`EvaluationMetricMetadata::Capture`] rows. One
/// `record_call` invocation lands one row per axis; splitting the
/// token axis into input / output / cached_input lets the operator
/// pivot per-axis without re-parsing a composite name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCaptureAxis {
    /// Wall-clock latency from invocation to completion (ms).
    LatencyMs,
    /// Tokens billed at the full input rate (cache miss).
    InputTokens,
    /// Tokens generated by the model.
    OutputTokens,
    /// Subset of input tokens that hit the prompt cache. Always
    /// recorded even when zero so the operator distinguishes "no
    /// cache" from "cache axis missing on this provider."
    CachedInputTokens,
    /// Cost in micro-USD (1e-6 USD), derived from
    /// [`ox_ontology::ModelPrices`] active at write time.
    CostMicroUsd,
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
/// (e.g. `ox-brain`) hold an `Arc<dyn EvaluationCapture>` and call
/// through it without knowing whether the bytes flow to a real DB
/// or a test stub.
///
/// One LLM call → one [`record_call`] invocation → one row per
/// axis ([`EvaluationCaptureAxis`]). Cost is derived from the
/// active [`ox_ontology::ModelPrices`] row + persisted as the
/// historical truth, so a tariff revision does not silently
/// rewrite history.
#[async_trait]
pub trait EvaluationCapture: Send + Sync {
    /// Record one LLM call observation against the active case.
    ///
    /// Splits into four to five `evaluation_metrics` rows
    /// (`latency_ms`, `tokens.input`, `tokens.output`,
    /// `tokens.cached_input` when non-zero, `cost_micro_usd` when a
    /// price row applies); the operator pivots per-axis without
    /// re-parsing a composite name. The default impl is a noop so a
    /// `NullEvaluationCapture`-style harness stays drop-in.
    async fn record_call(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        call: ModelCall,
    ) -> OxResult<()> {
        let _ = (ctx, operation, call);
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
        let fingerprint = EvaluationFingerprint {
            ontology_version_id: Uuid::nil(),
            dataset_id: Uuid::nil(),
            model_id: ox_ontology::ModelId::new("anthropic/claude-opus-4-7"),
            prompt_template_id: None,
            prompt_template_version: None,
            decoding_config_hash: None,
            retrieval_profile_id: None,
        };
        let digest = fingerprint.digest().unwrap();
        let run = EvaluationRun {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            fingerprint,
            fingerprint_digest: digest,
            name: "rag-baseline".into(),
            description: String::new(),
            status: EvaluationRunStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            metadata: serde_json::json!({"label": "baseline"}),
        };
        let v = serde_json::to_value(&run).unwrap();
        assert_eq!(v.get("status").and_then(|s| s.as_str()), Some("running"));
        assert!(
            v.get("fingerprint_digest").is_some(),
            "fingerprint digest must round-trip on the wire"
        );
        let back: EvaluationRun = serde_json::from_value(v).unwrap();
        assert_eq!(back.status, EvaluationRunStatus::Running);
        assert_eq!(back.fingerprint_digest, run.fingerprint_digest);
    }

    #[test]
    fn case_omits_absent_optional_fields_on_wire() {
        let case = EvaluationCase {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            case_key: "q01".into(),
            input: EvaluationCaseInput::Explain {
                question: "?".into(),
                expected_answer: None,
            },
            expected: None,
            actual: None,
            error: None,
            latency_ms: None,
            metadata: EvaluationCaseMetadata::default(),
            created_at: Utc::now(),
        };
        let v = serde_json::to_value(&case).unwrap();
        assert!(
            v.get("expected").is_none(),
            "absent expected must be skipped on wire"
        );
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
        let observed =
            scope_evaluation_context(ctx.clone(), async { current_evaluation_context() }).await;
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
        let call = ModelCall {
            model_id: ox_ontology::ModelId::new("anthropic/claude-haiku-4-5"),
            input_tokens: 100,
            output_tokens: 50,
            cached_input_tokens: 0,
            latency_ms: 42,
        };
        // Default `record_call` is a noop — test pins the contract
        // so a future `EvaluationCapture` extension that forgets to
        // default to noop trips this test.
        cap.record_call(&ctx, "translate_query", call).await.unwrap();
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
            metadata: EvaluationMetricMetadata::default(),
            provenance_id: None,
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

    #[test]
    fn retrieval_surface_wire_string_pinned() {
        // Stable contract — every dashboard pivot keys metrics
        // by these strings.
        assert_eq!(RetrievalSurface::VerifiedQuery.as_str(), "verified_query");
        assert_eq!(
            RetrievalSurface::CommunitySummary.as_str(),
            "community_summary"
        );
        assert_eq!(RetrievalSurface::KnowledgeEntry.as_str(), "knowledge_entry");
    }

    #[test]
    fn retrieval_comparison_case_input_round_trips() {
        let input = EvaluationCaseInput::RetrievalComparison {
            question: "월간 활성 사용자 추이는?".into(),
            surface: RetrievalSurface::CommunitySummary,
            top_k: 10,
            expected_ids: vec!["leiden:0:7".into(), "leiden:1:3".into()],
        };
        let v = serde_json::to_value(&input).unwrap();
        // Wire shape pin: tag=`retrieval_comparison`, surface as
        // snake_case, expected_ids as parallel array.
        assert_eq!(v["kind"], "retrieval_comparison");
        assert_eq!(v["surface"], "community_summary");
        assert_eq!(v["top_k"], 10);
        let back: EvaluationCaseInput = serde_json::from_value(v).unwrap();
        match back {
            EvaluationCaseInput::RetrievalComparison {
                question,
                surface,
                top_k,
                expected_ids,
            } => {
                assert_eq!(question, "월간 활성 사용자 추이는?");
                assert_eq!(surface, RetrievalSurface::CommunitySummary);
                assert_eq!(top_k, 10);
                assert_eq!(expected_ids.len(), 2);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn retrieval_comparison_marks_canonical_required() {
        let input = EvaluationCaseInput::RetrievalComparison {
            question: "q".into(),
            surface: RetrievalSurface::VerifiedQuery,
            top_k: 5,
            expected_ids: vec![],
        };
        // Comparison cases need the canonical ontology version
        // for community / knowledge surface routing — same gate
        // RetrieveAnchors uses.
        assert!(input.requires_canonical_ontology());
    }

    #[test]
    fn retrieval_comparison_question_accessor() {
        let input = EvaluationCaseInput::RetrievalComparison {
            question: "고객 LTV 계산?".into(),
            surface: RetrievalSurface::KnowledgeEntry,
            top_k: 3,
            expected_ids: vec![],
        };
        assert_eq!(input.question(), "고객 LTV 계산?");
    }

    #[test]
    fn retrieval_comparison_expected_collapses_to_anchor_set() {
        let input = EvaluationCaseInput::RetrievalComparison {
            question: "q".into(),
            surface: RetrievalSurface::VerifiedQuery,
            top_k: 5,
            expected_ids: vec!["vq-aa".into(), "vq-bb".into()],
        };
        match input.expected() {
            Some(EvaluationExpected::AnchorSet { anchor_ids }) => {
                assert_eq!(anchor_ids, vec!["vq-aa".to_string(), "vq-bb".to_string()]);
            }
            other => panic!("expected AnchorSet, got {other:?}"),
        }
    }
}
