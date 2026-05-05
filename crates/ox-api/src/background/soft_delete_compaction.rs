//! Daily retention cron for the graph's soft-delete tombstones.
//!
//! Soft-deleted nodes carry a `_deleted_at` timestamp set by
//! [`ox_runtime::cypher::SoftDeleteRewriter`] when a `DELETE` /
//! `DETACH DELETE` runs on a non-bypass request. Retention compacts
//! every node whose tombstone is older than the configured cutoff —
//! a single graph-wide `DETACH DELETE` issued under
//! `GRAPH_SYSTEM_BYPASS=true` so the rewriter passes the destructive
//! statement through verbatim instead of re-soft-deleting it.
//!
//! Audit trail: emit one INFO log per scan with the deleted-row
//! count + cutoff. The graph runtime's own audit middleware covers
//! per-statement provenance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use ox_core::error::OxResult;
use ox_core::types::PropertyValue;
use ox_runtime::GraphRuntime;

use super::cron::{CronTask, spawn_cron};

const SCAN_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Default retention period in days. The retention compactor
/// hard-deletes any node whose `_deleted_at` is older than this.
/// Overridable per workspace once a `system_config` row lands; the
/// default lives here so a fresh deploy has a working policy from
/// boot.
const DEFAULT_RETENTION_DAYS: i64 = 90;

const COMPACTION_CYPHER: &str = "MATCH (n) WHERE n._deleted_at IS NOT NULL AND \
    n._deleted_at < $cutoff_ms DETACH DELETE n";

struct SoftDeleteCompaction {
    runtime: Arc<dyn GraphRuntime>,
    retention_days: i64,
}

#[async_trait]
impl CronTask for SoftDeleteCompaction {
    fn name(&self) -> &'static str {
        "soft-delete-compaction"
    }

    fn interval(&self) -> Duration {
        SCAN_INTERVAL
    }

    fn fire_on_start(&self) -> bool {
        // A fresh deploy has no tombstones to compact — letting the
        // first tick fire after one full interval avoids a spurious
        // graph round-trip on every server start.
        false
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_SOFT_DELETE)
    }

    async fn run_once(&self) -> OxResult<()> {
        let cutoff_ms = current_cutoff_ms(self.retention_days);
        let mut params = HashMap::new();
        params.insert(
            "cutoff_ms".to_string(),
            PropertyValue::Int(cutoff_ms),
        );

        match self.runtime.execute_query(COMPACTION_CYPHER, &params).await {
            Ok(result) => {
                info!(
                    retention_days = self.retention_days,
                    rows = result.metadata.rows_returned,
                    "soft-delete compaction complete"
                );
                Ok(())
            }
            Err(e) => {
                warn!(error = %e, "soft-delete compaction failed");
                Err(e)
            }
        }
    }
}

/// `Instant`-style "now minus retention" rendered as the same
/// millisecond unit `SoftDeleteRewriter` writes via `timestamp()`.
fn current_cutoff_ms(retention_days: i64) -> i64 {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let retention_ms = retention_days * 86_400_000;
    now_ms - retention_ms
}

pub fn spawn_soft_delete_compaction(
    runtime: Arc<dyn GraphRuntime>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(
        Arc::new(SoftDeleteCompaction {
            runtime,
            retention_days: DEFAULT_RETENTION_DAYS,
        }),
        Some(pool),
        cancel,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_ms_subtracts_full_retention_window() {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let cutoff = current_cutoff_ms(90);
        let elapsed = now_ms - cutoff;
        let expected = 90i64 * 86_400_000;
        // Allow a few ms drift between `chrono::Utc::now` calls.
        assert!(
            (elapsed - expected).abs() < 100,
            "elapsed {elapsed} ms, expected ~{expected}"
        );
    }

    #[test]
    fn cutoff_ms_handles_zero_retention() {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let cutoff = current_cutoff_ms(0);
        assert!((cutoff - now_ms).abs() < 100);
    }
}
