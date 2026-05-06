//! Hybrid-retrieval primitive types (vector + fulltext + graph).
//!
//! Carries the request shape consumed by `QueryOp::HybridSearch`
//! (the GraphRAG retrieval variant) and the per-engine reader arms
//! (Neo4j `db.index.vector.queryNodes` + `db.index.fulltext.queryNodes`,
//! Memgraph `vector_search.search` + `text_search.search`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pattern::PatternIR;

/// Embedded representation of an operator question (or sub-
/// question from the translation pipeline). Carried as a typed
/// vector so the planner can pass it to per-engine vector-index
/// readers without round-tripping through a string.
///
/// Stored as a `Vec<f32>` rather than a fixed-size array because
/// embedding dimension is model-dependent (1536 for OpenAI's
/// `text-embedding-3-small`, 384 for `all-MiniLM-L6-v2`, 1024
/// for Cohere's `embed-multilingual-v3`); the platform stores
/// the source model alongside the vector so consumers know
/// which index to query against.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Embedding {
    /// Dense vector representation. Per-dimension `f32` is the
    /// industry-standard precision for retrieval embeddings;
    /// the storage cost scales linearly with dimension count
    /// (1536 dims × 4 bytes = ~6KB per vector).
    pub vector: Vec<f32>,
    /// Source model identifier (e.g. `"text-embedding-3-small"`,
    /// `"all-MiniLM-L6-v2"`). Consumers route the vector to the
    /// matching dimension's index — querying a 1536-dim vector
    /// against a 384-dim index is a typed compile-time error.
    pub model_id: String,
    /// Vector dimension count, redundant with `vector.len()`
    /// but cheaper to read in validators. The
    /// `validate_dimension` test pins the consistency.
    pub dim: u32,
}

impl Embedding {
    /// Construct an embedding from a vector + a model id; the
    /// dimension count is inferred from `vector.len()`. The
    /// caller doesn't have to keep the two in sync — the
    /// constructor does.
    pub fn new(vector: Vec<f32>, model_id: impl Into<String>) -> Self {
        let dim = vector.len() as u32;
        Self {
            vector,
            model_id: model_id.into(),
            dim,
        }
    }

    /// `true` when the vector's dimension matches the stored
    /// `dim` field. Validators call this before routing the
    /// embedding to the index reader so a dimension mismatch
    /// surfaces as a clear typed error rather than an opaque
    /// per-engine driver panic.
    pub fn dimension_consistent(&self) -> bool {
        self.vector.len() as u32 == self.dim
    }
}

/// Per-side score-fusion strategy. `ReciprocalRankFusion` is
/// the v1 default — the most well-studied + stable hybrid
/// fusion method (Cormack et al. 2009); weighted-sum is a
/// future option once the platform's cost model has the data
/// to pick weights informed by per-workspace retrieval
/// quality stats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FusionStrategy {
    /// Reciprocal Rank Fusion. For each result candidate, its
    /// fused score is `sum_over_sources(1.0 / (k + rank))`
    /// where `k` is the smoothing constant (default 60 per
    /// the original paper).
    ReciprocalRankFusion {
        /// Smoothing constant. Higher values flatten the
        /// per-source weight; lower values let the top-k
        /// candidates dominate.
        #[serde(default = "default_rrf_k")]
        k: u32,
    },
    /// Weighted sum of per-source scores. Reserved for v2 —
    /// the v1 routes always pick `ReciprocalRankFusion`.
    /// Lands here in the type space so the dispatcher's
    /// match doesn't need a future-shape rewrite.
    WeightedSum {
        vector_weight: f32,
        fulltext_weight: f32,
    },
}

fn default_rrf_k() -> u32 {
    60
}

impl Default for FusionStrategy {
    fn default() -> Self {
        Self::ReciprocalRankFusion { k: default_rrf_k() }
    }
}

/// Hybrid-retrieval request payload. Will be carried inside
/// the future `QueryOp::HybridSearch` variant; the variant
/// itself is deferred (see module-level doc) but the payload
/// type ships now so consumers can reference it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HybridSearchRequest {
    /// Vector query — embeddings of the operator's question
    /// (or a sub-question from the translation pipeline).
    pub vector_query: Embedding,
    /// Lexical full-text query — narrows the candidate pool
    /// before vector ranking. `None` = vector-only retrieval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fulltext_query: Option<String>,
    /// Optional graph-traversal predicate — restricts the
    /// candidate pool to nodes satisfying the pattern (e.g.
    /// "within 2 hops of customer X"). `None` = no graph
    /// constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_constraints: Option<PatternIR>,
    /// Score-fusion strategy. Defaults to
    /// `ReciprocalRankFusion { k: 60 }`.
    #[serde(default)]
    pub fuse: FusionStrategy,
    /// Top-k retrieval count. The reader returns at most
    /// `top_k` candidates after fusion.
    #[serde(default = "default_top_k")]
    pub top_k: u32,
}

fn default_top_k() -> u32 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_new_infers_dim() {
        let e = Embedding::new(vec![0.1, 0.2, 0.3, 0.4], "test-model");
        assert_eq!(e.dim, 4);
        assert_eq!(e.vector.len(), 4);
        assert_eq!(e.model_id, "test-model");
    }

    #[test]
    fn embedding_dimension_consistency_check() {
        let mut e = Embedding::new(vec![0.1, 0.2], "test");
        assert!(e.dimension_consistent());

        // Manually corrupt the dim field — validator must
        // catch it. Without this check a downstream index
        // reader receives a length mismatch as an opaque
        // driver panic.
        e.dim = 999;
        assert!(!e.dimension_consistent());
    }

    #[test]
    fn fusion_default_is_rrf_k60() {
        let f = FusionStrategy::default();
        match f {
            FusionStrategy::ReciprocalRankFusion { k } => assert_eq!(k, 60),
            _ => panic!("expected ReciprocalRankFusion default"),
        }
    }

    #[test]
    fn hybrid_search_request_serialises_top_k() {
        let req = HybridSearchRequest {
            vector_query: Embedding::new(vec![0.0; 384], "all-MiniLM-L6-v2"),
            fulltext_query: Some("customer churn".into()),
            graph_constraints: None,
            fuse: FusionStrategy::default(),
            top_k: 25,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["top_k"], 25);
        assert_eq!(json["vector_query"]["model_id"], "all-MiniLM-L6-v2");
        assert_eq!(json["fuse"]["kind"], "reciprocal_rank_fusion");
    }

    #[test]
    fn weighted_sum_round_trips() {
        let f = FusionStrategy::WeightedSum {
            vector_weight: 0.7,
            fulltext_weight: 0.3,
        };
        let json = serde_json::to_string(&f).unwrap();
        let back: FusionStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }
}
