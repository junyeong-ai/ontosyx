use async_trait::async_trait;
use ox_core::error::{OxError, OxResult};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::OnceLock;
use tokio::sync::Semaphore;

use super::{MemoryFilter, VectorHit, VectorStore};

/// Limit concurrent fire-and-forget DB updates to prevent pool exhaustion.
static BG_TASK_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();

fn bg_semaphore() -> &'static Semaphore {
    BG_TASK_SEMAPHORE.get_or_init(|| Semaphore::new(8))
}

/// Vector store backed by PostgreSQL with the pgvector extension.
///
/// Uses HNSW indexing for fast approximate nearest neighbor search
/// with cosine similarity.
pub struct PgVectorStore {
    pool: PgPool,
    dimensions: usize,
}

impl PgVectorStore {
    pub fn new(pool: PgPool, dimensions: usize) -> Self {
        Self { pool, dimensions }
    }

    /// Connect to PostgreSQL and create a PgVectorStore.
    pub async fn connect(url: &str, dimensions: usize) -> Result<Self, ox_core::error::OxError> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| ox_core::error::OxError::Runtime {
                message: format!("Memory vector store connection failed: {e}"),
            })?;
        Ok(Self { pool, dimensions })
    }

    /// Format a float vector as pgvector literal: '[0.1,0.2,0.3]'
    fn format_vector(embedding: &[f32]) -> String {
        let values: Vec<String> = embedding.iter().map(|v| format!("{v}")).collect();
        format!("[{}]", values.join(","))
    }

    /// Verify pgvector extension is available.
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1 FROM pg_extension WHERE extname = 'vector'")
            .fetch_optional(&self.pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false)
    }
}

#[async_trait]
impl VectorStore for PgVectorStore {
    async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        content: &str,
        metadata: &Value,
    ) -> OxResult<()> {
        if embedding.len() != self.dimensions {
            return Err(OxError::Validation {
                field: "embedding".to_string(),
                message: format!(
                    "Expected {} dimensions, got {}",
                    self.dimensions,
                    embedding.len()
                ),
            });
        }

        let vector_str = Self::format_vector(embedding);

        sqlx::query(
            "INSERT INTO memory_entries (id, embedding, content, metadata)
             VALUES ($1, $2::vector, $3, $4)
             ON CONFLICT (id) DO UPDATE SET
                embedding = EXCLUDED.embedding,
                content = EXCLUDED.content,
                metadata = EXCLUDED.metadata",
        )
        .bind(id)
        .bind(&vector_str)
        .bind(content)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Vector upsert failed: {e}"),
        })?;

        Ok(())
    }

    async fn search(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: Option<&Value>,
    ) -> OxResult<Vec<VectorHit>> {
        let vector_str = Self::format_vector(embedding);

        // Extract ontology_id filter for scoped search
        let ontology_filter = filter
            .and_then(|f| f.get("ontology_id"))
            .and_then(|v| v.as_str());

        let rows: Vec<(String, String, f64, Value)> = if let Some(ont_id) = ontology_filter {
            sqlx::query_as(
                "SELECT id, content, 1 - (embedding <=> $1::vector) AS score, metadata
                 FROM memory_entries
                 WHERE metadata->>'ontology_id' = $3
                 ORDER BY embedding <=> $1::vector
                 LIMIT $2",
            )
            .bind(&vector_str)
            .bind(top_k as i64)
            .bind(ont_id)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT id, content, 1 - (embedding <=> $1::vector) AS score, metadata
                 FROM memory_entries
                 ORDER BY embedding <=> $1::vector
                 LIMIT $2",
            )
            .bind(&vector_str)
            .bind(top_k as i64)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| OxError::Runtime {
            message: format!("Vector search failed: {e}"),
        })?;

        // Fire-and-forget: update last_accessed_at (bounded to prevent pool exhaustion)
        if !rows.is_empty() {
            let ids: Vec<String> = rows.iter().map(|(id, _, _, _)| id.clone()).collect();
            let pool = self.pool.clone();
            tokio::spawn(async move {
                let Ok(_permit) = bg_semaphore().acquire().await else { return };
                let _ = sqlx::query(
                    "UPDATE memory_entries SET last_accessed_at = NOW() WHERE id = ANY($1)",
                )
                .bind(&ids)
                .execute(&pool)
                .await;
            });
        }

        let hits = rows
            .into_iter()
            .map(|(id, content, score, metadata)| VectorHit {
                id,
                content,
                score: score as f32,
                metadata,
            })
            .collect();

        Ok(hits)
    }

    async fn search_filtered(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: &MemoryFilter,
    ) -> OxResult<Vec<VectorHit>> {
        let vector_str = Self::format_vector(embedding);

        // Build dynamic WHERE clause from non-None filter fields.
        let mut conditions = Vec::new();
        let mut param_idx = 3u32; // $1 = embedding, $2 = limit

        if filter.ontology_id.is_some() {
            conditions.push(format!("metadata->>'ontology_id' = ${param_idx}"));
            param_idx += 1;
        }
        if filter.source.is_some() {
            conditions.push(format!("metadata->>'source' = ${param_idx}"));
            param_idx += 1;
        }
        if filter.session_id.is_some() {
            conditions.push(format!("metadata->>'session_id' = ${param_idx}"));
            // param_idx += 1; // last one, no need to increment
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, content, 1 - (embedding <=> $1::vector) AS score, metadata \
             FROM memory_entries {where_clause} \
             ORDER BY embedding <=> $1::vector \
             LIMIT $2"
        );

        let mut query = sqlx::query_as::<_, (String, String, f64, Value)>(&sql)
            .bind(&vector_str)
            .bind(top_k as i64);

        // Bind filter params in the same order as the WHERE conditions.
        if let Some(ref oid) = filter.ontology_id {
            query = query.bind(oid);
        }
        if let Some(ref src) = filter.source {
            query = query.bind(src);
        }
        if let Some(ref sid) = filter.session_id {
            query = query.bind(sid);
        }

        let rows: Vec<(String, String, f64, Value)> =
            query.fetch_all(&self.pool).await.map_err(|e| OxError::Runtime {
                message: format!("Vector search_filtered failed: {e}"),
            })?;

        // Fire-and-forget: update last_accessed_at
        if !rows.is_empty() {
            let ids: Vec<String> = rows.iter().map(|(id, _, _, _)| id.clone()).collect();
            let pool = self.pool.clone();
            tokio::spawn(async move {
                let Ok(_permit) = bg_semaphore().acquire().await else {
                    return;
                };
                let _ = sqlx::query(
                    "UPDATE memory_entries SET last_accessed_at = NOW() WHERE id = ANY($1)",
                )
                .bind(&ids)
                .execute(&pool)
                .await;
            });
        }

        let hits = rows
            .into_iter()
            .map(|(id, content, score, metadata)| VectorHit {
                id,
                content,
                score: score as f32,
                metadata,
            })
            .collect();

        Ok(hits)
    }

    async fn pattern_search(
        &self,
        pattern: &str,
        top_k: usize,
    ) -> OxResult<Vec<VectorHit>> {
        // Escape SQL ILIKE special characters before wrapping with %.
        let escaped = pattern.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let like_pattern = format!("%{escaped}%");

        let rows: Vec<(String, String, Value)> = sqlx::query_as(
            "SELECT id, content, metadata
             FROM memory_entries
             WHERE content ILIKE $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(&like_pattern)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Pattern search failed: {e}"),
        })?;

        let hits = rows
            .into_iter()
            .map(|(id, content, metadata)| VectorHit {
                id,
                content,
                score: 1.0, // Pattern match = exact relevance
                metadata,
            })
            .collect();

        Ok(hits)
    }

    async fn delete(&self, id: &str) -> OxResult<()> {
        sqlx::query("DELETE FROM memory_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Vector delete failed: {e}"),
            })?;
        Ok(())
    }

    async fn cleanup_stale(&self, retention_days: i64) -> OxResult<u64> {
        let result = sqlx::query(
            "DELETE FROM memory_entries WHERE last_accessed_at IS NOT NULL AND last_accessed_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Memory cleanup failed: {e}"),
        })?;
        Ok(result.rows_affected())
    }

    async fn cleanup_by_filter(&self, filter: &MemoryFilter) -> OxResult<u64> {
        let mut conditions = Vec::new();
        let mut param_idx = 1u32;

        if filter.ontology_id.is_some() {
            conditions.push(format!("metadata->>'ontology_id' = ${param_idx}"));
            param_idx += 1;
        }
        if filter.source.is_some() {
            conditions.push(format!("metadata->>'source' = ${param_idx}"));
            param_idx += 1;
        }
        if filter.session_id.is_some() {
            conditions.push(format!("metadata->>'session_id' = ${param_idx}"));
            // param_idx += 1; // last one, no need to increment
        }

        if conditions.is_empty() {
            return Ok(0); // Safety: never delete everything
        }

        let sql = format!(
            "DELETE FROM memory_entries WHERE {}",
            conditions.join(" AND ")
        );

        let mut query = sqlx::query(&sql);

        // Bind filter params in the same order as the WHERE conditions.
        if let Some(ref oid) = filter.ontology_id {
            query = query.bind(oid);
        }
        if let Some(ref src) = filter.source {
            query = query.bind(src);
        }
        if let Some(ref sid) = filter.session_id {
            query = query.bind(sid);
        }

        let result = query.execute(&self.pool).await.map_err(|e| OxError::Runtime {
            message: format!("Memory cleanup_by_filter failed: {e}"),
        })?;

        Ok(result.rows_affected())
    }

    fn store_name(&self) -> &str {
        "pgvector"
    }
}
