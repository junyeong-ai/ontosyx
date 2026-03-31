use std::sync::Arc;

use chrono::{DateTime, Utc};
use ox_core::error::OxResult;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::embedding::{EmbeddingProvider, EmbeddingRole};
use crate::vector::{MemoryFilter, VectorStore};

// ---------------------------------------------------------------------------
// Memory types
// ---------------------------------------------------------------------------

/// Source of a memory entry — identifies what created it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    Query,
    Analysis,
    Edit,
    Session,
    Recipe,
    /// Schema node/edge descriptions indexed for RAG-based query translation.
    Schema,
}

/// Metadata associated with a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub source: MemorySource,
    pub ontology_id: Option<String>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A memory entry to be stored.
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub metadata: MemoryMetadata,
}

/// A memory search result.
#[derive(Debug, Clone)]
pub struct MemoryHit {
    pub id: String,
    pub content: String,
    pub metadata: MemoryMetadata,
    pub score: f32,
}

// ---------------------------------------------------------------------------
// Instruction templates for instruction-aware embedding
// ---------------------------------------------------------------------------

pub mod instructions {
    pub fn storage(source: &super::MemorySource) -> &'static str {
        match source {
            super::MemorySource::Query => {
                "Represent the data query and its results for retrieval"
            }
            super::MemorySource::Analysis => {
                "Represent the data analysis methodology and findings"
            }
            super::MemorySource::Edit => {
                "Represent the ontology modification and its rationale"
            }
            super::MemorySource::Session => {
                "Represent the agent session summary for retrieval"
            }
            super::MemorySource::Recipe => {
                "Represent the data analysis algorithm for reuse"
            }
            super::MemorySource::Schema => {
                "Represent the ontology schema node with its properties, relationships, and domain semantics"
            }
        }
    }

    pub fn search(source: Option<&super::MemorySource>) -> &'static str {
        match source {
            Some(super::MemorySource::Query) => "Find past queries similar to this question",
            Some(super::MemorySource::Analysis) => "Find relevant past analyses",
            Some(super::MemorySource::Edit) => "Find related ontology changes",
            Some(super::MemorySource::Recipe) => {
                "Find a reusable analysis recipe for this task"
            }
            Some(super::MemorySource::Schema) => {
                "Find ontology schema nodes relevant to this data question"
            }
            Some(super::MemorySource::Session) | None => {
                "Find relevant information from past sessions"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MemoryStore — unified embedding + vector search
// ---------------------------------------------------------------------------

/// Unified memory interface combining embedding and vector search.
///
/// Lifecycle:
/// 1. `store()`: Embed content → persist vector + content + metadata
/// 2. `search()`: Embed query → cosine similarity → return matches with content
/// 3. `delete()`: Remove entry
pub struct MemoryStore {
    embedder: Arc<dyn EmbeddingProvider>,
    vectors: Arc<dyn VectorStore>,
}

impl MemoryStore {
    pub fn new(
        embedder: Arc<dyn EmbeddingProvider>,
        vectors: Arc<dyn VectorStore>,
    ) -> Self {
        Self { embedder, vectors }
    }

    /// Store a memory entry: embed content → persist to vector store.
    pub async fn store(&self, entry: MemoryEntry) -> OxResult<()> {
        let instruction = instructions::storage(&entry.metadata.source);
        let embedding = self
            .embedder
            .embed(&entry.content, instruction, EmbeddingRole::Document)
            .await?;
        let metadata = serde_json::to_value(&entry.metadata).unwrap_or_default();

        self.vectors
            .upsert(&entry.id, &embedding, &entry.content, &metadata)
            .await?;

        info!(
            id = %entry.id,
            source = ?entry.metadata.source,
            "Memory stored"
        );

        Ok(())
    }

    /// Search for related memories using semantic similarity.
    ///
    /// When `ontology_id` is provided, results are scoped to that ontology.
    pub async fn search(
        &self,
        query: &str,
        source_hint: Option<&MemorySource>,
        top_k: usize,
        ontology_id: Option<&str>,
    ) -> OxResult<Vec<MemoryHit>> {
        let instruction = instructions::search(source_hint);
        let embedding = self
            .embedder
            .embed(query, instruction, EmbeddingRole::Query)
            .await?;

        let filter = ontology_id.map(|id| serde_json::json!({"ontology_id": id}));
        let hits = self.vectors.search(&embedding, top_k, filter.as_ref()).await?;

        let results = hits
            .into_iter()
            .filter_map(|hit| {
                let metadata: MemoryMetadata =
                    serde_json::from_value(hit.metadata).ok()?;
                Some(MemoryHit {
                    id: hit.id,
                    content: hit.content,
                    metadata,
                    score: hit.score,
                })
            })
            .collect();

        Ok(results)
    }

    /// Search for related memories using semantic similarity with structured metadata filters.
    ///
    /// This complements `search()` by accepting a typed `MemoryFilter` that can
    /// filter by `ontology_id`, `source`, and `session_id` simultaneously.
    pub async fn search_filtered(
        &self,
        query: &str,
        source_hint: Option<&MemorySource>,
        top_k: usize,
        filter: &MemoryFilter,
    ) -> OxResult<Vec<MemoryHit>> {
        let instruction = instructions::search(source_hint);
        let embedding = self
            .embedder
            .embed(query, instruction, EmbeddingRole::Query)
            .await?;

        let hits = self.vectors.search_filtered(&embedding, top_k, filter).await?;

        let results = hits
            .into_iter()
            .filter_map(|hit| {
                let metadata: MemoryMetadata =
                    serde_json::from_value(hit.metadata).ok()?;
                Some(MemoryHit {
                    id: hit.id,
                    content: hit.content,
                    metadata,
                    score: hit.score,
                })
            })
            .collect();

        Ok(results)
    }

    /// Pattern-based text search (ILIKE/trigram) — for exact keyword matching.
    /// Complements semantic `search()` when the user knows specific terms.
    pub async fn pattern_search(&self, pattern: &str, top_k: usize) -> OxResult<Vec<MemoryHit>> {
        let hits = self.vectors.pattern_search(pattern, top_k).await?;

        let results = hits
            .into_iter()
            .filter_map(|hit| {
                let metadata: MemoryMetadata =
                    serde_json::from_value(hit.metadata).ok()?;
                Some(MemoryHit {
                    id: hit.id,
                    content: hit.content,
                    metadata,
                    score: hit.score,
                })
            })
            .collect();

        Ok(results)
    }

    /// Delete a memory entry.
    pub async fn delete(&self, id: &str) -> OxResult<()> {
        self.vectors.delete(id).await
    }

    /// Delete stale memory entries that haven't been accessed within `retention_days`.
    pub async fn cleanup_stale(&self, retention_days: i64) -> OxResult<u64> {
        self.vectors.cleanup_stale(retention_days).await
    }

    /// Delete all memory entries for a specific ontology.
    pub async fn cleanup_by_ontology(&self, ontology_id: &str) -> OxResult<u64> {
        let filter = MemoryFilter {
            ontology_id: Some(ontology_id.to_string()),
            ..Default::default()
        };
        self.vectors.cleanup_by_filter(&filter).await
    }

    /// Delete all memory entries for a specific session.
    pub async fn cleanup_by_session(&self, session_id: &str) -> OxResult<u64> {
        let filter = MemoryFilter {
            session_id: Some(session_id.to_string()),
            ..Default::default()
        };
        self.vectors.cleanup_by_filter(&filter).await
    }

    pub fn provider_name(&self) -> &str {
        self.embedder.provider_name()
    }

    pub fn store_name(&self) -> &str {
        self.vectors.store_name()
    }
}
