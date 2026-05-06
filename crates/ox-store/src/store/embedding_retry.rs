//! Pending-embedding retry queue — failed embedding operations are
//! parked here for a periodic sweep to retry.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::PendingEmbedding;

#[async_trait]
pub trait EmbeddingRetryStore: Send + Sync {
    async fn create_pending_embedding(
        &self,
        content: &str,
        metadata: &serde_json::Value,
    ) -> OxResult<()>;
    async fn list_pending_embeddings(&self, limit: i64) -> OxResult<Vec<PendingEmbedding>>;
    async fn record_embedding_failure(&self, id: Uuid, error: &str) -> OxResult<()>;
    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<bool>;
}
