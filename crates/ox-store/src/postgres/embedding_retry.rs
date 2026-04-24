//! [`EmbeddingRetryStore`] — pending_embeddings retry queue with bounded retry_count.

use super::*;

#[async_trait]
impl EmbeddingRetryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pending_embedding(
        &self,
        content: &str,
        metadata: &serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query("INSERT INTO pending_embeddings (content, metadata) VALUES ($1, $2)")
            .bind(content)
            .bind(metadata)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pending_embeddings(&self, limit: i64) -> OxResult<Vec<PendingEmbedding>> {
        sqlx::query_as(
            "SELECT * FROM pending_embeddings WHERE retry_count < 3 ORDER BY created_at LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn mark_embedding_failed(&self, id: Uuid, error: &str) -> OxResult<()> {
        sqlx::query(
            "UPDATE pending_embeddings SET retry_count = retry_count + 1, last_error = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM pending_embeddings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(result.rows_affected() > 0)
    }
}
