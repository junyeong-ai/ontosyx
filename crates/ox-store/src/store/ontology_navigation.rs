//! Progressive-Disclosure navigation over the Level-3 flat indexes.
//!
//! Backed by the Level 3 materialised indexes in the schema
//! baseline. Every method is version-scoped — the caller picks which
//! ontology version to navigate, matching the temporal-rewriter
//! contract.
//!
//! The four core flows:
//!
//!   1. Entry discovery — user types "orders" → `search_entry_points`
//!      returns ranked hits (fuzzy + full-text + semantic).
//!   2. Expansion — from a selected entity, `expand_neighbors` yields
//!      the 1-hop cross-references (Property→ValueSet, etc).
//!   3. Hierarchy — `walk_hierarchy` traverses closure tables
//!      (CodeSystem broader, GlossaryTerm parent, Interface
//!      implements) in O(1).
//!   4. Similarity — `similar_to` uses the `entity_embedding` HNSW
//!      index for semantic kNN.
//!
//! The trait is separate from [`super::OntologyVersionStore`] because
//! (a) navigation is a read-only surface even when versioning is in
//! play, (b) a future split where the navigation store is a
//! read-replica / cached view stays clean.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::navigation::{
    EntitySearchHit, EntryPointSearchOptions, HierarchyFacetOptions, LlmRenderOptions,
    NeighborExpandOptions, Subgraph,
};

/// Progressive-Disclosure navigation surface. Each method
/// corresponds to one step of the search → expand → filter →
/// render pipeline so the layered usage is clear at the trait
/// level.
///
/// Options structs in [`crate::navigation`] keep the signatures
/// parameter-rich without adding positional bloat. `Subgraph` is the
/// shared value moved through steps 2 + 3 so a caller can chain
/// without re-allocating.
#[async_trait]
pub trait OntologyNavigationStore: Send + Sync {
    /// Step 1 — anchor search. Blended trigram + full-text + embedding
    /// scoring over the searchable document. Returned hits are sorted
    /// by `score` descending; the caller picks the top-K as anchors
    /// for `expand_neighbors`.
    async fn search_entry_points(
        &self,
        options: EntryPointSearchOptions,
    ) -> OxResult<Vec<EntitySearchHit>>;

    /// Step 2 — BFS from a batch of anchors, depth-limited. Returns a
    /// single [`Subgraph`] aggregating every reachable node / edge.
    /// Sets `Subgraph.truncated` when `max_nodes` trimmed the
    /// frontier.
    async fn expand_neighbors(&self, options: NeighborExpandOptions) -> OxResult<Subgraph>;

    /// Step 3 — merge hierarchy closure into an existing subgraph and
    /// optionally filter by facet. Called on the result of step 2;
    /// returns the mutated subgraph so the caller can chain or
    /// snapshot independently of the input.
    async fn apply_hierarchy_and_facet(
        &self,
        subgraph: Subgraph,
        options: HierarchyFacetOptions,
    ) -> OxResult<Subgraph>;

    /// Step 4 — render the subgraph as markdown suited to the LLM
    /// prompt tail. Pure function; does not touch the store beyond
    /// needing `&self` for trait-object erasure.
    fn render_subgraph_for_llm(&self, subgraph: &Subgraph, options: &LlmRenderOptions) -> String;

    /// Semantic kNN over the Level-3 embedding index. Returns empty
    /// when the target entity has no embedding yet (cold row —
    /// background populator hasn't caught up). Surfaced separately
    /// from `search_entry_points` because the caller typically wants
    /// either anchor search (blend) *or* pure semantic neighbourhood
    /// — not both at once.
    async fn similar_entities(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
        top_k: u32,
    ) -> OxResult<Vec<EntitySearchHit>>;
}
