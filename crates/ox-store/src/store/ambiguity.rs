//! Closed-loop ambiguity resolver storage.
//!
//! Context rows are detected during source analysis and upserted by
//! natural key `(source_id, relation, column)`. Resolutions append to
//! a history log; at most one non-revoked resolution is active per
//! context (DB-enforced by partial unique index). Superseding a
//! resolution revokes the previous active row *and* writes the new
//! row in the same transaction — the store impl is responsible for
//! the atomicity.

use async_trait::async_trait;

use ox_core::error::OxResult;

#[async_trait]
pub trait AmbiguityStore: Send + Sync {
    async fn list_ambiguity_contexts(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>>;

    /// List every context visible in the current workspace (RLS
    /// bounded). Backs the admin `/settings/ambiguity` dashboard,
    /// which can't scope by source_id because it shows all pending
    /// ambiguities across data sources at once.
    async fn list_ambiguity_contexts_in_workspace(
        &self,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>>;

    async fn get_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>>;

    async fn find_ambiguity_context_by_column(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>>;

    /// Upsert by natural key. Replaces the row when
    /// `(source_id, relation, column)` already exists — the refresh
    /// path for re-running analysis against a changed schema.
    async fn upsert_ambiguity_context(
        &self,
        context: ox_ontology::ambiguity::AmbiguityContext,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityContext>;

    async fn delete_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool>;

    async fn list_ambiguity_resolutions(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityResolution>>;

    async fn find_active_ambiguity_resolution(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityResolution>>;

    /// Atomically revoke the prior active resolution (if any) and
    /// record the new resolution as active. `supersedes` on the new
    /// row points at the revoked one. Returns the inserted row.
    async fn create_ambiguity_resolution(
        &self,
        resolution: ox_ontology::ambiguity::AmbiguityResolution,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution>;

    /// Revoke the currently-active resolution for `context_id`, if any.
    /// Returns `true` when a row transitioned to revoked.
    async fn revoke_active_ambiguity_resolution(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool>;

    /// Bulk-revoke active resolutions for many contexts in one
    /// transaction. Same per-row semantics as
    /// `revoke_active_ambiguity_resolution` (contexts with no active
    /// row are silently skipped). Returns the count that
    /// transitioned — may be less than `ids.len()` when the cohort
    /// overlaps already-revoked or unresolved contexts.
    async fn bulk_revoke_active_ambiguity_resolutions(
        &self,
        context_ids: &[ox_ontology::ambiguity::AmbiguityId],
    ) -> OxResult<u64>;
}
