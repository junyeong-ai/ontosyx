//! Daily quality-baseline cron.
//!
//! Every workspace gets a per-metric `median ± k·MAD` snapshot
//! written to `workspace_quality_baseline` so the banner can move
//! off its hardcoded prior onto workspace-specific thresholds
//! (Phase B wiring). Running the cron from day one means Phase B
//! has a real warm-up window to validate against instead of a
//! cold start.
//!
//! The scan mirrors `stale_concepts`: `spawn_system` so
//! `SYSTEM_BYPASS` is set, `list_workspace_ids` fans out per
//! tenant, each workspace runs inside `WORKSPACE_ID.scope` so
//! `aggregate_quality_metrics` (which reads RLS-scoped signals)
//! and the `upsert_quality_baseline` INSERT both land on the
//! right tenant.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use ox_store::{MetricValue, MetricWindow, QualityMetricsReport, Store, WorkspaceQualityBaseline};

use super::cron::{CronTask, spawn_cron};

/// How often the cron runs. Daily is the natural granularity — the
/// metrics settle over days, and a 24h recomputation is invisible
/// to the banner which renders against whichever row is current.
const SCAN_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Window the cron summarizes each run. 30 days balances "enough
/// samples to drive MAD" with "sensitive enough to notice real
/// drift". Phase B can expose this as a tunable.
const BASELINE_WINDOW: MetricWindow = MetricWindow::Last30d;

/// Multiplier for the warn / critical derivation from MAD. Tuning
/// lives here so Phase B can expose it as system_config without
/// re-reading the cron body.
const WARN_MAD_MULTIPLIER: f64 = 2.0;
const CRITICAL_MAD_MULTIPLIER: f64 = 3.0;

/// Cron impl surface. Every field the scan needs lives here so
/// `run_once` is a thin delegate to [`run_scan`].
struct QualityBaselineScan {
    store: Arc<dyn Store>,
}

#[async_trait]
impl CronTask for QualityBaselineScan {
    fn name(&self) -> &'static str {
        "quality-baseline-scan"
    }

    fn interval(&self) -> Duration {
        SCAN_INTERVAL
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_QUALITY_BASELINE)
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        run_scan(self.store.as_ref()).await
    }
}

pub fn spawn_quality_baseline_scan(
    store: Arc<dyn Store>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(Arc::new(QualityBaselineScan { store }), Some(pool), cancel);
}

async fn run_scan(store: &dyn Store) -> ox_core::error::OxResult<()> {
    let workspaces = store.list_workspace_ids().await?;
    if workspaces.is_empty() {
        info!("quality-baseline scan: no workspaces");
        return Ok(());
    }

    let mut touched = 0usize;
    for ws_id in workspaces {
        let wrote = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                match store.aggregate_quality_metrics(BASELINE_WINDOW).await {
                    Ok(report) => {
                        let thresholds = derive_thresholds(&report);
                        let baseline = WorkspaceQualityBaseline {
                            workspace_id: ws_id,
                            window_label: BASELINE_WINDOW.as_str().to_string(),
                            sample_size: report.sample_size as i64,
                            thresholds,
                            computed_at: Utc::now(),
                        };
                        match store.upsert_quality_baseline(&baseline).await {
                            Ok(()) => true,
                            Err(e) => {
                                warn!(workspace = %ws_id, error = %e, "baseline upsert failed");
                                false
                            }
                        }
                    }
                    Err(e) => {
                        warn!(workspace = %ws_id, error = %e, "aggregate_quality_metrics failed");
                        false
                    }
                }
            })
            .await;
        if wrote {
            touched += 1;
        }
    }

    info!(
        workspaces = touched,
        window = BASELINE_WINDOW.as_str(),
        "quality-baseline scan complete"
    );
    Ok(())
}

/// Derive the `{ metric_key: { median, mad, warn, critical } }`
/// JSONB payload from a metrics report.
///
/// For "higher is better" metrics (pass rate, reproducibility,
/// anchor match, glossary hit, clarification success) the warn /
/// critical lines sit *below* the median (drift is downward). For
/// "lower is better" (stale concept ratio) they sit *above* —
/// conceptually symmetric, opposite sign.
///
/// MAD is proxied by `(upper_bound_95 - lower_bound_95) / 2`
/// from the Wilson interval already on `MetricValue`; a genuine
/// MAD would need raw sample access, but the Wilson half-width
/// tracks MAD closely for the proportion metrics we publish and
/// saves re-scanning the signal log. The SI-symmetric derivation
/// means future swap to raw-sample MAD is a one-line change.
fn derive_thresholds(report: &QualityMetricsReport) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "shacl_pass_rate".into(),
        threshold_higher_is_better(&report.shacl_pass_rate),
    );
    out.insert(
        "query_reproducibility".into(),
        threshold_higher_is_better(&report.query_reproducibility),
    );
    out.insert(
        "anchor_match_rate".into(),
        threshold_higher_is_better(&report.anchor_match_rate),
    );
    out.insert(
        "concept_hit_rate".into(),
        threshold_higher_is_better(&report.concept_hit_rate),
    );
    out.insert(
        "clarification_success_rate".into(),
        threshold_higher_is_better(&report.clarification_success_rate),
    );
    out.insert(
        "stale_concept_ratio".into(),
        threshold_lower_is_better(&report.stale_concept_ratio),
    );
    serde_json::Value::Object(out)
}

fn mad_proxy(metric: &MetricValue) -> f64 {
    ((metric.upper_bound_95 - metric.lower_bound_95) / 2.0).max(0.0)
}

fn threshold_higher_is_better(metric: &MetricValue) -> serde_json::Value {
    let mad = mad_proxy(metric);
    json!({
        "median": metric.value,
        "mad": mad,
        "warn": (metric.value - WARN_MAD_MULTIPLIER * mad).clamp(0.0, 1.0),
        "critical": (metric.value - CRITICAL_MAD_MULTIPLIER * mad).clamp(0.0, 1.0),
    })
}

fn threshold_lower_is_better(metric: &MetricValue) -> serde_json::Value {
    let mad = mad_proxy(metric);
    json!({
        "median": metric.value,
        "mad": mad,
        "warn": (metric.value + WARN_MAD_MULTIPLIER * mad).clamp(0.0, 1.0),
        "critical": (metric.value + CRITICAL_MAD_MULTIPLIER * mad).clamp(0.0, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(value: f64, lower: f64, upper: f64) -> MetricValue {
        MetricValue {
            value,
            trend_delta: 0.0,
            lower_bound_95: lower,
            upper_bound_95: upper,
        }
    }

    #[test]
    fn higher_is_better_drops_warn_below_median() {
        let m = metric(0.9, 0.85, 0.95);
        let t = threshold_higher_is_better(&m);
        let median = t["median"].as_f64().unwrap();
        let warn = t["warn"].as_f64().unwrap();
        let critical = t["critical"].as_f64().unwrap();
        assert_eq!(median, 0.9);
        assert!(warn < median);
        assert!(critical < warn);
    }

    #[test]
    fn lower_is_better_raises_warn_above_median() {
        let m = metric(0.1, 0.05, 0.15);
        let t = threshold_lower_is_better(&m);
        let median = t["median"].as_f64().unwrap();
        let warn = t["warn"].as_f64().unwrap();
        let critical = t["critical"].as_f64().unwrap();
        assert_eq!(median, 0.1);
        assert!(warn > median);
        assert!(critical > warn);
    }

    #[test]
    fn thresholds_clamp_to_unit_interval() {
        // Wide band at the low edge — naive subtraction would dip
        // below zero; the clamp keeps thresholds inside [0, 1].
        let m = metric(0.05, 0.0, 0.5);
        let t = threshold_higher_is_better(&m);
        assert!(t["warn"].as_f64().unwrap() >= 0.0);
        assert!(t["critical"].as_f64().unwrap() >= 0.0);
    }

    #[test]
    fn mad_proxy_never_negative_for_degenerate_interval() {
        // lower_bound > upper_bound is a data bug, but the helper
        // must stay non-negative to avoid polluting downstream
        // arithmetic. `.max(0.0)` enforces the invariant.
        let m = metric(0.5, 0.6, 0.4);
        assert!(mad_proxy(&m) >= 0.0);
    }
}
