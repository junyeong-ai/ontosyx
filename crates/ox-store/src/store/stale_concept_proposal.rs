//! Durable stale-concept deprecation proposals. Populated by the
//! `scan_stale_concepts` cron; admins decide approve / dismiss.

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::quality_signal::{StaleConceptProposal, StaleProposalDecision};

#[async_trait]
pub trait StaleConceptProposalStore: Send + Sync {
    /// List proposals visible to the current workspace. When
    /// `pending_only` is true, terminal (approved/dismissed) rows
    /// are excluded — the admin dashboard hot path.
    async fn list_stale_concept_proposals(
        &self,
        pending_only: bool,
    ) -> OxResult<Vec<StaleConceptProposal>>;

    /// Get a single proposal by id. Returns `None` when not found
    /// (RLS-scoped — a cross-workspace id looks like "not found").
    async fn get_stale_concept_proposal(
        &self,
        id: uuid::Uuid,
    ) -> OxResult<Option<StaleConceptProposal>>;

    /// Insert if not present (natural key = `(workspace_id, type_id)`).
    /// Cron calls this per stale hit; duplicates are silently no-ops.
    /// Returns the resulting row (newly inserted OR the existing one).
    async fn upsert_stale_concept_proposal(
        &self,
        proposal: StaleConceptProposal,
    ) -> OxResult<StaleConceptProposal>;

    /// Record an admin decision on a pending proposal. Noop when
    /// the proposal is already in a terminal state (returns the
    /// existing row).
    async fn record_stale_proposal_decision(
        &self,
        id: uuid::Uuid,
        decision: StaleProposalDecision,
        decided_by_user_id: Option<uuid::Uuid>,
        reason: Option<String>,
    ) -> OxResult<StaleConceptProposal>;

    /// Bulk variant — apply the same decision to every pending
    /// proposal whose id is in `ids`. Returns the count of rows
    /// actually transitioned (rows already in a terminal state
    /// are silently skipped, mirroring the single-id semantics).
    /// One round-trip regardless of `ids.len()`.
    async fn record_stale_proposal_decisions(
        &self,
        ids: &[uuid::Uuid],
        decision: StaleProposalDecision,
        decided_by_user_id: Option<uuid::Uuid>,
        reason: Option<String>,
    ) -> OxResult<u64>;
}
