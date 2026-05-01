//! Per-query signal records + aggregated metric shapes that feed the
//! patent's "6 창" (six-window) quality dashboard.
//!
//! Where `QualityStore` measures *data* quality (null ratios, FK
//! coverage, freshness) on ontology types, **this** module measures
//! *ontology* quality — does the LLM actually find what it needs when
//! users ask questions?
//!
//! Signal sources:
//!
//! - **anchor_top_score** — the highest-blended score returned by
//!   [`crate::OntologyNavigationStore::search_entry_points`] for the
//!   user's question. Low score ⇒ ontology under-indexed for this
//!   phrasing.
//! - **glossary_term_hits** — which glossary terms the agent
//!   actually consulted while translating NL→Cypher.
//! - **ambiguity_resolution_ids** — which ambiguity resolutions the
//!   query path applied.
//! - **shacl_passed / shacl_failure_kind** — `ShaclValidator` verdict.
//! - **query_ir_normalized_hash** — deterministic hash of the
//!   QueryIR with timestamps and auto-generated ids stripped;
//!   anchors the "쿼리 재현성" (query reproducibility) metric.
//! - **referenced_type_ids** — every NodeType / EdgeType / PropertyDef
//!   the compiled query touched; the basis for stale-concept scan.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One row per successful query execution. Persisted via
/// [`crate::QualitySignalStore::create_query_execution_signal`] as
/// fire-and-forget from the query-path hot loop — a write failure is
/// logged and dropped, so a signal-store outage never pokes a
/// successful user query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExecutionSignal {
    pub execution_id: Uuid,
    pub workspace_id: Uuid,
    pub captured_at: DateTime<Utc>,

    // Anchor layer — the entry-point hit that seeded Progressive Disclosure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_top_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor_hit_kinds: Vec<String>,

    // Glossary layer — which business terms the agent used to resolve the question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glossary_term_hits: Vec<Uuid>,

    // Ambiguity layer — closed-loop resolver use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambiguity_resolution_ids: Vec<Uuid>,
    /// Did the query follow a clarification (`resolve_ambiguity` tool)?
    /// Feeds the "되묻기 이후 성공률" metric.
    #[serde(default)]
    pub ambiguity_was_clarified: bool,

    // SHACL layer — did the ontology-rule validator accept the compiled query?
    pub shacl_passed: bool,
    /// Named failure category when `shacl_passed == false`. `None` on
    /// success. Enum wire form: snake_case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shacl_failure_kind: Option<ShaclFailureKind>,

    // Reproducibility layer — sha256 of the QueryIR with volatile
    // fields stripped. Two queries with the same hash produced the
    // same plan even if auto-ids or timestamps differ.
    pub query_ir_normalized_hash: String,

    // Stale-concept layer — the exact types the query touched.
    // Written after the compiler emits plan metadata (which already
    // collects `type_ids` via the provenance trail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub referenced_type_ids: Vec<Uuid>,
}

/// Typed SHACL failure kind. The taxonomy is deliberately small —
/// every UI filter / telemetry gauge keys off a finite set, not a
/// freeform reason string. Unknown causes ⇒ Other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaclFailureKind {
    /// Edge traversed with an incompatible cardinality (ManyToMany
    /// missing DISTINCT, or One-sided aggregation blown up).
    CardinalityViolation,
    /// A Measure property was pulled into GROUP BY.
    MeasureGroupBy,
    /// The query referenced a CodedValue that doesn't exist.
    UnknownCodedValue,
    /// A NOT-NULL / min_count property was omitted on CREATE/MERGE.
    MandatoryPropertyMissing,
    /// Temporal grain (year/month/day) mismatch between filters.
    TemporalGrainMismatch,
    /// Any failure that doesn't fit the named buckets above.
    Other,
}

/// Rolling time window for aggregation. Expressed as whole days so
/// the SQL side is straightforward `captured_at > now() - interval
/// '{n} days'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricWindow {
    Last7d,
    Last30d,
    Last90d,
}

impl MetricWindow {
    pub fn as_days(self) -> i64 {
        match self {
            Self::Last7d => 7,
            Self::Last30d => 30,
            Self::Last90d => 90,
        }
    }

    /// Stable short-form (`"7d"` / `"30d"` / `"90d"`) used by the
    /// `workspace_quality_baseline.window` text column and the API
    /// query-string. Kept as an explicit match so the wire format
    /// can't drift if a new variant is added without a code review.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Last7d => "7d",
            Self::Last30d => "30d",
            Self::Last90d => "90d",
        }
    }

    /// Previous-window pair for trend calculation. `Last7d` pairs
    /// against the 7 days BEFORE the current window (days -14..-7).
    pub fn previous_as_days_range(self) -> (i64, i64) {
        let d = self.as_days();
        (d * 2, d)
    }
}

/// Single metric cell in the dashboard report. Bound fields carry a
/// 95% Wilson score interval so tiny samples read as "noisy" in the
/// UI instead of false-positives.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub trend_delta: f64,
    pub lower_bound_95: f64,
    pub upper_bound_95: f64,
}

impl MetricValue {
    /// Zero-sample dashboard tile — renders as "—" on the UI.
    pub fn empty() -> Self {
        Self {
            value: 0.0,
            trend_delta: 0.0,
            lower_bound_95: 0.0,
            upper_bound_95: 0.0,
        }
    }

    /// Wilson score interval for a Bernoulli proportion. Small
    /// sample sizes (`total < 30`) give wide bounds — the UI treats
    /// a wide band as "not enough data yet" without special-casing
    /// in the backend.
    ///
    /// `k` = successes, `total` = trials. Returns `empty()` when
    /// `total == 0` so callers never divide by zero.
    pub fn wilson_proportion(k: u64, total: u64, previous_value: f64) -> Self {
        if total == 0 {
            return Self::empty();
        }
        let n = total as f64;
        let p = (k as f64) / n;
        // z = 1.96 for 95% CI.
        let z: f64 = 1.96;
        let denom = 1.0 + z * z / n;
        let center = (p + z * z / (2.0 * n)) / denom;
        let margin = z * ((p * (1.0 - p) / n) + z * z / (4.0 * n * n)).sqrt() / denom;
        Self {
            value: p,
            trend_delta: p - previous_value,
            lower_bound_95: (center - margin).max(0.0),
            upper_bound_95: (center + margin).min(1.0),
        }
    }
}

/// Aggregated dashboard report — one row, six tiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetricsReport {
    pub anchor_match_rate: MetricValue,
    pub glossary_hit_rate: MetricValue,
    pub clarification_success_rate: MetricValue,
    pub query_reproducibility: MetricValue,
    pub shacl_pass_rate: MetricValue,
    pub stale_concept_ratio: MetricValue,
    pub sample_size: u64,
    pub window: MetricWindow,
}

/// SHACL failure distribution row for the "실패 유형 분포" chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaclFailureCount {
    pub kind: ShaclFailureKind,
    pub count: u64,
}

/// Stale-type scan result. `last_used_at == None` means the type
/// has never been referenced since the tracker started recording —
/// candidate for deprecation after admin review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleTypeEntry {
    pub workspace_id: Uuid,
    pub type_id: Uuid,
    pub type_kind: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub days_since_last_use: i64,
}

// ---------------------------------------------------------------------------
// StaleConceptProposal — persistent actionable row
//
// The cron writes one row per stale type; the admin UI decides
// approve / dismiss. Natural key is `(workspace_id, type_id)` so
// the cron is idempotent and a superseded decision can re-emerge
// after the row is cleared.
// ---------------------------------------------------------------------------

/// Admin decision on a stale-concept proposal. `Pending` is the
/// default; the two terminal states carry `decided_at` + optional
/// `reason` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleProposalDecision {
    Pending,
    Approved,
    Dismissed,
}

impl StaleProposalDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn try_from_db(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

/// One durable deprecation proposal. `decided_at` + `decided_by_user_id`
/// are `None` on fresh rows; decisions land via
/// [`crate::StaleConceptProposalStore::record_decision`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleConceptProposal {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub type_id: Uuid,
    pub type_kind: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub days_since_last_use: i64,
    pub proposed_at: DateTime<Utc>,
    pub decision: StaleProposalDecision,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by_user_id: Option<Uuid>,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// WorkspaceQualityBaseline — per-workspace adaptive threshold snapshot
// ---------------------------------------------------------------------------

/// Nightly snapshot of a workspace's quality-metric median + MAD
/// rollup, shaped so the banner's alert engine can swap from the
/// hardcoded prior to workspace-specific thresholds at render time
/// (Phase B).
///
/// `thresholds` is a JSONB map keyed by metric name
/// (`shacl_pass_rate`, `query_reproducibility`, …); each value
/// carries `{ median, mad, warn, critical }`. Keeping the shape
/// JSON means adding a new metric (e.g. Phase C `anchor_top_score`)
/// requires only a cron-computation extension, not a migration.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceQualityBaseline {
    pub workspace_id: Uuid,
    /// `"7d"` / `"30d"` / `"90d"` — the window the cron summarised.
    /// Named `window_label` (not `window`) because `WINDOW` is a
    /// reserved keyword in PostgreSQL and using it bare as a
    /// column name trips the DDL parser.
    pub window_label: String,
    /// Sample count that fed the computation. The banner treats
    /// baselines below its `MIN_SAMPLE_SIZE` as insufficient
    /// evidence and falls back to the hardcoded prior.
    pub sample_size: i64,
    /// `{ metric_key: { median, mad, warn, critical } }` bundle.
    pub thresholds: serde_json::Value,
    pub computed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_proportion_zero_sample_is_empty() {
        let v = MetricValue::wilson_proportion(0, 0, 0.0);
        assert_eq!(v.value, 0.0);
        assert_eq!(v.lower_bound_95, 0.0);
        assert_eq!(v.upper_bound_95, 0.0);
    }

    #[test]
    fn wilson_proportion_centres_on_p_with_large_sample() {
        // 800/1000 = 0.8; the 95% band must straddle 0.8 closely.
        let v = MetricValue::wilson_proportion(800, 1000, 0.79);
        assert!((v.value - 0.8).abs() < 1e-9);
        assert!(v.lower_bound_95 > 0.77);
        assert!(v.upper_bound_95 < 0.83);
        assert!((v.trend_delta - 0.01).abs() < 1e-9);
    }

    #[test]
    fn wilson_proportion_small_sample_gives_wide_bounds() {
        // 4/5 — wide band signals "not enough data yet".
        let v = MetricValue::wilson_proportion(4, 5, 0.0);
        assert!((v.value - 0.8).abs() < 1e-9);
        assert!(v.upper_bound_95 - v.lower_bound_95 > 0.4);
    }

    #[test]
    fn metric_window_previous_range_is_consecutive() {
        // Current 7-day window ⇒ previous is 14..7 days ago. Callers
        // build the SQL `captured_at < now() - INTERVAL '7 days' AND
        // captured_at > now() - INTERVAL '14 days'`.
        let (older, newer) = MetricWindow::Last7d.previous_as_days_range();
        assert_eq!(older, 14);
        assert_eq!(newer, 7);
    }

    #[test]
    fn shacl_failure_kind_serialises_as_snake_case() {
        let k = ShaclFailureKind::MeasureGroupBy;
        let j = serde_json::to_string(&k).unwrap();
        assert_eq!(j, "\"measure_group_by\"");
    }
}
