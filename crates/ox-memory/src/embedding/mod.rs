pub mod noop;
#[cfg(feature = "onnx")]
pub mod onnx;

use async_trait::async_trait;
use ox_core::error::OxResult;

// ---------------------------------------------------------------------------
// EmbeddingRole — asymmetric embedding intent
// ---------------------------------------------------------------------------

/// Indicates whether text is being embedded for storage (document-side)
/// or for retrieval (query-side).
///
/// Many modern embedding models produce asymmetric embeddings:
/// - **Document**: optimized for being found (indexed content)
/// - **Query**: optimized for finding (search queries)
///
/// Models that don't distinguish (symmetric) can ignore this parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbeddingRole {
    /// Embedding content for storage/indexing.
    #[default]
    Document,
    /// Embedding a search query for retrieval.
    Query,
}

// ---------------------------------------------------------------------------
// EmbeddingProvider — model-agnostic text embedding
// ---------------------------------------------------------------------------

/// Trait for text embedding providers.
///
/// Supports both symmetric and asymmetric embedding models:
/// - **Asymmetric** (jina-v5): uses `role` to prepend `"Query: "` or `"Document: "`.
/// - **Instruction-aware** (qwen3-embedding): uses `instruction` for semantic guidance.
/// - **Symmetric** (basic models): ignores both `role` and `instruction`.
///
/// Implementations:
/// - `OnnxEmbeddingProvider`: Local ONNX model (auto-detects model capabilities)
/// - `NoopEmbeddingProvider`: Zero vectors for pipeline testing
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text.
    ///
    /// - `instruction`: semantic guidance for instruction-aware models
    ///   (e.g., "Represent the data analysis for retrieval").
    /// - `role`: whether this is a document or query embedding.
    async fn embed(&self, text: &str, instruction: &str, role: EmbeddingRole) -> OxResult<Vec<f32>>;

    /// Embed multiple texts with paired instructions.
    /// Default implementation calls `embed()` sequentially.
    async fn embed_batch(
        &self,
        items: &[(String, String)],
        role: EmbeddingRole,
    ) -> OxResult<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(items.len());
        for (text, instruction) in items {
            results.push(self.embed(text, instruction, role).await?);
        }
        Ok(results)
    }

    /// Embedding vector dimensions (e.g., 1024).
    fn dimensions(&self) -> usize;

    /// Provider name for logging and diagnostics.
    fn provider_name(&self) -> &str;
}
