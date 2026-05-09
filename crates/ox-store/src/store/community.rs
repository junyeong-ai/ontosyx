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

    /// Hybrid 3-ranker retrieval over the community layer.
    ///
    /// RRF fusion (k = 60) of three rankers, each pulling
    /// `top_k * 3` candidates:
    ///
    /// 1. **Trigram (title + summary)** —
    ///    `(similarity(title, q) + similarity(summary, q)) DESC`.
    ///    Catches typo / cosmetic variation across both fields.
    /// 2. **Lexical FTS** — `searchable_tsv @@
    ///    plainto_tsquery('simple', $tokenized)` ranked by
    ///    `ts_rank_cd`. Workspace lindera + glossary user-dict
    ///    canonicalises both index-time and runtime tokens so
    ///    Korean compounds / glossary synonyms collapse onto
    ///    the same lemmas.
    /// 3. **Vector NN** — `embedding <=> $vec` cosine. Optional
    ///    — when the caller can't supply an embedding (cold
    ///    start, no embedder), the fusion degrades to
    ///    2-ranker.
    ///
    /// Eligibility: filtered to `ontology_version_id =
    /// $version_id`. Empty `question_raw` short-circuits to
    /// "no results" without burning the SQL roundtrip.
    async fn search_community_summaries(
        &self,
        version_id: Uuid,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
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

    /// Lookup by natural key. The detection cron uses this to
    /// fetch the stored row's `member_fingerprint` before
    /// deciding whether to invoke the LLM summariser; an
    /// unchanged fingerprint short-circuits the call.
    async fn find_community_summary_by_natural_key(
        &self,
        version_id: Uuid,
        community_id: &str,
    ) -> OxResult<Option<CommunitySummary>>;

    async fn delete_community_summary(&self, id: Uuid) -> OxResult<bool>;
}
