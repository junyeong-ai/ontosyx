//! Periodic evict for the process-wide [`ClarificationTracker`].
//!
//! Phase 4.6 wired the tracker so resolve_ambiguity → query_graph
//! in the same session flips `ambiguity_was_clarified = true` on
//! the quality signal. Entries live in a `DashMap<session_id,
//! DateTime<Utc>>`; without a sweep, sessions that started a
//! resolve but then silently dropped (network failure, user
//! walked away, long-running agent) would keep their stamp
//! forever.
//!
//! The tracker's own `evict_older_than(window)` already does the
//! filtering; this module is the thin cron that calls it on an
//! interval matching the lookback window. A purge two windows old
//! costs nothing (every query compares to "now"), but keeps the
//! map size bounded in a churn-heavy deployment.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Duration as ChronoDuration;
use tokio_util::sync::CancellationToken;

use ox_agent::clarification_tracker::{DEFAULT_WINDOW_MINUTES, SharedClarificationTracker};

use super::cron::{CronTask, spawn_cron};

/// Evict sweeps every 30 minutes. The default match window is 10
/// minutes; 30 leaves a 3× buffer so a query landing right at the
/// edge of the window still reads a live entry, and the map size
/// stabilises at "active sessions in the last three windows".
const EVICT_INTERVAL: Duration = Duration::from_secs(30 * 60);

struct ClarificationEvict {
    tracker: SharedClarificationTracker,
}

#[async_trait]
impl CronTask for ClarificationEvict {
    fn name(&self) -> &'static str {
        "clarification-tracker-evict"
    }

    fn interval(&self) -> Duration {
        EVICT_INTERVAL
    }

    /// Unlike the stale-concept scan, an evict on a just-booted
    /// tracker has nothing to do — everything in the DashMap is
    /// fresh by definition. Skipping the immediate tick avoids a
    /// no-op log line per restart.
    fn fire_on_start(&self) -> bool {
        false
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        let window = ChronoDuration::minutes(DEFAULT_WINDOW_MINUTES);
        self.tracker.evict_older_than(window);
        // DEBUG rather than INFO — this runs every 30 minutes on a
        // healthy deploy and a stable process shouldn't flood info
        // logs with routine sweeps.
        tracing::debug!(cron = "clarification-tracker-evict", "evict complete");
        Ok(())
    }
}

/// Spawn the eviction loop. Cancellation token is shared with the
/// rest of `main.rs`'s spawns so graceful shutdown drains the loop
/// before the process exits.
pub fn spawn_clarification_evict(
    tracker: SharedClarificationTracker,
    cancel: CancellationToken,
) {
    spawn_cron(Arc::new(ClarificationEvict { tracker }), cancel);
}
