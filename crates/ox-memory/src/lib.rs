//! # ox-memory
//!
//! Semantic memory for the Ontosyx agent — embedding + vector search.
//!
//! Provides trait-based abstractions for:
//! - **EmbeddingProvider**: Model-agnostic text embedding (local ONNX, OpenAI, Bedrock)
//! - **VectorStore**: Storage-agnostic vector similarity search (pgvector, Qdrant)
//! - **MemoryStore**: Unified interface combining embedding + vector operations
//!
//! ## Architecture
//!
//! ```text
//! MemoryStore
//!   ├── EmbeddingProvider (trait)
//!   │   ├── OnnxEmbeddingProvider (auto-detects model capabilities)
//!   │   └── NoopEmbeddingProvider (zero vectors for testing)
//!   └── VectorStore (trait)
//!       ├── PgVectorStore (PostgreSQL + pgvector)
//!       └── (future: Qdrant, Weaviate)
//! ```

pub mod embedding;
pub mod store;
pub mod vector;

pub use embedding::noop::NoopEmbeddingProvider;
#[cfg(feature = "onnx")]
pub use embedding::onnx::OnnxEmbeddingProvider;
pub use embedding::{EmbeddingProvider, EmbeddingRole};
pub use store::{MemoryEntry, MemoryHit, MemoryMetadata, MemorySource, MemoryStore};
pub use vector::pgvector::PgVectorStore;
pub use vector::{MemoryFilter, VectorStore};
