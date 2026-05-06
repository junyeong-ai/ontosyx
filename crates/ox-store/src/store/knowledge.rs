//! Failure-driven learning knowledge base.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::KnowledgeEntry;

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
        status: &str,
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

    /// Counts grouped by (status, kind) — for dashboard stats without loading all rows.
    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>>;

    /// Delete deprecated entries older than N days + auto-deprecate confidence < 0.1.
    async fn cleanup_knowledge(&self, older_than_days: i64) -> OxResult<u64>;
}
