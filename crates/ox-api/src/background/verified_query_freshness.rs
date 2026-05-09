//! Verified-query freshness cron (Φ11.3).
//!
//! Walks each workspace's `verified_queries` (status =
//! `Verified`) against its committed canonical ontology version
//! and flips the row's status to `Stale` when the persisted
//! `QueryIR` references labels the active ontology no longer
//! declares.
//!
//! ## Why a cron, not a write-time hook
//!
//! Schema edits land via `OntologyVersionStore::commit_version`
//! across ~6 distinct call sites (admin route, schema-ops adopt,
//! draft completion, …). A write-time hook would have to
//! cross-reference every verified query against the new IR on
//! every commit — each commit pays linearly in verified-bank
//! size. A periodic sweep amortises that cost; the schema-drift
//! detection latency is bounded by the cron interval (default 1
//! hour), which is acceptable because verified queries are an
//! ICL-injection input, not a load-bearing query path —
//! "occasionally" using a stale exemplar is graceful (the LLM
//! can still produce a working IR), "permanently" using one
//! isn't.
//!
//! ## Singleton + workspace fan
//!
//! Singleton-locked via
//! [`ADVISORY_LOCK_CRON_VERIFIED_QUERY_FRESHNESS`] so two
//! replicas don't race on the same `transition_verified_query_status`
//! UPDATE. Cross-workspace fan-out via `list_workspace_ids` under
//! `SYSTEM_BYPASS`; per-workspace work runs inside
//! `WORKSPACE_ID.scope(ws_id, ...)` so the row reads + UPDATEs
//! land on the correct tenant.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use ox_ontology::{VerifiedQueryDef, VerifiedQueryStatus};
use ox_store::Store;

use super::cron::{CronTask, spawn_cron};

/// Default sweep interval. One hour is a sensible compromise:
/// schema drift detection is bounded; the cost (per-workspace IR
/// rehydrate + bank scan) stays low; and the dashboard's
/// freshness signal updates within the hour after a schema
/// commit.
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);

/// Per-workspace cap on verified queries scanned per tick. A
/// backlog (e.g. a workspace with > 1000 verified queries past
/// the bank's usual size) processes across multiple ticks
/// rather than burning a tick on one workspace. Set high enough
/// that no realistic workspace splits across two ticks; raise
/// when the operator surface needs it.
const PER_WORKSPACE_LIMIT: u32 = 1_000;

struct VerifiedQueryFreshnessSweep {
    store: Arc<dyn Store>,
}

#[async_trait]
impl CronTask for VerifiedQueryFreshnessSweep {
    fn name(&self) -> &'static str {
        "verified-query-freshness-sweep"
    }

    fn interval(&self) -> Duration {
        SWEEP_INTERVAL
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_VERIFIED_QUERY_FRESHNESS)
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        run_sweep(self.store.as_ref()).await
    }
}

pub fn spawn_verified_query_freshness_sweep(
    store: Arc<dyn Store>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(
        Arc::new(VerifiedQueryFreshnessSweep { store }),
        Some(pool),
        cancel,
    );
}

async fn run_sweep(store: &dyn Store) -> ox_core::error::OxResult<()> {
    let workspace_ids = store.list_workspace_ids().await?;
    if workspace_ids.is_empty() {
        return Ok(());
    }

    let mut workspaces_scanned = 0usize;
    let mut workspaces_skipped_no_canonical = 0usize;
    let mut total_verified = 0usize;
    let mut total_stale = 0usize;
    let mut total_errors = 0usize;

    for ws_id in workspace_ids {
        let outcome = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                sweep_workspace(store).await
            })
            .await;

        match outcome {
            Ok(WorkspaceSweepReport::NoCanonical) => {
                workspaces_skipped_no_canonical += 1;
            }
            Ok(WorkspaceSweepReport::Scanned {
                verified_count,
                staled_count,
                error_count,
            }) => {
                workspaces_scanned += 1;
                total_verified += verified_count;
                total_stale += staled_count;
                total_errors += error_count;
            }
            Err(e) => {
                warn!(
                    workspace_id = %ws_id,
                    error = %e,
                    "verified-query freshness sweep: workspace scan failed",
                );
                total_errors += 1;
            }
        }
    }

    if workspaces_scanned + workspaces_skipped_no_canonical + total_errors > 0 {
        info!(
            workspaces_scanned,
            workspaces_skipped_no_canonical,
            total_verified,
            total_stale,
            total_errors,
            "verified-query freshness sweep tick complete",
        );
    }
    Ok(())
}

enum WorkspaceSweepReport {
    /// Workspace has no committed canonical ontology version —
    /// greenfield workspace or pre-canonical state. Skip silently;
    /// nothing to validate against.
    NoCanonical,
    /// Workspace scanned. Counts surface in the per-tick log
    /// summary.
    Scanned {
        verified_count: usize,
        staled_count: usize,
        error_count: usize,
    },
}

async fn sweep_workspace(store: &dyn Store) -> ox_core::error::OxResult<WorkspaceSweepReport> {
    let Some(ontology) = store.get_workspace_ontology().await? else {
        return Ok(WorkspaceSweepReport::NoCanonical);
    };
    let Some(snapshot) = store.find_current_version(ontology.id).await? else {
        return Ok(WorkspaceSweepReport::NoCanonical);
    };
    let Some(ontology_ir) = store.get_ontology_ir(snapshot.id).await? else {
        return Ok(WorkspaceSweepReport::NoCanonical);
    };

    let verified = store
        .list_verified_queries(Some(VerifiedQueryStatus::Verified), PER_WORKSPACE_LIMIT)
        .await?;
    let verified_count = verified.len();
    let mut staled_count = 0usize;
    let mut error_count = 0usize;

    for vq in verified {
        match check_and_transition(store, &ontology_ir, &vq).await {
            Ok(true) => staled_count += 1,
            Ok(false) => {}
            Err(e) => {
                error_count += 1;
                warn!(
                    vq_id = %vq.id.as_str(),
                    error = %e,
                    "verified-query freshness: per-row check failed",
                );
            }
        }
    }

    Ok(WorkspaceSweepReport::Scanned {
        verified_count,
        staled_count,
        error_count,
    })
}

/// Returns `Ok(true)` when the row was transitioned to Stale,
/// `Ok(false)` when it stays `Verified` (every referenced label
/// resolves), `Err` on storage / deserialise failures.
async fn check_and_transition(
    store: &dyn Store,
    ontology_ir: &ox_ontology::ir::OntologyIR,
    vq: &VerifiedQueryDef,
) -> ox_core::error::OxResult<bool> {
    // The `query_ir` JSONB on the row deserialises into a typed
    // `QueryIR`. A persistent corruption (caller stored a
    // malformed IR) surfaces here as a runtime error; the
    // operator surface should flag it for re-promotion. We do
    // NOT auto-stale on deserialise failure — that's a different
    // class of problem (corrupted, not stale) and silently
    // burying it would hide the root cause.
    let query_ir: ox_query_ir::query::QueryIR =
        serde_json::from_value(vq.query_ir.clone()).map_err(|e| {
            ox_core::error::OxError::Runtime {
                message: format!(
                    "verified_queries.query_ir for {vq_id} failed to deserialise: {e}",
                    vq_id = vq.id.as_str()
                ),
            }
        })?;

    let unknown = ox_query_ir::unknown_labels_in_query(ontology_ir, &query_ir);
    if unknown.is_empty() {
        return Ok(false);
    }

    info!(
        vq_id = %vq.id.as_str(),
        unknown_labels = ?unknown,
        "verified-query freshness: marking stale (referenced labels unknown to active ontology)"
    );
    store
        .transition_verified_query_status(&vq.id, VerifiedQueryStatus::Stale)
        .await?;
    Ok(true)
}
