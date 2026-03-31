pub mod pgvector;

use async_trait::async_trait;
use ox_core::error::OxResult;
use serde_json::Value;

// ---------------------------------------------------------------------------
// VectorStore — storage-agnostic vector similarity search
// ---------------------------------------------------------------------------

/// A single vector search result.
#[derive(Debug, Clone)]
pub struct VectorHit {
    pub id: String,
    /// Original text content of the entry.
    pub content: String,
    /// Cosine similarity score (0.0 to 1.0).
    pub score: f32,
    /// Structured metadata (source, ontology_id, session_id, etc.).
    pub metadata: Value,
}

/// Optional metadata filters for vector search.
///
/// Non-`None` fields are combined with AND logic and matched against
/// the JSONB `metadata` column in the `memory_entries` table.
#[derive(Debug, Default, Clone)]
pub struct MemoryFilter {
    pub ontology_id: Option<String>,
    pub source: Option<String>,
    pub session_id: Option<String>,
}

/// Trait for vector similarity search backends.
///
/// Implementations:
/// - `PgVectorStore`: PostgreSQL with pgvector extension
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert or update a vector entry with content and metadata.
    async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        content: &str,
        metadata: &Value,
    ) -> OxResult<()>;

    /// Search for similar vectors using cosine similarity.
    /// Returns entries with content, score, and metadata.
    async fn search(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: Option<&Value>,
    ) -> OxResult<Vec<VectorHit>>;

    /// Search with structured metadata filters applied as WHERE clauses.
    ///
    /// Default implementation delegates to `search` (ignoring filters) so
    /// that existing backends keep working without changes.
    async fn search_filtered(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: &MemoryFilter,
    ) -> OxResult<Vec<VectorHit>> {
        // Fallback: build a Value filter from ontology_id only (matches legacy path).
        let json_filter = filter
            .ontology_id
            .as_deref()
            .map(|id| serde_json::json!({"ontology_id": id}));
        self.search(embedding, top_k, json_filter.as_ref()).await
    }

    /// Pattern-based text search (ILIKE / trigram).
    /// Complements semantic search for exact keyword/pattern matching.
    async fn pattern_search(&self, pattern: &str, top_k: usize) -> OxResult<Vec<VectorHit>>;

    /// Delete a vector entry by ID.
    async fn delete(&self, id: &str) -> OxResult<()>;

    /// Delete stale memory entries that haven't been accessed within `retention_days`.
    async fn cleanup_stale(&self, retention_days: i64) -> OxResult<u64>;

    /// Delete all entries matching a metadata filter.
    /// Returns the number of entries deleted.
    /// Safety: returns 0 if the filter is empty (no conditions) to prevent
    /// accidental full table wipes.
    async fn cleanup_by_filter(&self, filter: &MemoryFilter) -> OxResult<u64> {
        let _ = filter;
        Ok(0)
    }

    /// Store name for logging and diagnostics.
    fn store_name(&self) -> &str;
}
