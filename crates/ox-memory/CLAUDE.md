# ox-memory

Semantic memory: embedding + vector search for long-term agent learning.

## Architecture

- `EmbeddingProvider` trait — text → vector. Implementations: `OnnxEmbeddingProvider` (local, requires `onnx` feature), `NoopEmbeddingProvider` (zero vectors for testing).
- `VectorStore` trait — storage-agnostic similarity search. Implementation: `PgVectorStore` (PostgreSQL + pgvector).
- `MemoryStore` — unified facade combining embedding + vector ops.

## Feature Gating

The `onnx` feature enables ONNX runtime (ort 2.0), ndarray, and tokenizers. Without it, `NoopEmbeddingProvider` produces zero vectors — functional for tests but no real similarity.
