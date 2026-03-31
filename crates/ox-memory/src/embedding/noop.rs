use async_trait::async_trait;
use ox_core::error::OxResult;

use super::{EmbeddingProvider, EmbeddingRole};

/// No-op embedding provider that returns zero vectors.
///
/// Enables the memory pipeline (store, pattern_search) without
/// requiring an actual embedding model. Semantic search returns
/// no meaningful results; use `pattern_search()` instead.
pub struct NoopEmbeddingProvider {
    dimensions: usize,
}

impl NoopEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait]
impl EmbeddingProvider for NoopEmbeddingProvider {
    async fn embed(
        &self,
        _text: &str,
        _instruction: &str,
        _role: EmbeddingRole,
    ) -> OxResult<Vec<f32>> {
        Ok(vec![0.0; self.dimensions])
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "noop"
    }
}
