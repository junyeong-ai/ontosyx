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

crate::wire_enum! {
    /// Status of an [`EvaluationRun`]. Wire shape is the
    /// snake_case string ("running" / "succeeded" / …) so adding
    /// a future variant is a Rust-side change with no migration;
    /// the catalog parity test pins the string set.
    pub enum EvaluationRunStatus {
        /// The run is in progress — cases are being recorded.
        Running => "running",
        /// Every case completed and at least one metric landed;
        /// the run is sealed and downstream dashboards consume
        /// the rows.
        Succeeded => "succeeded",
        /// The run aborted before completion (judge timeout,
        /// infra outage, dataset error). The error envelope lives
        /// in `metadata.failure` rather than a dedicated column
        /// so future failure-shape changes ride on JSONB.
        Failed => "failed",
        /// The run was deliberately stopped by an operator
        /// before it finished. Distinct from `Failed` because
        /// the cases that did record stay valid for analysis.
        Cancelled => "cancelled",
    }
}

impl EvaluationRunStatus {
    /// True for the three terminal states (`Succeeded`,
    /// `Failed`, `Cancelled`). The dashboard uses this to dim
    /// still-running rows.
    pub const fn is_terminal(self) -> bool {
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

crate::wire_enum! {
    /// Retrieval surface — the storage / IR target a comparison
    /// run scores. Three first-class surfaces today: verified
    /// queries (Φ11 ICL bank), community summaries (Φ10
    /// GraphRAG), and knowledge entries. Each pairs hybrid (RRF
    /// fusion) with a trigram-only baseline. Stable closed set;
    /// adding a surface lands here once and the SQL aggregators
    /// pick it up automatically through `all_wire_strings`.
    pub enum RetrievalSurface {
        /// Verified-query bank — `(question, query_ir)` pairs
        /// the Brain consults as ICL exemplars (Φ11).
        VerifiedQuery => "verified_query",
        /// Microsoft GraphRAG community summaries (Φ10).
        CommunitySummary => "community_summary",
        /// Knowledge corrections / hints.
        KnowledgeEntry => "knowledge_entry",
    }
}

crate::wire_enum! {
    /// Retrieval-comparison leg — which retrieval path a metric
    /// row belongs to. Pairs with [`RetrievalSurface`] +
    /// [`RetrievalAxis`] under the dotted metric naming
    /// convention (`<surface>.<leg>.<axis>`).
    pub enum RetrievalLeg {
        /// RRF fusion path — what the platform actually serves
        /// at runtime.
        Hybrid => "hybrid",
        /// Trigram-only baseline — what the platform served
        /// before hybrid retrieval. Drives the lift contrast.
        Trigram => "trigram",
    }
}

crate::wire_enum! {
    /// Retrieval IR axis — the four canonical metrics
    /// [`score_retrieval_metrics`] computes. Stable closed set;
    /// adding a new axis lands here AND on
    /// [`RetrievalMetrics`] AND on the case-execute persistence
    /// loop (one fresh `evaluation_metrics` row per axis).
    pub enum RetrievalAxis {
        PrecisionAtK => "precision_at_k",
        RecallAtK => "recall_at_k",
        Mrr => "mrr",
        NdcgAtK => "ndcg_at_k",
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
        hybrid: RetrievalLegResult,
        trigram: RetrievalLegResult,
    },
}

/// Outcome envelope for a single
/// [`EvaluationActual::RetrievalComparison`] leg — the ranked
/// list + the IR metrics scored against the gold-standard set.
/// Distinct from [`RetrievalLeg`], which is the leg
/// identifier (Hybrid / Trigram); this struct is what one leg
/// produced.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalLegResult {
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
    /// Per-(surface, axis) hybrid-vs-trigram aggregate,
    /// folded from the case-level `<surface>.<leg>.<axis>`
    /// metric rows. Empty when the run has no
    /// `retrieval_comparison` cases, so dashboards can switch
    /// the lift card on `len() > 0`. Sorted by `(surface,
    /// axis) ASC` for stable FE rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrieval_comparisons: Vec<RetrievalComparisonAggregate>,
}

/// Run-level aggregate for one `(surface, axis)` pair across
/// every `retrieval_comparison` case in the run. Drives the
/// "did hybrid actually help on this dataset?" question without
/// the dashboard having to parse the dotted metric-name
/// convention itself.
///
/// Pairing is per-case: a single case contributes at most one
/// (hybrid, trigram) pair per axis. Cases that produced only
/// one leg (BE half-failed) drop out of the denominator — the
/// pair has to be complete to count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalComparisonAggregate {
    pub surface: RetrievalSurface,
    /// Closed set — extending requires a [`RetrievalAxis`]
    /// variant + the matching `RetrievalMetrics` field. The
    /// SQL aggregator lifts the dotted-name axis tail and
    /// converts via [`RetrievalAxis::from_wire_str`]; an
    /// unknown axis drops the row from the typed aggregate.
    pub axis: RetrievalAxis,
    /// Cases where both `<surface>.hybrid.<axis>` and
    /// `<surface>.trigram.<axis>` landed. Denominator for
    /// `mean_lift` / `win_rate_pct`.
    pub paired_case_count: u64,
    pub hybrid_mean: f64,
    pub trigram_mean: f64,
    /// `hybrid_mean − trigram_mean`. Positive = hybrid wins on
    /// average across the run. Persisted as a derived value so
    /// the FE can render directly without re-computing across
    /// cells.
    pub mean_lift: f64,
    /// Percentage of paired cases where
    /// `hybrid_score > trigram_score`. Ties (delta == 0)
    /// count as half a win — same convention
    /// `RunComparisonReport.win_rate_pct` uses for the
    /// baseline-vs-candidate framing.
    pub win_rate_pct: f64,
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
    /// Per-(surface, axis) hybrid lift change between runs.
    /// `candidate_lift − baseline_lift` — positive means hybrid
    /// is helping more in the candidate run than the baseline.
    /// Empty when neither run has any `retrieval_comparison`
    /// cases, so the FE switches the lift-diff card on
    /// `len() > 0`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrieval_comparison_deltas: Vec<RetrievalComparisonDelta>,
    /// Regression alerts — cells whose `lift_delta` crossed the
    /// configured threshold AND have enough paired cases to
    /// trust the signal. Drives the diff-page alarm banner.
    /// Empty when no regression OR no `retrieval_comparison`
    /// cases at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrieval_lift_regressions: Vec<RetrievalLiftRegressionAlert>,
}

/// A single (surface, axis) regression alert. Detected when a
/// run-vs-run hybrid lift change crosses the configured
/// threshold with enough paired cases to clear the noise floor.
/// The diff page renders these as a danger-toned banner above
/// the lift table so an operator viewing the comparison sees
/// the alarm immediately.
///
/// `threshold` is echoed onto the wire so the FE renders "lift
/// dropped 0.08 (threshold −0.05)" without re-deriving the cut.
/// Future workspace-level threshold customisation lands as a
/// settings field; the wire shape is already prepared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalLiftRegressionAlert {
    pub surface: RetrievalSurface,
    pub axis: RetrievalAxis,
    /// `candidate_lift − baseline_lift`. Negative since the
    /// alert only fires when this crossed the negative
    /// threshold.
    pub lift_delta: f64,
    pub baseline_lift: f64,
    pub candidate_lift: f64,
    /// Threshold the alert cleared (e.g. `-0.05`). Echoed so
    /// the FE renders both the observed delta and the cut
    /// without re-deriving the constant.
    pub threshold: f64,
    /// Paired-case denominator on the candidate run — the
    /// statistic the alert was computed on. Surfaced so a
    /// reader can gauge significance.
    pub candidate_paired_case_count: u64,
}

/// Default threshold for the run-vs-run hybrid lift regression
/// alarm. Negative — the alert fires when
/// `lift_delta < threshold` (candidate retreated by more than
/// 0.05 lift points). Workspace settings override this via
/// [`WorkspaceEvaluationSettings`].
pub const RETRIEVAL_LIFT_REGRESSION_THRESHOLD: f64 = -0.05;

/// Default minimum paired-case count on the candidate run
/// before the regression alert fires. Suppresses noise from
/// runs with too few `retrieval_comparison` cases — a single
/// bad-actor case in a 2-case run shouldn't trigger an alarm.
/// Workspace settings override via
/// [`WorkspaceEvaluationSettings`].
pub const RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N: u64 = 3;

/// Regression alarm policy threaded through
/// [`crate::EvaluationStore::compare_evaluation_runs`]. Carries
/// the threshold + min-N gate the alarm uses; the route layer
/// loads it from the workspace's
/// [`WorkspaceEvaluationSettings`] (or platform defaults) and
/// passes it through. Keeping this explicit on the trait
/// signature means the store layer never reads workspace
/// settings implicitly — the seam is clean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalLiftRegressionPolicy {
    /// Negative cut. Alarm fires when
    /// `lift_delta < threshold`. Must be negative — the
    /// signature mirrors the platform default semantics.
    pub threshold: f64,
    /// Minimum paired-case denominator on the candidate run
    /// before the alarm fires.
    pub min_paired_case_count: u64,
}

impl RetrievalLiftRegressionPolicy {
    /// Platform-default policy — used when a workspace hasn't
    /// customised its settings. Pinned to the
    /// `RETRIEVAL_LIFT_REGRESSION_*` constants.
    pub const fn platform_default() -> Self {
        Self {
            threshold: RETRIEVAL_LIFT_REGRESSION_THRESHOLD,
            min_paired_case_count: RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N,
        }
    }
}

impl Default for RetrievalLiftRegressionPolicy {
    fn default() -> Self {
        Self::platform_default()
    }
}

/// Workspace-scoped evaluation settings, persisted on the
/// `workspaces.settings` JSONB column under the `evaluation`
/// key. Missing fields fall back to platform defaults at read
/// time — operators only persist the axes they want to override.
///
/// Wire shape (lives inside `workspaces.settings.evaluation`):
///
/// ```json
/// {
///   "retrieval_lift_regression_threshold": -0.10,
///   "retrieval_lift_regression_min_paired_case_count": 5
/// }
/// ```
///
/// Both fields ride on `#[serde(default)]` + skip-if-default so
/// a workspace that only overrides one axis writes a tight
/// payload, and a workspace that hasn't touched the setting
/// (the default) writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WorkspaceEvaluationSettings {
    #[serde(default = "default_lift_regression_threshold")]
    pub retrieval_lift_regression_threshold: f64,
    #[serde(default = "default_lift_regression_min_paired_n")]
    pub retrieval_lift_regression_min_paired_case_count: u64,
}

const fn default_lift_regression_threshold() -> f64 {
    RETRIEVAL_LIFT_REGRESSION_THRESHOLD
}

const fn default_lift_regression_min_paired_n() -> u64 {
    RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N
}

impl Default for WorkspaceEvaluationSettings {
    fn default() -> Self {
        Self {
            retrieval_lift_regression_threshold: RETRIEVAL_LIFT_REGRESSION_THRESHOLD,
            retrieval_lift_regression_min_paired_case_count:
                RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N,
        }
    }
}

impl WorkspaceEvaluationSettings {
    /// Compile a policy from these settings. The platform
    /// constants live on [`RetrievalLiftRegressionPolicy::platform_default`];
    /// this is the workspace-overridden variant.
    pub fn regression_policy(&self) -> RetrievalLiftRegressionPolicy {
        RetrievalLiftRegressionPolicy {
            threshold: self.retrieval_lift_regression_threshold,
            min_paired_case_count: self.retrieval_lift_regression_min_paired_case_count,
        }
    }

    /// Validation gate — same invariants the platform constants
    /// satisfy at compile time. Routes through
    /// [`is_valid_regression_threshold`] +
    /// [`is_valid_regression_min_paired_case_count`] so the
    /// runtime check and the compile-time `const _ assert` share
    /// a single predicate. Caller surfaces the typed error to
    /// operators editing the settings via the admin route.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !is_valid_regression_threshold(self.retrieval_lift_regression_threshold) {
            return Err(
                "retrieval_lift_regression_threshold must lie in (-1.0, 0.0) — \
                 negative, but bounded so a saturated value can't fire on every cell",
            );
        }
        if !is_valid_regression_min_paired_case_count(
            self.retrieval_lift_regression_min_paired_case_count,
        ) {
            return Err(
                "retrieval_lift_regression_min_paired_case_count must be >= 2 — \
                 single-case runs shouldn't trigger the alarm",
            );
        }
        Ok(())
    }
}

/// Pure validation predicate for the regression-alarm
/// threshold. `const fn` so the same predicate gates the
/// platform default at compile time AND the runtime
/// workspace-override at HTTP boundary — single source of
/// truth across both layers.
///
/// Returns `true` when the threshold lies in the open
/// interval `(-1.0, 0.0)`. Negative because the alarm fires
/// on `lift_delta < threshold`; bounded away from -1.0 so a
/// saturated value can't fire on every cell.
pub const fn is_valid_regression_threshold(threshold: f64) -> bool {
    threshold > -1.0 && threshold < 0.0
}

/// Pure validation predicate for the minimum paired-case
/// denominator. Mirrors [`is_valid_regression_threshold`] —
/// single source of truth across compile-time + runtime.
///
/// Returns `true` when `min_n >= 2`. A single-case denominator
/// would let any one bad-actor case fire the alarm on its own,
/// which is always noise.
pub const fn is_valid_regression_min_paired_case_count(min_n: u64) -> bool {
    min_n >= 2
}

// Compile-time invariants on the platform-default constants.
// Same predicates the runtime validator runs on user input —
// build refuses to ship if either constant drifts out of its
// honest range. Strictly stronger than a runtime test.
const _: () = assert!(
    is_valid_regression_threshold(RETRIEVAL_LIFT_REGRESSION_THRESHOLD),
    "RETRIEVAL_LIFT_REGRESSION_THRESHOLD must lie in (-1.0, 0.0)",
);
const _: () = assert!(
    is_valid_regression_min_paired_case_count(RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N),
    "RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N must be >= 2",
);

/// One case-level retrieval-comparison outlier — the case
/// whose `(hybrid − trigram)` lift dragged a (surface, axis)
/// cell's mean. Returned by
/// [`crate::EvaluationStore::list_run_comparison_outliers`]
/// for cell-level drill-down: the operator clicks "why did
/// hybrid retreat on community_summary.recall_at_k?" and the
/// dashboard surfaces this list of bad-actor cases.
///
/// `case_lift` is the per-case delta. Cases that lack one of
/// the legs (the metric pair didn't land) are filtered out at
/// the SQL gate — same paired contract the run-level
/// aggregate uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalComparisonOutlier {
    pub case_id: Uuid,
    pub case_key: String,
    pub surface: RetrievalSurface,
    pub axis: RetrievalAxis,
    pub hybrid_score: f64,
    pub trigram_score: f64,
    /// `hybrid_score − trigram_score`. Negative = hybrid lost
    /// on this case. The drill-down endpoint orders worst-first.
    pub case_lift: f64,
}

/// One run-vs-run lift delta for a `(surface, axis)` cell. The
/// dashboard renders this above the per-axis report so a
/// regression review sees "did the candidate run lose any of
/// the hybrid lift the baseline run captured?" without manual
/// arithmetic.
///
/// `paired_case_count_*` denominators are surfaced separately
/// because the two runs may differ — a candidate run that ran
/// fewer comparison cases shouldn't be read the same as one
/// with the same denominator. The FE displays both and lets the
/// operator gauge whether the delta is statistically meaningful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RetrievalComparisonDelta {
    pub surface: RetrievalSurface,
    pub axis: RetrievalAxis,
    pub baseline_lift: f64,
    pub candidate_lift: f64,
    /// `candidate_lift − baseline_lift`. Positive = hybrid
    /// improved between runs; negative = regression.
    pub lift_delta: f64,
    pub baseline_paired_case_count: u64,
    pub candidate_paired_case_count: u64,
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
            retrieval_comparisons: vec![],
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
        // Empty `retrieval_comparisons` is skipped on the wire so
        // dashboards that don't bundle the comparison card still
        // get a tight payload.
        assert!(v.get("retrieval_comparisons").is_none());
        let back: RunSummary = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn run_summary_emits_retrieval_comparisons_when_present() {
        let s = RunSummary {
            run_id: Uuid::new_v4(),
            total_cases: 4,
            judged_cases: 0,
            failed_cases: 0,
            axis_means: vec![],
            retrieval_comparisons: vec![
                RetrievalComparisonAggregate {
                    surface: RetrievalSurface::VerifiedQuery,
                    axis: RetrievalAxis::RecallAtK,
                    paired_case_count: 4,
                    hybrid_mean: 0.72,
                    trigram_mean: 0.55,
                    mean_lift: 0.17,
                    win_rate_pct: 75.0,
                },
                RetrievalComparisonAggregate {
                    surface: RetrievalSurface::CommunitySummary,
                    axis: RetrievalAxis::NdcgAtK,
                    paired_case_count: 4,
                    hybrid_mean: 0.61,
                    trigram_mean: 0.61,
                    mean_lift: 0.0,
                    win_rate_pct: 50.0,
                },
            ],
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["retrieval_comparisons"][0]["surface"], "verified_query");
        assert_eq!(v["retrieval_comparisons"][0]["axis"], "recall_at_k");
        assert_eq!(v["retrieval_comparisons"][0]["paired_case_count"], 4);
        assert_eq!(v["retrieval_comparisons"][0]["win_rate_pct"], 75.0);
        let back: RunSummary = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn retrieval_comparison_aggregate_round_trips() {
        let a = RetrievalComparisonAggregate {
            surface: RetrievalSurface::KnowledgeEntry,
            axis: RetrievalAxis::Mrr,
            paired_case_count: 8,
            hybrid_mean: 0.55,
            trigram_mean: 0.40,
            mean_lift: 0.15,
            win_rate_pct: 62.5,
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["surface"], "knowledge_entry");
        assert_eq!(v["axis"], "mrr");
        let back: RetrievalComparisonAggregate = serde_json::from_value(v).unwrap();
        assert_eq!(back, a);
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
            retrieval_comparison_deltas: vec![],
            retrieval_lift_regressions: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        // Empty deltas skip on the wire so reports without
        // retrieval_comparison cases stay tight.
        assert!(v.get("retrieval_comparison_deltas").is_none());
        let back: RunComparisonReport = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn run_comparison_report_emits_retrieval_deltas() {
        let r = RunComparisonReport {
            baseline_run_id: Uuid::new_v4(),
            candidate_run_id: Uuid::new_v4(),
            dataset_id: Uuid::new_v4(),
            per_case: vec![],
            per_axis: vec![],
            retrieval_comparison_deltas: vec![RetrievalComparisonDelta {
                surface: RetrievalSurface::VerifiedQuery,
                axis: RetrievalAxis::RecallAtK,
                baseline_lift: 0.10,
                candidate_lift: 0.18,
                lift_delta: 0.08,
                baseline_paired_case_count: 12,
                candidate_paired_case_count: 12,
            }],
            retrieval_lift_regressions: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(
            v["retrieval_comparison_deltas"][0]["surface"],
            "verified_query"
        );
        assert_eq!(v["retrieval_comparison_deltas"][0]["lift_delta"], 0.08);
        let back: RunComparisonReport = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn regression_threshold_predicate_pins_open_interval() {
        // Open interval (-1.0, 0.0) — endpoints rejected.
        assert!(is_valid_regression_threshold(-0.5));
        assert!(is_valid_regression_threshold(-0.001));
        assert!(!is_valid_regression_threshold(0.0));
        assert!(!is_valid_regression_threshold(-1.0));
        assert!(!is_valid_regression_threshold(0.5));
        assert!(!is_valid_regression_threshold(-1.5));
    }

    #[test]
    fn regression_min_paired_n_predicate_pins_at_least_two() {
        assert!(!is_valid_regression_min_paired_case_count(0));
        assert!(!is_valid_regression_min_paired_case_count(1));
        assert!(is_valid_regression_min_paired_case_count(2));
        assert!(is_valid_regression_min_paired_case_count(100));
    }

    #[test]
    fn workspace_evaluation_settings_default_matches_platform_constants() {
        let s = WorkspaceEvaluationSettings::default();
        assert_eq!(
            s.retrieval_lift_regression_threshold,
            RETRIEVAL_LIFT_REGRESSION_THRESHOLD
        );
        assert_eq!(
            s.retrieval_lift_regression_min_paired_case_count,
            RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N
        );
    }

    #[test]
    fn workspace_evaluation_settings_round_trip_with_overrides() {
        let s = WorkspaceEvaluationSettings {
            retrieval_lift_regression_threshold: -0.10,
            retrieval_lift_regression_min_paired_case_count: 5,
        };
        let v = serde_json::to_value(s).unwrap();
        assert_eq!(v["retrieval_lift_regression_threshold"], -0.10);
        assert_eq!(v["retrieval_lift_regression_min_paired_case_count"], 5);
        let back: WorkspaceEvaluationSettings = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn workspace_evaluation_settings_partial_payload_falls_back_to_defaults() {
        // Operator only persisted threshold; min-N field absent.
        // Serde default wires the platform constant in.
        let v = serde_json::json!({
            "retrieval_lift_regression_threshold": -0.07,
        });
        let s: WorkspaceEvaluationSettings = serde_json::from_value(v).unwrap();
        assert_eq!(s.retrieval_lift_regression_threshold, -0.07);
        assert_eq!(
            s.retrieval_lift_regression_min_paired_case_count,
            RETRIEVAL_LIFT_REGRESSION_MIN_PAIRED_N
        );
    }

    #[test]
    fn workspace_evaluation_settings_validate_rejects_positive_threshold() {
        let s = WorkspaceEvaluationSettings {
            retrieval_lift_regression_threshold: 0.05,
            retrieval_lift_regression_min_paired_case_count: 3,
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn workspace_evaluation_settings_validate_rejects_min_n_below_two() {
        let s = WorkspaceEvaluationSettings {
            retrieval_lift_regression_threshold: -0.05,
            retrieval_lift_regression_min_paired_case_count: 1,
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn workspace_evaluation_settings_regression_policy_threads_overrides() {
        let s = WorkspaceEvaluationSettings {
            retrieval_lift_regression_threshold: -0.10,
            retrieval_lift_regression_min_paired_case_count: 5,
        };
        let p = s.regression_policy();
        assert_eq!(p.threshold, -0.10);
        assert_eq!(p.min_paired_case_count, 5);
    }

    #[test]
    fn retrieval_lift_regression_alert_round_trips() {
        let alert = RetrievalLiftRegressionAlert {
            surface: RetrievalSurface::CommunitySummary,
            axis: RetrievalAxis::NdcgAtK,
            lift_delta: -0.08,
            baseline_lift: 0.18,
            candidate_lift: 0.10,
            threshold: RETRIEVAL_LIFT_REGRESSION_THRESHOLD,
            candidate_paired_case_count: 12,
        };
        let v = serde_json::to_value(&alert).unwrap();
        assert_eq!(v["surface"], "community_summary");
        assert_eq!(v["lift_delta"], -0.08);
        assert_eq!(v["threshold"], -0.05);
        let back: RetrievalLiftRegressionAlert = serde_json::from_value(v).unwrap();
        assert_eq!(back, alert);
    }


    #[test]
    fn retrieval_surface_all_covers_every_variant() {
        // Exhaustive match → if a new variant lands, this test
        // fails compilation until Self::ALL is updated. Acts as
        // a compile-time guard that the constant array stays
        // in lockstep with the enum.
        for s in RetrievalSurface::ALL.iter().copied() {
            match s {
                RetrievalSurface::VerifiedQuery
                | RetrievalSurface::CommunitySummary
                | RetrievalSurface::KnowledgeEntry => {}
            }
        }
        assert_eq!(RetrievalSurface::ALL.len(), 3);
    }

    #[test]
    fn retrieval_leg_all_covers_every_variant() {
        for l in RetrievalLeg::ALL.iter().copied() {
            match l {
                RetrievalLeg::Hybrid | RetrievalLeg::Trigram => {}
            }
        }
        assert_eq!(RetrievalLeg::ALL.len(), 2);
    }

    #[test]
    fn retrieval_axis_all_covers_every_variant() {
        for a in RetrievalAxis::ALL.iter().copied() {
            match a {
                RetrievalAxis::PrecisionAtK
                | RetrievalAxis::RecallAtK
                | RetrievalAxis::Mrr
                | RetrievalAxis::NdcgAtK => {}
            }
        }
        assert_eq!(RetrievalAxis::ALL.len(), 4);
    }

    #[test]
    fn retrieval_axis_wire_strings_match_metric_struct_field_names() {
        // The four axis wire strings must align with the field
        // names in `RetrievalMetrics` so the SQL pivot key (`axis`)
        // round-trips through both surfaces.
        let mut wires = RetrievalAxis::all_wire_strings();
        wires.sort();
        let mut fields = vec!["precision_at_k", "recall_at_k", "mrr", "ndcg_at_k"];
        fields.sort();
        assert_eq!(wires, fields);
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
