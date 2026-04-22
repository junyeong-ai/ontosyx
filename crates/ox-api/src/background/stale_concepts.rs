//! Daily stale-concept cron.
//!
//! Scans `ontology_type_last_used` across every workspace (under
//! `SYSTEM_BYPASS`), groups hits by workspace, and upserts one
//! `StaleConceptProposal` per stale type. Existing proposals are
//! idempotent-no-op via the table's natural key
//! (`workspace_id, type_id`).
//!
//! The cron never auto-deprecates — it only writes a proposal row.
//! Admin UI flips `decision` to `approved` / `dismissed`; the
//! deprecation edit itself rides `OntologyEditOp::DeleteCodeSystem`
//! / `DeprecateGlossaryTerm` through the Phase 6 routing path.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use ox_store::{StaleConceptProposal, StaleProposalDecision, Store};

/// How often the cron scans. Staleness cutoffs measure in months, so
/// daily granularity is enough — a proposal taking 24h to appear is
/// indistinguishable from "yesterday's scan already picked it up".
const SCAN_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Default staleness cutoff in days. Matches the patent matrix's
/// "6개월 미사용" threshold; a workspace can override this later via
/// system_config.
const DEFAULT_STALE_AFTER_DAYS: i64 = 180;

pub fn spawn_stale_concept_scan(
    store: Arc<dyn Store>,
    cancel: CancellationToken,
) {
    crate::spawn_scoped::spawn_system(async move {
        // First tick fires immediately — running the scan on startup
        // makes the new-deploy case ("just-installed cluster has old
        // ontologies") surface proposals without a 24h wait.
        let mut ticker = tokio::time::interval(SCAN_INTERVAL);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("stale-concept scan shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    if let Err(e) = run_scan(store.as_ref(), DEFAULT_STALE_AFTER_DAYS).await {
                        warn!(error = %e, "stale-concept scan failed");
                    }
                }
            }
        }
    });
}

async fn run_scan(store: &dyn Store, stale_after_days: i64) -> ox_core::error::OxResult<()> {
    // `list_stale_types` runs under the current pool scope — we're
    // inside `spawn_system` so SYSTEM_BYPASS is set and the query
    // returns rows for every workspace. The struct carries
    // `workspace_id`, so per-workspace grouping happens in-process.
    let rows = store.list_stale_types(stale_after_days).await?;
    if rows.is_empty() {
        info!(days = stale_after_days, "stale-concept scan: nothing to propose");
        return Ok(());
    }

    // Group by workspace so each batch can run inside a single
    // `WORKSPACE_ID.scope` — that way the upsert's
    // `current_setting('app.workspace_id')` lands on the right tenant
    // instead of NULL (which it would under bare SYSTEM_BYPASS).
    let mut by_ws: HashMap<Uuid, Vec<_>> = HashMap::new();
    for row in rows {
        by_ws.entry(row.workspace_id).or_default().push(row);
    }

    let mut inserted = 0usize;
    let mut workspaces_touched = 0usize;
    for (ws_id, entries) in by_ws {
        workspaces_touched += 1;
        let ws_inserted = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                let mut count = 0usize;
                for entry in entries {
                    let proposal = StaleConceptProposal {
                        id: Uuid::new_v4(),
                        workspace_id: ws_id,
                        type_id: entry.type_id,
                        type_kind: entry.type_kind,
                        last_used_at: entry.last_used_at,
                        days_since_last_use: entry.days_since_last_use,
                        proposed_at: Utc::now(),
                        decision: StaleProposalDecision::Pending,
                        decided_at: None,
                        decided_by_user_id: None,
                        reason: None,
                    };
                    if let Err(e) = store.upsert_stale_concept_proposal(proposal).await {
                        warn!(workspace = %ws_id, error = %e, "stale proposal upsert failed");
                    } else {
                        count += 1;
                    }
                }
                count
            })
            .await;
        inserted += ws_inserted;
    }

    info!(
        workspaces = workspaces_touched,
        proposed = inserted,
        days = stale_after_days,
        "stale-concept scan complete"
    );
    Ok(())
}
