//! PROV-O activity persistence.
//!
//! Backs `provenance_records` — the workspace-scoped audit DAG of
//! every fact-producing mutation. Producers (commit_version, eval
//! judge, action execute, …) call [`ProvenanceStore::record_activity`]
//! with a [`ProvenanceCapture`] + the subject they just produced;
//! the trait stamps a fresh id and returns the typed
//! [`ProvenanceId`] for downstream FK pinning.
//!
//! Reads — `get_provenance_record` for single lookup, listing /
//! filtering live on the audit-trail surface (`AuditTrailStore`)
//! that walks the audit + provenance + verification trio together.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;
use ox_ontology::{EntityRef, ProvenanceCapture, ProvenanceDef, ProvenanceId};

#[async_trait]
pub trait ProvenanceStore: Send + Sync {
    /// Persist a `ProvenanceCapture` against the supplied `subject`,
    /// stamping a fresh id + `at_time`. Returns the persisted id —
    /// callers thread it into the FK column on the row whose
    /// production they are recording (`ontology_version_snapshots
    /// .provenance_id`, `evaluation_metrics.provenance_id`, …).
    ///
    /// Workspace-scoped via the bound task-local — the store
    /// rejects without `WORKSPACE_ID` set rather than landing
    /// rows under a different tenant.
    async fn record_activity(
        &self,
        capture: ProvenanceCapture,
        subject: EntityRef,
    ) -> OxResult<ProvenanceId>;

    /// Fetch a record by id. Used by audit-trail surfaces that
    /// need to render the activity body alongside the row that
    /// references it. Returns `None` when the id is unknown
    /// (RLS-scoped — cross-tenant ids resolve to `None`).
    async fn get_provenance_record(&self, id: Uuid) -> OxResult<Option<ProvenanceDef>>;
}
