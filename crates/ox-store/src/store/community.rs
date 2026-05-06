//! `CommunitySummaryStore` — the GraphRAG community-layer
//! storage trait.
//!
//! Workspace-scoped, version-keyed. Mirrors the rest of the
//! Level-3 navigation surface (entity_search_vector,
//! entity_neighbors) — `ontology_version_id` is the version
//! pin, `community_id` is the workspace-supplied or
//! detection-generated stable identifier.
//!
//! Retrieval semantics (the FE / agent will consume these
//! through the upcoming `OntologyNavigationStore::search_communities`
//! method that wraps `search_summaries`):
//!
//! - **Author-time**: operator or detection cron upserts the
//!   summary; UPSERT key is `(ontology_version_id,
//!   community_id)` so re-summarisation replaces in place.
//! - **Retrieval-time**: the agent's GraphRAG path calls
//!   `search_summaries(version, query, top_k)` to surface
//!   relevant communities alongside entity-level matches. The
//!   blend on the postgres side rides `gin_trgm_ops` over the
//!   `summary` and `title` columns.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::community::CommunitySummary;

#[async_trait]
pub trait CommunitySummaryStore: Send + Sync {
    /// Insert-or-update on `(ontology_version_id,
    /// community_id)`. Re-summarising under the same id
    /// replaces the prose in place so lineage stays attached
    /// (downstream FE references / metric rows tied to the
    /// id survive).
    async fn upsert_community_summary(
        &self,
        summary: &CommunitySummary,
    ) -> OxResult<CommunitySummary>;

    /// List every community summary attached to the version.
    /// Sorted by `(level ASC, community_id ASC)` so the FE
    /// renders the hierarchy top-down deterministically.
    async fn list_community_summaries_for_version(
        &self,
        version_id: Uuid,
    ) -> OxResult<Vec<CommunitySummary>>;

    /// Trigram-blended search over title + summary text.
    /// Returns `top_k` rows ranked by combined similarity.
    /// Empty query short-circuits to "no results" — the
    /// caller is expected to guard against that already, but
    /// the contract is explicit.
    async fn search_community_summaries(
        &self,
        version_id: Uuid,
        query: &str,
        top_k: u32,
    ) -> OxResult<Vec<CommunitySummary>>;

    /// Reverse lookup: which communities contain this entity?
    /// Walks the `gin (member_logical_ids)` array index. The
    /// future agent path "operator picked this anchor — what
    /// communities is it in?" rides on this method.
    async fn list_communities_for_entity(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
    ) -> OxResult<Vec<CommunitySummary>>;

    async fn delete_community_summary(&self, id: Uuid) -> OxResult<bool>;
}
