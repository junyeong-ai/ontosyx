//! Workspace-level quality-metric baselines for adaptive alert
//! thresholds. Populated nightly by the `quality_baseline` cron
//! from [`super::QualitySignalStore::aggregate_quality_metrics`]
//! rollups; the banner consults it at render time when the
//! adaptive path lights up.

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::quality_signal::WorkspaceQualityBaseline;

#[async_trait]
pub trait QualityBaselineStore: Send + Sync {
    /// Upsert the current-workspace baseline row. Cron calls this
    /// once per workspace per day; upsert-in-place means consumers
    /// always read the latest snapshot without a window-picking
    /// predicate.
    async fn upsert_quality_baseline(
        &self,
        baseline: &WorkspaceQualityBaseline,
    ) -> OxResult<()>;

    /// Fetch the current-workspace baseline, if any. `None` means
    /// the cron hasn't run yet (fresh workspace / first boot);
    /// the banner falls back to its hardcoded prior in that case.
    async fn get_quality_baseline(&self) -> OxResult<Option<WorkspaceQualityBaseline>>;
}
