//! `SourceContractStore` — workspace-scoped persistence for the
//! frozen physical-shape snapshots Φ12's commit-path validator
//! consumes.
//!
//! Workspace-scoped, 4-clause RLS. UPSERT on the
//! `(workspace_id, source_id, relation)` natural key — re-running
//! the introspection pipeline against an unchanged relation is
//! idempotent, and a fingerprint mismatch on the inbound row
//! signals schema drift the FE surfaces.
//!
//! The validator that consumes contracts lives upstream in
//! `ox-ontology::OntologyIR::validate_against_source_contracts`;
//! this crate only owns the persistence layer.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::{SourceContractDef, mapping::SourceId};

#[async_trait]
pub trait SourceContractStore: Send + Sync {
    /// Insert-or-update on `(workspace_id, source_id, relation)`.
    /// Returns the persisted row so the caller picks up the
    /// server-stamped `introspected_at` (always replaced with
    /// `now()` by the impl) and the canonicalised fingerprint
    /// without re-fetching.
    async fn upsert_source_contract(
        &self,
        contract: &SourceContractDef,
    ) -> OxResult<SourceContractDef>;

    /// Fast natural-key lookup. The introspection pipeline calls
    /// this to compare the incoming row's fingerprint against the
    /// stored fingerprint and detect schema drift before
    /// upserting; the validator path uses
    /// [`Self::list_source_contracts`] instead.
    async fn find_source_contract(
        &self,
        source_id: &SourceId,
        relation: &str,
    ) -> OxResult<Option<SourceContractDef>>;

    /// Every contract in the active workspace.
    ///
    /// The commit-path validator consumes the entire bank because
    /// it walks every mapping in the ontology IR — selectively
    /// loading would require a fan-out of N+1 queries on the
    /// mapping → source-id projection. The bank is bounded by the
    /// number of relations the workspace has introspected, which
    /// is the same scale as `TableInventoryEntry` rows.
    async fn list_source_contracts(&self) -> OxResult<Vec<SourceContractDef>>;

    /// List contracts narrowed to one source. The Source Inspector
    /// FE uses this to render "this source has been introspected
    /// for N relations, here is the per-column shape" without
    /// cross-source noise.
    async fn list_source_contracts_for_source(
        &self,
        source_id: &SourceId,
    ) -> OxResult<Vec<SourceContractDef>>;

    /// Hard delete by natural key. The retraction path that
    /// removes a contract whose source has been dropped from the
    /// workspace; subsequent `validate_against_source_contracts`
    /// soft-skips that source again until re-introspection.
    /// Returns `Ok(false)` when the row did not exist.
    async fn delete_source_contract(&self, source_id: &SourceId, relation: &str) -> OxResult<bool>;
}
