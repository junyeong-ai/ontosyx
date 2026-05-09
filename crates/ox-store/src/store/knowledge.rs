//! Failure-driven learning knowledge base.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{KnowledgeEntry, KnowledgeStatus};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn create_knowledge_entry(&self, entry: &KnowledgeEntry) -> OxResult<()>;
    async fn get_knowledge_entry(&self, id: Uuid) -> OxResult<Option<KnowledgeEntry>>;
    async fn update_knowledge_entry(
        &self,
        id: Uuid,
        title: &str,
        content: &str,
        structured_data: &serde_json::Value,
        affected_labels: &[String],
        affected_properties: &[String],
        tokenized_text: &str,
        tokenizer_dict_fingerprint: &str,
    ) -> OxResult<()>;
    async fn delete_knowledge_entry(&self, id: Uuid) -> OxResult<bool>;

    async fn list_knowledge_entries(
        &self,
        ontology_name: Option<&str>,
        kind: Option<&str>,
        status: Option<&str>,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<KnowledgeEntry>>;

    /// Approved entries for a given ontology name and version, ordered by confidence.
    async fn list_active_knowledge(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        kinds: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>>;

    async fn update_knowledge_status(
        &self,
        id: Uuid,
        status: KnowledgeStatus,
        reviewer_id: Option<Uuid>,
        review_notes: Option<&str>,
    ) -> OxResult<()>;

    async fn update_knowledge_confidence(&self, id: Uuid, confidence: f64) -> OxResult<()>;

    /// Bulk-mark entries as stale when affected_labels overlap with changed labels.
    async fn expire_knowledge_by_labels(
        &self,
        ontology_name: &str,
        changed_labels: &[String],
    ) -> OxResult<u64>;

    /// Fire-and-forget: increment use_count and update last_used_at.
    async fn record_knowledge_usage(&self, ids: &[Uuid]) -> OxResult<()>;

    /// Admin confirms knowledge is valid for a given ontology version.
    async fn verify_knowledge(&self, id: Uuid, version: i32) -> OxResult<()>;

    /// Label-based GIN lookup: affected_labels && $labels, ordered by confidence.
    async fn search_knowledge_by_labels(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        labels: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>>;

    /// Hybrid 4-ranker retrieval over the active knowledge bank.
    ///
    /// Reciprocal Rank Fusion (k = 60) over independent rankers,
    /// each pulling `limit * 3` candidates:
    ///
    /// 1. **Trigram (title)** — `title % $1` ranked by similarity.
    ///    Catches paraphrases of the correction's headline.
    /// 2. **Trigram (content)** — `content % $1` ranked by
    ///    similarity. Catches mid-prose phrasings.
    /// 3. **Lexical FTS** — `searchable_tsv @@
    ///    plainto_tsquery('simple', $tokenized)` ranked by
    ///    `ts_rank_cd`. Workspace lindera + glossary user-dict
    ///    canonicalises both write-time and runtime tokens so
    ///    Korean compounds + concept synonyms collapse.
    /// 4. **Vector** — `embedding <=> $vec` cosine NN. Optional
    ///    — `None` skips this arm; the 3-ranker fusion still
    ///    applies.
    ///
    /// Eligibility hardgated by ontology compatibility (the
    /// caller already knows the workspace via RLS):
    ///
    /// - `ontology_name = $name`
    /// - `status = 'approved'`
    /// - `ontology_version_min <= $version AND
    ///    (ontology_version_max IS NULL OR ontology_version_max
    ///    >= $version)`
    ///
    /// **Label hint** is a soft boost, not a filter — when the
    /// caller passes `label_hints`, every row whose
    /// `affected_labels && hints` is non-empty contributes a
    /// fifth synthetic ranker (rank 1 for matched, infinity for
    /// non-matched), so a corrections targeting an affected
    /// label outscores an equally-similar correction without
    /// the label match. Empty `label_hints` → ranker omitted
    /// entirely.
    ///
    /// Confidence is multiplied into the final RRF score so a
    /// row with 0.9 confidence outranks a row with 0.5
    /// confidence at the same fusion rank — a long-tail signal
    /// the bare ranker doesn't carry.
    async fn hybrid_search_knowledge_entries(
        &self,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
        ontology_name: &str,
        ontology_version: i32,
        label_hints: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>>;

    /// Counts grouped by (status, kind) — for dashboard stats without loading all rows.
    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>>;

    /// Delete deprecated entries older than N days + auto-deprecate confidence < 0.1.
    async fn cleanup_knowledge(&self, older_than_days: i64) -> OxResult<u64>;
}
