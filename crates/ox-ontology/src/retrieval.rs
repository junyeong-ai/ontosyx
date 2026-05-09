//! GraphRAG retrieval policy as typed first-class data.
//!
//! ## Why a typed policy
//!
//! Pre-Φ10 `query_graph` carried the retrieval shape as inline
//! literals scattered across `crates/ox-agent/src/tools/query_graph.rs`:
//! `expand_options.depth = 2`, `expand_options.max_nodes = 40`,
//! `search_entry_points(top_k=8)`,
//! `search_community_summaries(top_k=4)`. Two structural problems:
//!
//! 1. **Retrieval was hand-tuned, not evaluated.** Comparing
//!    "depth 2 with weight 1.0 on every edge" against
//!    "depth 3 with weight 2.0 on `OWNS` edges" required a code
//!    edit + redeploy. The eval surface (Φ8.3) could pin the
//!    fingerprint of a run but had no way to vary the retrieval
//!    policy because the policy wasn't data.
//! 2. **Edge types weighted equal.** Rare edges (`OWNS`) and
//!    high-fanout edges (`HAS_TAG`) carried identical hop
//!    priority. The literature (Microsoft GraphRAG paper,
//!    LightRAG dual-level) is unanimous that edge-type weighting
//!    is the single most impactful retrieval lever; Ontosyx had
//!    no way to express it.
//!
//! [`RetrievalProfile`] is the closed bundle every GraphRAG
//! invocation pins: per-edge-type weight matrix + traversal
//! strategy + limits. Stored, named, evaluable. Two profiles
//! identical in every field hash to the same fingerprint —
//! comparable RAGAS runs, deterministic replay.
//!
//! ## Orthogonal: community detection
//!
//! [`CommunityDetectionPolicy`] is a *separate* axis — it runs
//! offline as a cron sweep that emits `CommunitySummary` rows
//! (existing `community.rs` substrate). The retrieval profile
//! consumes those summaries at query time; the detection policy
//! produces them. Entangling the two would force an algorithm
//! change to coincide with a tariff change. Keep them split.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::eval_fingerprint::RetrievalProfileId;
use crate::ir::EdgeTypeId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`CommunityDetectionPolicy`].
    /// Workspace-scoped per natural key `(workspace_id, name)`.
    CommunityDetectionPolicyId
);

// ---------------------------------------------------------------------------
// Retrieval profile
// ---------------------------------------------------------------------------

/// Typed retrieval policy. One profile = one named answer to "how
/// should the graph be walked?".
///
/// Workspace-scoped per `(workspace_id, name)` UNIQUE key. The
/// platform supports an arbitrary count per workspace; `Perspective`
/// (a sibling FE concept) holds an `Option<RetrievalProfileId>`
/// FK so different ontology views can pin different retrieval
/// shapes (a Customer-centric perspective wants different edge
/// weights than a Product-centric one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct RetrievalProfile {
    pub id: RetrievalProfileId,
    pub workspace_id: Uuid,
    /// Workspace-unique human-readable name. Operators reference
    /// the profile by name on the FE; the id is stable across
    /// renames (rename = `update_retrieval_profile` preserving
    /// id, not `delete + create`).
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Per-edge-type weight overrides. Edges absent from the map
    /// inherit [`Self::default_edge_weight`]. Weights are
    /// non-negative `f32`; the traversal strategy interprets `0.0`
    /// as "skip this edge type" (effective edge type filter
    /// without a separate field).
    pub edge_weights: BTreeMap<EdgeTypeId, f32>,
    pub default_edge_weight: f32,
    pub traversal: TraversalStrategy,
    pub limits: RetrievalLimits,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RetrievalProfile {
    /// Build the workspace-default profile in memory. Returned
    /// values mirror the pre-Φ10 inline literals in
    /// `crates/ox-agent/src/tools/query_graph.rs::try_retrieve_subgraph_md`
    /// so wiring this fallback at the consumer is a behaviour-
    /// preserving swap — the FE / RAGAS scores stay identical to
    /// the pre-data-migration baseline, and operators that want
    /// to override only need `upsert_retrieval_profile` a row
    /// named `default`.
    ///
    /// Returned in-memory only — no persistence side effect. A
    /// future Φ10 phase wires a workspace-creation hook that
    /// auto-seeds the `default` row so the platform never falls
    /// back to this constant in practice; until then the consumer
    /// path tolerates "no row" silently and uses these defaults.
    pub fn workspace_default(workspace_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: RetrievalProfileId::new("rp-workspace-default"),
            workspace_id,
            name: "default".into(),
            description:
                "Workspace-default retrieval profile (in-memory fallback). \
                 Upsert a row named `default` to override."
                    .into(),
            edge_weights: BTreeMap::new(),
            default_edge_weight: 1.0,
            traversal: TraversalStrategy::Bfs { max_depth: 2 },
            limits: RetrievalLimits {
                max_nodes: 40,
                max_tokens: 1_500,
                anchor_top_k: 8,
                community_top_k: 4,
            },
            created_at: now,
            updated_at: now,
        }
    }

    /// Resolve the weight for `edge_type_id`. Returns the override
    /// when present, the workspace default otherwise. Negative
    /// overrides clamp to zero — the traversal layer rejects
    /// negative scores, and the platform never wants a "deeper
    /// than zero" edge.
    pub fn weight_for(&self, edge_type_id: &EdgeTypeId) -> f32 {
        let raw = self
            .edge_weights
            .get(edge_type_id)
            .copied()
            .unwrap_or(self.default_edge_weight);
        if raw < 0.0 || !raw.is_finite() {
            0.0
        } else {
            raw
        }
    }
}

/// Traversal strategy — the algorithm shape applied during
/// neighbor expansion. Closed enum: every variant maps to a
/// distinct backend code path in the GraphRAG retrieval emitter.
/// Adding a strategy is one variant + one emitter arm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraversalStrategy {
    /// Breadth-first up to `max_depth` hops. The conservative
    /// default — bounded fanout, deterministic ordering.
    Bfs { max_depth: u8 },
    /// Personalized PageRank — restart-augmented random walk.
    /// `restart_probability` (typically 0.15) controls how often
    /// the walk teleports back to the anchor; lower values
    /// explore farther, higher values stay close. `iterations`
    /// caps power-iteration count so a dense graph doesn't burn
    /// budget.
    Ppr {
        restart_probability: f32,
        iterations: u8,
        max_depth: u8,
    },
    /// Beam search — keep `width` highest-scoring candidates per
    /// hop, prune the rest. Useful for "find the strongest
    /// connection" queries where breadth-first is too noisy.
    BeamSearch { width: u8, max_depth: u8 },
}

impl TraversalStrategy {
    /// Discriminator string for telemetry / wire surfaces. Mirrors
    /// the `serde` rename so a JSONB column round-trip works.
    pub fn kind_str(&self) -> &'static str {
        match self {
            TraversalStrategy::Bfs { .. } => "bfs",
            TraversalStrategy::Ppr { .. } => "ppr",
            TraversalStrategy::BeamSearch { .. } => "beam_search",
        }
    }

    /// Hop ceiling for the strategy — the agent layer reads this
    /// to budget per-stage time. Each variant carries its own
    /// `max_depth`; the accessor avoids per-call match boilerplate.
    pub fn max_depth(&self) -> u8 {
        match self {
            TraversalStrategy::Bfs { max_depth }
            | TraversalStrategy::Ppr { max_depth, .. }
            | TraversalStrategy::BeamSearch { max_depth, .. } => *max_depth,
        }
    }
}

/// Closed budget for one retrieval invocation. Splitting
/// node-count + token-count is intentional — a graph that fits in
/// the node cap but blows the token budget after rendering still
/// gets truncated to the token ceiling. Both axes guard
/// independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct RetrievalLimits {
    /// Maximum nodes returned by neighbour expansion. Beyond this
    /// the retrieval layer truncates; the truncation is logged
    /// against the active `InferenceSession` so the operator can
    /// see when the budget bit.
    pub max_nodes: u32,
    /// Maximum tokens after the LLM-render pass. Smaller than
    /// `max_nodes × per-node-render` would imply because long
    /// labels / descriptions dominate token count.
    pub max_tokens: u32,
    /// Anchor top-k for `search_entry_points`. The first hop into
    /// the graph — too few misses relevant entry points, too many
    /// pollutes downstream traversal.
    pub anchor_top_k: u32,
    /// Community summary top-k for the global retrieval pass
    /// (Microsoft GraphRAG-style). `0` skips community fetch
    /// entirely — useful for narrow questions that don't benefit
    /// from a global summary.
    pub community_top_k: u32,
}

// ---------------------------------------------------------------------------
// Community detection
// ---------------------------------------------------------------------------

/// Detection policy — drives the offline cron that materialises
/// `CommunitySummary` rows the retrieval layer consumes.
/// Workspace-scoped per `(workspace_id, name)` UNIQUE.
///
/// Orthogonal to [`RetrievalProfile`] — a workspace can define
/// one detection policy and many retrieval profiles, or vice
/// versa. The eval surface fingerprints retrieval profile +
/// detection policy independently so an A/B over detection
/// algorithm doesn't perturb retrieval, and vice versa.
/// Workspace-scoped configuration for the Leiden community
/// detection cron (Microsoft GraphRAG canonical, Traag-Waltman-
/// van Eck 2019). The platform commits to Leiden as the
/// foundational community-detection algorithm; the policy carries
/// its tuning surface directly rather than indirecting through an
/// algorithm enum.
///
/// Operators upsert under `name = "default"` to override the
/// auto-seeded workspace row (Φ10.5); the bootstrap helper
/// [`Self::workspace_default`] returns the conservative defaults
/// new workspaces start with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct CommunityDetectionPolicy {
    pub id: CommunityDetectionPolicyId,
    pub workspace_id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Modularity resolution γ. Higher → more, smaller
    /// communities; lower → fewer, larger ones. Microsoft
    /// GraphRAG ships 1.0; experiments tune within [0.5, 2.0].
    pub resolution: f32,
    /// RNG seed for the local-moving and refinement phases.
    /// Two runs against the same ontology version with the same
    /// seed return byte-identical partitions — the operator
    /// trust contract.
    pub seed: u64,
    /// Hierarchical depth cap. Leiden recurses by aggregating
    /// the refined partition into super-nodes; `levels` bounds
    /// how deep the recursion goes. `1` means flat (single
    /// modularity-optimised partition); the platform default is
    /// 3 (Microsoft GraphRAG convention).
    pub levels: u8,
    /// Smallest cluster the cron retains as a published summary.
    /// Below this threshold the cluster is dropped from the
    /// emitted set at every level.
    pub min_cluster_size: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CommunityDetectionPolicy {
    /// Workspace-default Leiden policy in memory. Conservative
    /// tuning that fits the 10²-10³ node range a typical
    /// schema-level graph occupies: resolution 1.0, deterministic
    /// seed 42, 3 hierarchy levels, suppress clusters smaller
    /// than 2.
    ///
    /// Returned in-memory only — no persistence side effect.
    /// Workspace creation auto-seeds a `default` row using these
    /// constants (Φ10.5); operators upsert under the same name
    /// to override.
    pub fn workspace_default(workspace_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: CommunityDetectionPolicyId::new("cdp-workspace-default"),
            workspace_id,
            name: "default".into(),
            description:
                "Workspace-default Leiden community detection policy. \
                 Upsert a row named `default` to override."
                    .into(),
            resolution: 1.0,
            seed: 42,
            levels: 3,
            min_cluster_size: 2,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> RetrievalProfile {
        let mut weights = BTreeMap::new();
        weights.insert(EdgeTypeId::new("et-owns"), 2.0);
        weights.insert(EdgeTypeId::new("et-has_tag"), 0.3);
        RetrievalProfile {
            id: RetrievalProfileId::new("rp-default"),
            workspace_id: Uuid::nil(),
            name: "default".into(),
            description: String::new(),
            edge_weights: weights,
            default_edge_weight: 1.0,
            traversal: TraversalStrategy::Bfs { max_depth: 3 },
            limits: RetrievalLimits {
                max_nodes: 40,
                max_tokens: 1500,
                anchor_top_k: 8,
                community_top_k: 4,
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn weight_for_returns_override_when_present() {
        let p = sample_profile();
        assert_eq!(p.weight_for(&EdgeTypeId::new("et-owns")), 2.0);
        assert_eq!(p.weight_for(&EdgeTypeId::new("et-has_tag")), 0.3);
    }

    #[test]
    fn weight_for_falls_back_to_default() {
        let p = sample_profile();
        assert_eq!(
            p.weight_for(&EdgeTypeId::new("et-purchased")),
            p.default_edge_weight
        );
    }

    #[test]
    fn weight_for_clamps_negative_and_nan_to_zero() {
        let mut p = sample_profile();
        p.edge_weights.insert(EdgeTypeId::new("et-bad"), -1.0);
        p.edge_weights.insert(EdgeTypeId::new("et-nan"), f32::NAN);
        p.edge_weights.insert(EdgeTypeId::new("et-inf"), f32::INFINITY);
        assert_eq!(p.weight_for(&EdgeTypeId::new("et-bad")), 0.0);
        assert_eq!(p.weight_for(&EdgeTypeId::new("et-nan")), 0.0);
        assert_eq!(p.weight_for(&EdgeTypeId::new("et-inf")), 0.0);
    }

    #[test]
    fn traversal_strategy_kind_round_trips() {
        for s in [
            TraversalStrategy::Bfs { max_depth: 2 },
            TraversalStrategy::Ppr {
                restart_probability: 0.15,
                iterations: 10,
                max_depth: 4,
            },
            TraversalStrategy::BeamSearch {
                width: 8,
                max_depth: 3,
            },
        ] {
            let v = serde_json::to_value(&s).unwrap();
            assert_eq!(v.get("kind").and_then(|s| s.as_str()).unwrap(), s.kind_str());
            let back: TraversalStrategy = serde_json::from_value(v).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn traversal_max_depth_works_per_variant() {
        assert_eq!(TraversalStrategy::Bfs { max_depth: 5 }.max_depth(), 5);
        assert_eq!(
            TraversalStrategy::Ppr {
                restart_probability: 0.2,
                iterations: 8,
                max_depth: 7,
            }
            .max_depth(),
            7
        );
    }

    #[test]
    fn community_detection_policy_round_trips_through_json() {
        let p = CommunityDetectionPolicy {
            id: CommunityDetectionPolicyId::new("cdp-default"),
            workspace_id: Uuid::nil(),
            name: "default".into(),
            description: String::new(),
            resolution: 1.0,
            seed: 42,
            levels: 3,
            min_cluster_size: 2,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["resolution"].as_f64(), Some(1.0));
        assert_eq!(v["seed"].as_u64(), Some(42));
        assert_eq!(v["levels"].as_u64(), Some(3));
        let back: CommunityDetectionPolicy = serde_json::from_value(v).unwrap();
        assert_eq!(back.resolution, p.resolution);
        assert_eq!(back.seed, p.seed);
        assert_eq!(back.levels, p.levels);
    }

    #[test]
    fn retrieval_profile_round_trips_through_json() {
        let p = sample_profile();
        let v = serde_json::to_value(&p).unwrap();
        let back: RetrievalProfile = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.edge_weights, p.edge_weights);
        assert_eq!(back.traversal, p.traversal);
        assert_eq!(back.limits, p.limits);
    }
}
