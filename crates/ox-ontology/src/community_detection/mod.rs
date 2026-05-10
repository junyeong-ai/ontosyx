//! Community detection over the workspace's ontology graph.
//!
//! Φ10.4 — Leiden algorithm (Traag-Waltman-van Eck 2019), the
//! Microsoft GraphRAG canonical for hierarchical community
//! detection. The platform commits to Leiden as its
//! foundational algorithm; the policy carries Leiden's tuning
//! surface directly rather than indirecting through an
//! algorithm enum.
//!
//! ## Why Leiden (not Louvain, not Label Propagation)
//!
//! - **Microsoft GraphRAG default.** The reference open-source
//!   GraphRAG stack (graspologic, scikit-network, Neo4j GDS)
//!   converges on Leiden as the production algorithm.
//! - **Connectivity guarantee.** Louvain's well-known
//!   pathology — "disconnected communities" — is fixed by
//!   Leiden's refinement phase, which guarantees that every
//!   emitted community is a connected sub-graph. Without that
//!   guarantee, the GraphRAG retrieval path can surface a
//!   community whose members are graph-isolated from each
//!   other — visually and semantically incoherent.
//! - **Hierarchical aggregation.** Leiden recursively
//!   aggregates the refined partition into super-nodes,
//!   producing a multi-level hierarchy the retrieval path
//!   walks at the granularity it needs (broad → narrow). Flat
//!   algorithms (Label Propagation) don't expose this axis.
//! - **Deterministic with seed.** The local-moving and
//!   refinement phases consume an `rngs::StdRng` seeded from
//!   [`crate::CommunityDetectionPolicy::seed`]; two runs
//!   against the same ontology version produce byte-identical
//!   partitions.
//!
//! ## Why pure Rust
//!
//! Schema-level community graphs carry hundreds of nodes
//! (NodeType + EdgeType + GlossaryTerm + Concept + Segment).
//! Algorithm performance is dominated by single-thread
//! iteration cost on this scale; even Leiden's three-phase
//! recursion is sub-second up to 10⁴ nodes. Embedding
//! `leidenalg` (Python + igraph C core) would add a Python
//! runtime + native deps to a Rust-only stack — operationally
//! disproportionate for a sub-second computation.
//!
//! ## Architecture
//!
//! - [`graph`] — pure data: [`CommunityGraph`] (undirected
//!   weighted adjacency list) + [`build_ontology_graph`]
//!   projection. The projection is the only coupling between
//!   the algorithm and the IR.
//! - [`leiden`] — Leiden algorithm: local moving / refinement
//!   / aggregation / recursion. Single entry point
//!   [`detect_communities`] consuming a graph + policy and
//!   returning a hierarchical [`DetectionResult`].

pub mod graph;
pub mod leiden;

pub use graph::{CommunityGraph, CommunityGraphNode, build_ontology_graph};
pub use leiden::{DetectedCommunity, DetectionError, DetectionResult, detect_communities};
