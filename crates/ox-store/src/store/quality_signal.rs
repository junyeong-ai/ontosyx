//! Signal log + aggregation for the "6 창" ontology-quality
//! dashboard. Sees every successful query (fire-and-forget) and
//! rolls the log into window-scoped metrics on demand.

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::quality_signal::{
    MetricWindow, QualityMetricsReport, QueryExecutionSignal, ShaclFailureCount, StaleTypeEntry,
};

#[async_trait]
pub trait QualitySignalStore: Send + Sync {
    /// Append a single query's signal row. Fire-and-forget —
    /// callers spawn this off the hot path and log write errors
    /// instead of propagating them.
    async fn create_query_execution_signal(&self, signal: &QueryExecutionSignal) -> OxResult<()>;

    /// Aggregate the six dashboard metrics for the current
    /// workspace over `window`. Returns Wilson-score bands plus
    /// trend deltas against the immediately-previous window of the
    /// same length.
    async fn aggregate_quality_metrics(
        &self,
        window: MetricWindow,
    ) -> OxResult<QualityMetricsReport>;

    /// Grouped SHACL-failure distribution for the "실패 유형 분포"
    /// chart over `window`. Returns one row per observed
    /// `ShaclFailureKind`, zero rows when no failures recorded.
    async fn list_shacl_failure_distribution(
        &self,
        window: MetricWindow,
    ) -> OxResult<Vec<ShaclFailureCount>>;

    /// Upsert "last used" timestamps + rolling 7/30-day counts for
    /// every type in `type_ids`. Called from the signal write path
    /// so the stale scan doesn't have to rescan signal history.
    async fn upsert_type_last_used(&self, type_ids: &[(uuid::Uuid, &str)]) -> OxResult<()>;

    /// List types whose `last_used_at` is older than
    /// `stale_after_days` for the current workspace, sorted by
    /// `last_used_at` ascending (staleest first).
    async fn list_stale_types(&self, stale_after_days: i64) -> OxResult<Vec<StaleTypeEntry>>;
}
