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

use std::time::Duration;

use chrono::Duration as ChronoDuration;
use tokio_util::sync::CancellationToken;
use tracing::info;

use ox_agent::clarification_tracker::{
    DEFAULT_WINDOW_MINUTES, SharedClarificationTracker,
};

/// Evict sweeps every 30 minutes. The default match window is 10
/// minutes; 30 leaves a 3× buffer so a query landing right at the
/// edge of the window still reads a live entry, and the map size
/// stabilises at "active sessions in the last three windows".
const EVICT_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Spawn the eviction loop under the system scope. Cancellation
/// token is shared with the rest of `main.rs`'s spawns so graceful
/// shutdown drains the loop before the process exits.
pub fn spawn_clarification_evict(
    tracker: SharedClarificationTracker,
    cancel: CancellationToken,
) {
    crate::spawn_scoped::spawn_system(async move {
        // `interval` fires its first tick immediately. We skip it
        // with `tick().await` up-front so the very first "evict"
        // isn't a no-op on a fresh process — evict at T+EVICT_INTERVAL
        // and every EVICT_INTERVAL after.
        let mut ticker = tokio::time::interval(EVICT_INTERVAL);
        // First tick is the zero tick — consume it so the loop
        // sleeps EVICT_INTERVAL before the real first sweep.
        ticker.tick().await;
        let window = ChronoDuration::minutes(DEFAULT_WINDOW_MINUTES);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("clarification-tracker evict shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    tracker.evict_older_than(window);
                    // Intentionally log at debug level — this runs
                    // frequently and a stable deploy shouldn't
                    // flood info logs. Callers grepping for
                    // the signal keyword still find it at DEBUG.
                    tracing::debug!("clarification-tracker evict complete");
                }
            }
        }
    });
}
