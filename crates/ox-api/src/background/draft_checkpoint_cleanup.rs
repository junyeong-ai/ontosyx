//! Daily draft-cluster-checkpoint cleanup.
//!
//! `design_ontology_batch`'s checkpoint cache is bounded by the
//! 24-hour `expires_at` on each row. A successful design drops its
//! own checkpoints (via `delete_draft_cluster_checkpoints_by_project`
//! in the streaming handler), but abandoned design sessions —
//! browser closed, user lost interest, transient failure not
//! retried — leave rows behind. This cron runs once a day under
//! `SYSTEM_BYPASS::scope` and DELETEs every row whose `expires_at`
//! is in the past, returning the cache to a clean baseline.
//!
//! Mirrors the `stale-concept-scan` shape: thin `CronTask` impl
//! delegating to a free `run_sweep` that's testable on its own.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use ox_store::Store;

use super::cron::{CronTask, spawn_cron};

/// Sweep cadence. Daily is enough — `expires_at` defaults to 24h
/// from row creation, so a day-cadence sweep means a stale row
/// lives at most ~48h before deletion.
const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

struct DraftCheckpointCleanup {
    store: Arc<dyn Store>,
}

#[async_trait]
impl CronTask for DraftCheckpointCleanup {
    fn name(&self) -> &'static str {
        "draft-checkpoint-cleanup"
    }

    fn interval(&self) -> Duration {
        SWEEP_INTERVAL
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_DRAFT_CHECKPOINT)
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        run_sweep(self.store.as_ref()).await
    }
}

pub fn spawn_draft_checkpoint_cleanup(
    store: Arc<dyn Store>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(Arc::new(DraftCheckpointCleanup { store }), Some(pool), cancel);
}

async fn run_sweep(store: &dyn Store) -> ox_core::error::OxResult<()> {
    // The store impl runs the DELETE under the bypass policy — no
    // need to wrap in `WORKSPACE_ID.scope`. The cron driver owns
    // the SYSTEM_BYPASS scope already.
    match store.sweep_expired_draft_cluster_checkpoints().await {
        Ok(deleted) => {
            if deleted > 0 {
                info!(
                    deleted,
                    "draft-checkpoint cleanup swept {deleted} expired rows"
                );
            } else {
                info!("draft-checkpoint cleanup: nothing to sweep");
            }
            Ok(())
        }
        Err(e) => {
            warn!(error = %e, "draft-checkpoint cleanup failed");
            Err(e)
        }
    }
}
