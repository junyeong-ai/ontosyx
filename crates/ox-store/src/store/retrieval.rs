//! GraphRAG retrieval policy persistence.
//!
//! Backs `retrieval_profiles` from the schema baseline. Two
//! orthogonal axes — `RetrievalProfileStore` here for the online
//! retrieval shape, `CommunityDetectionPolicyStore` (sibling
//! file) for the offline detection cron.
//!
//! Workspace-scoped. UPSERT on the `(workspace_id, name)` natural
//! key — re-importing under the same name preserves `id` +
//! `created_at` and updates the rest.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::{RetrievalProfile, RetrievalProfileId};

#[async_trait]
pub trait RetrievalProfileStore: Send + Sync {
    /// Insert-or-update on `(workspace_id, name)`. Returns the
    /// persisted row so the caller picks up the server-stamped
    /// `created_at` / `updated_at` without re-fetching.
    async fn upsert_retrieval_profile(
        &self,
        profile: &RetrievalProfile,
    ) -> OxResult<RetrievalProfile>;

    /// Lookup by id. RLS-scoped — cross-tenant ids resolve to
    /// `None`.
    async fn get_retrieval_profile(
        &self,
        id: &RetrievalProfileId,
    ) -> OxResult<Option<RetrievalProfile>>;

    /// Lookup by `(workspace_id, name)` — used by the agent /
    /// CLI flows that reference profiles by their human-readable
    /// name rather than id.
    async fn find_retrieval_profile_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<RetrievalProfile>>;

    /// List every profile in the active workspace, newest-updated
    /// first.
    async fn list_retrieval_profiles(&self) -> OxResult<Vec<RetrievalProfile>>;

    /// Delete a profile by id. Returns `Ok(false)` when no row
    /// matched — distinguishing "deleted" from "not found"
    /// without a separate exists probe. Caller is responsible for
    /// not breaking referential integrity (eval runs that pin
    /// the id will fail later — `Perspective.retrieval_profile_id`
    /// is `Option`, so a perspective with the deleted id loses
    /// its profile reference but stays alive).
    async fn delete_retrieval_profile(&self, id: &RetrievalProfileId) -> OxResult<bool>;
}
