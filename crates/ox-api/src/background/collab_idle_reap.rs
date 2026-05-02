//! Periodic reap for the collaboration hub.
//!
//! The WS handler's cleanup loop fires when the browser sends a
//! `Close` frame — but a hung tab, killed renderer, or NAT reset
//! never gets that far, leaving ghost presence in the room. The
//! sweep here calls [`CollaborationHub::reap_idle_members`] on an
//! interval shorter than `idle_timeout`, so other members see the
//! `UserLeft` and presence converges without operator action.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::collaboration::CollaborationHub;

use super::cron::{CronTask, spawn_cron};

struct CollabIdleReap {
    hub: Arc<CollaborationHub>,
    interval: Duration,
}

#[async_trait]
impl CronTask for CollabIdleReap {
    fn name(&self) -> &'static str {
        "collab-idle-reap"
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    /// A just-booted hub has no rooms, so the first tick has
    /// nothing to do. Skipping the immediate run avoids a no-op
    /// log line on every restart.
    fn fire_on_start(&self) -> bool {
        false
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        let reaped = self.hub.reap_idle_members().await;
        if reaped > 0 {
            tracing::info!(
                cron = "collab-idle-reap",
                reaped,
                "reaped idle collaboration members",
            );
        } else {
            tracing::debug!(cron = "collab-idle-reap", "no idle members");
        }
        Ok(())
    }
}

/// Spawn the reap loop. The interval is read from
/// `OxConfig.collaboration.reap_interval_secs`; the cancellation
/// token is shared with `main.rs`'s other spawns so graceful
/// shutdown drains the loop.
pub fn spawn_collab_idle_reap(
    hub: Arc<CollaborationHub>,
    interval: Duration,
    cancel: CancellationToken,
) {
    spawn_cron(Arc::new(CollabIdleReap { hub, interval }), cancel);
}
