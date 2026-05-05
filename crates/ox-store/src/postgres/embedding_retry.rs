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
        let workspace_id = super::bound_workspace_id_for_dml()?;
        sqlx::query(
            "INSERT INTO pending_embeddings (workspace_id, content, metadata) \
             VALUES ($1, $2, $3)",
        )
        .bind(workspace_id)
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
        super::require_workspace_context()?;
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
    async fn record_embedding_failure(&self, id: Uuid, error: &str) -> OxResult<()> {
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
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
