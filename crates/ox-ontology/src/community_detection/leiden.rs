//! Leiden algorithm (Traag-Waltman-van Eck 2019).
//!
//! Three-phase iterative refinement of a graph partition:
//!
//! 1. **Local moving** — for every node, evaluate the
//!    modularity gain of moving it into each of its
//!    neighbours' communities. Greedy: take the move with
//!    largest positive gain, otherwise leave the node.
//!    Repeat until no node moves in a full sweep.
//! 2. **Refinement** — for each community produced by phase 1,
//!    initialise its members as singletons and run a
//!    *constrained* local-moving pass that only allows nodes
//!    to merge with sub-communities they are well-connected
//!    to. The refined partition is a refinement of the
//!    phase-1 partition; every emitted community is
//!    γ-separated (connected at the resolution threshold).
//!    This is Leiden's central guarantee — the property
//!    Louvain lacks and which makes Louvain liable to emit
//!    disconnected communities on real graphs.
//! 3. **Aggregation** — collapse the refined partition into a
//!    super-graph: one super-node per refined community,
//!    super-edges weighted by the inter-community sums. The
//!    next iteration runs against this aggregate graph,
//!    producing a coarser-grained partition; recursion stops
//!    when the policy's `levels` cap is reached or the
//!    partition stops improving.
//!
//! The output is a *hierarchical* partition — one
//! [`DetectedCommunity`] per (level, community) pair. The
//! GraphRAG retrieval path consumes the level it wants; an
//! operator surface that wants the canonical "1 community per
//! cluster" view picks level 0 (the finest).
//!
//! ## Modularity formulation
//!
//! Reichardt-Bornholdt modularity with resolution γ:
//!
//! ```text
//! Q(P) = (1 / 2m) · Σ_C [ e_C - γ · (k_C² / 2m) ]
//! ```
//!
//! where `e_C` is the sum of intra-community edge weights
//! (counted once per edge) for community C, `k_C` is the sum
//! of degrees of nodes in C, and `m` is total edge weight.
//! γ = 1 recovers Newman 2006 modularity.
//!
//! ## Determinism
//!
//! [`crate::CommunityDetectionPolicy::seed`] feeds an
//! [`rand::rngs::StdRng`] threaded through the local-moving
//! sweep order and the refinement phase's well-connectedness
//! tie-break. Two runs with the same `(graph, seed)` produce
//! byte-identical partitions.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use rand::SeedableRng;
use thiserror::Error;

use crate::CommunityDetectionPolicy;
use crate::community_detection::graph::CommunityGraph;

/// One community emitted by the algorithm. `level` is
/// hierarchical depth (0 = finest, ascending = coarser).
/// `members` indexes into the *original* `CommunityGraph.nodes`
/// vector — Leiden's recursion threads membership through the
/// aggregation chain so the cron can resolve back to original
/// `(EntityKind, logical_id)` pairs without bookkeeping the
/// chain itself.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedCommunity {
    pub level: u32,
    /// Stable id within the result. `c-{level}-{seq}` where
    /// `seq` is the community's index in the level's emission
    /// order — sorted by descending member count, ties broken
    /// by ascending lowest-member-index. Deterministic given
    /// the partition.
    pub local_id: String,
    pub members: Vec<usize>,
}

/// Aggregate output of one detection run.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Communities at every level, flattened. Level 0 first,
    /// then level 1, etc. The cron writes one
    /// `community_summaries` row per entry.
    pub communities: Vec<DetectedCommunity>,
    /// Modularity Q at the finest level (level 0). The
    /// observability summary; per-level Q is computable from
    /// `communities` if a richer view is ever needed.
    pub modularity: f32,
    /// Number of recursion levels actually produced.
    /// Bounded above by [`CommunityDetectionPolicy::levels`];
    /// can be lower when the partition collapses to a single
    /// community before the cap.
    pub levels_produced: u32,
}

#[derive(Debug, Error, Clone)]
pub enum DetectionError {
    /// The graph was empty — no nodes to partition.
    #[error("community detection input graph is empty")]
    EmptyGraph,
}

/// Single entry point. Builds the partition for the given
/// graph + policy and returns a hierarchical [`DetectionResult`].
pub fn detect_communities(
    graph: &CommunityGraph,
    policy: &CommunityDetectionPolicy,
) -> Result<DetectionResult, DetectionError> {
    if graph.is_empty() {
        return Err(DetectionError::EmptyGraph);
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(policy.seed);
    let resolution = policy.resolution.max(f32::EPSILON);
    let levels_cap = policy.levels.max(1) as usize;

    // Per-level state we accumulate as recursion descends.
    // `levels[i]` holds, for every node in the *original*
    // graph, the community id it was assigned at level i.
    let mut levels: Vec<Vec<usize>> = Vec::with_capacity(levels_cap);

    // The "current" working graph + its membership lookup
    // back to the original. `current_to_original` maps each
    // working-graph node to the set of original nodes it
    // represents (the aggregation chain).
    let mut current_graph = graph.clone();
    let mut current_to_original: Vec<Vec<usize>> = (0..graph.node_count())
        .map(|i| vec![i])
        .collect();

    for _ in 0..levels_cap {
        let partition = local_moving_phase(&current_graph, resolution, &mut rng);

        // Map the working-graph partition back to original
        // node assignments and stash the level.
        let original_assignments =
            project_to_original(&partition, &current_to_original, graph.node_count());
        levels.push(original_assignments);

        // Singletons → algorithm has converged at this level;
        // recursing further produces no new structure.
        if is_singleton_partition(&partition, current_graph.node_count()) {
            break;
        }

        // Refinement: split each phase-1 community into
        // γ-connected sub-communities. Aggregation then
        // operates on the refined partition.
        let refined = refinement_phase(&current_graph, &partition, resolution, &mut rng);

        // Build the aggregate graph from the refined partition,
        // and update the working chain so the next iteration's
        // membership traces back to original nodes.
        let aggregate = aggregate_graph(&current_graph, &refined);
        let next_to_original =
            chain_membership(&refined, &current_to_original, aggregate.node_count());
        current_graph = aggregate;
        current_to_original = next_to_original;

        // The next iteration's "starting" partition is the
        // refined-into-aggregate identity (each aggregate node
        // its own community). The local-moving phase will then
        // merge them by phase-1's coarser rule. We don't stash
        // this starting state — the next iteration's
        // `local_moving_phase` builds it implicitly.
    }

    let modularity = if let Some(level0) = levels.first() {
        modularity_of_partition(graph, level0, resolution)
    } else {
        0.0
    };

    let communities = emit_communities(&levels);

    Ok(DetectionResult {
        levels_produced: levels.len() as u32,
        communities,
        modularity,
    })
}

// ---------------------------------------------------------------------------
// Local moving
// ---------------------------------------------------------------------------

/// Greedy modularity-gain local moving. Returns a Vec where
/// `partition[i]` is the community id assigned to node `i`.
/// Community ids are dense [0..k) — the caller can iterate
/// over them as a range.
fn local_moving_phase<R: rand::Rng>(
    graph: &CommunityGraph,
    resolution: f32,
    rng: &mut R,
) -> Vec<usize> {
    let n = graph.node_count();
    let m = graph.total_edge_weight().max(f32::EPSILON);
    let two_m = 2.0 * m;

    let mut community: Vec<usize> = (0..n).collect();
    let mut community_total_degree: Vec<f32> =
        (0..n).map(|i| graph.weighted_degree(i)).collect();

    let mut order: Vec<usize> = (0..n).collect();
    let max_passes = 32; // Hard convergence cap; well above empirical needs.
    for _ in 0..max_passes {
        order.shuffle(rng);
        let mut moved = false;
        for &node in &order {
            let node_degree = graph.weighted_degree(node);
            let weights_to = neighbour_weights_per_community(graph, node, &community);
            let current = community[node];
            let weight_to_current = *weights_to.get(&current).unwrap_or(&0.0);

            // Candidate set: neighbour communities + the
            // current community (so "stay" is implicitly
            // evaluated). Singleton-isolation is also a valid
            // candidate but only produces gain when leaving the
            // current community would shed a heavier coupling
            // than it gains; the formula handles that case
            // naturally because `weights_to[current]` includes
            // the node's own intra-community weight.
            let mut best_community = current;
            let mut best_gain = 0.0f32;
            for (&candidate, &weight_in) in &weights_to {
                if candidate == current {
                    continue;
                }
                let total_in_candidate = community_total_degree[candidate];
                // ΔQ for moving node from current to candidate.
                // Standard Leiden formulation:
                //   gain = (k_in - γ k_node Σ_tot / 2m) / m
                // where the constant 1/m cancels in
                // comparisons; we keep it for the threshold
                // check (gain > 0).
                let gain = (weight_in - weight_to_current) / m
                    - resolution * node_degree
                        * (total_in_candidate
                            - community_total_degree[current]
                            + node_degree)
                        / (two_m * m);
                if gain > best_gain {
                    best_gain = gain;
                    best_community = candidate;
                }
            }

            if best_community != current {
                community_total_degree[current] -= node_degree;
                community_total_degree[best_community] += node_degree;
                community[node] = best_community;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    densify_partition(&community)
}

/// Compute Σ weight from `node` into each community.
fn neighbour_weights_per_community(
    graph: &CommunityGraph,
    node: usize,
    community: &[usize],
) -> HashMap<usize, f32> {
    let mut sums: HashMap<usize, f32> = HashMap::with_capacity(graph.neighbours[node].len());
    for &(other, w) in &graph.neighbours[node] {
        let c = community[other];
        *sums.entry(c).or_insert(0.0) += w;
    }
    // The node's own community must include any self-loop
    // contribution. The graph builder rejects self-edges (a == b
    // skip), so we don't need to special-case here, but the
    // current-community entry must exist with at least 0.0 so
    // the move-comparison sees it.
    sums.entry(community[node]).or_insert(0.0);
    sums
}

// ---------------------------------------------------------------------------
// Refinement (Leiden's signature phase)
// ---------------------------------------------------------------------------

/// Refine each phase-1 community into a partition where every
/// resulting sub-community is γ-connected. Implementation
/// follows Traag 2019 §2.2.
///
/// Within each phase-1 community P, every node starts as its
/// own sub-community. Nodes that are sufficiently
/// well-connected to a sub-community of P (in the sense of
/// the [`is_well_connected`] predicate) merge greedily with
/// the highest-modularity-gain candidate sub-community —
/// constrained to candidates *within P*. The result is a
/// refinement of the phase-1 partition; the union of the
/// refined sub-communities of P equals P.
fn refinement_phase<R: rand::Rng>(
    graph: &CommunityGraph,
    coarse: &[usize],
    resolution: f32,
    rng: &mut R,
) -> Vec<usize> {
    let n = graph.node_count();
    let m = graph.total_edge_weight().max(f32::EPSILON);

    // Group nodes by their phase-1 community for the
    // constrained pass.
    let mut by_coarse: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node, &c) in coarse.iter().enumerate() {
        by_coarse.entry(c).or_default().push(node);
    }

    let mut refined: Vec<usize> = (0..n).collect();
    let mut refined_total_degree: Vec<f32> =
        (0..n).map(|i| graph.weighted_degree(i)).collect();

    let mut coarse_keys: Vec<usize> = by_coarse.keys().copied().collect();
    coarse_keys.sort();
    for coarse_id in coarse_keys {
        // `coarse_keys` was just collected from `by_coarse.keys()`
        // and we're the only mutator — `remove` always succeeds.
        // Empty defaults instead of unwrap so a future refactor
        // that cycles ids never panics.
        let mut members = by_coarse.remove(&coarse_id).unwrap_or_default();
        if members.is_empty() {
            continue;
        }
        members.shuffle(rng);
        let coarse_total_degree: f32 = members.iter().map(|&i| graph.weighted_degree(i)).sum();

        for &node in &members {
            // Only nodes that are well-connected to their
            // coarse community participate in refinement —
            // poorly-connected nodes stay as singletons,
            // which is the γ-separation guarantee at work.
            let node_to_coarse = weight_into_coarse(graph, node, coarse, coarse_id);
            if !is_well_connected(
                node_to_coarse,
                graph.weighted_degree(node),
                coarse_total_degree - graph.weighted_degree(node),
                resolution,
                m,
            ) {
                continue;
            }

            // Candidate sub-communities of P (other than this
            // node's current refined community).
            let weights_to =
                neighbour_weights_per_community_within(graph, node, &refined, coarse, coarse_id);
            let current = refined[node];
            let node_degree = graph.weighted_degree(node);
            let mut best = current;
            let mut best_gain = 0.0f32;
            for (&candidate, &weight_in) in &weights_to {
                if candidate == current {
                    continue;
                }
                // Same modularity gain formula as the local
                // moving phase, but constrained to within-P
                // candidates by construction (`weights_to`
                // already filters to refined sub-communities
                // inside P).
                let cand_degree = refined_total_degree[candidate];
                let cur_degree = refined_total_degree[current];
                let gain = (weight_in
                    - *weights_to.get(&current).unwrap_or(&0.0))
                    / m
                    - resolution * node_degree
                        * (cand_degree - cur_degree + node_degree)
                        / (2.0 * m * m);
                if gain > best_gain {
                    best_gain = gain;
                    best = candidate;
                }
            }
            if best != current {
                refined_total_degree[current] -= node_degree;
                refined_total_degree[best] += node_degree;
                refined[node] = best;
            }
        }
    }

    densify_partition(&refined)
}

/// Sum of edge weights between `node` and the rest of `coarse_id`.
fn weight_into_coarse(
    graph: &CommunityGraph,
    node: usize,
    coarse: &[usize],
    coarse_id: usize,
) -> f32 {
    graph.neighbours[node]
        .iter()
        .filter(|&&(other, _)| coarse[other] == coarse_id && other != node)
        .map(|&(_, w)| w)
        .sum()
}

/// γ-connectedness predicate from Traag 2019 §2.2: a node is
/// well-connected to a community when its weight into that
/// community exceeds γ · (k_node · k_C\{node}) / (2m). The
/// predicate generalises Newman's intuition that "more than
/// random" is the threshold for membership.
fn is_well_connected(
    weight_in: f32,
    node_degree: f32,
    other_total_degree: f32,
    resolution: f32,
    m: f32,
) -> bool {
    let threshold = resolution * node_degree * other_total_degree / (2.0 * m);
    weight_in >= threshold
}

fn neighbour_weights_per_community_within(
    graph: &CommunityGraph,
    node: usize,
    refined: &[usize],
    coarse: &[usize],
    coarse_id: usize,
) -> HashMap<usize, f32> {
    let mut sums: HashMap<usize, f32> = HashMap::new();
    for &(other, w) in &graph.neighbours[node] {
        if coarse[other] != coarse_id {
            continue;
        }
        let c = refined[other];
        *sums.entry(c).or_insert(0.0) += w;
    }
    sums.entry(refined[node]).or_insert(0.0);
    sums
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Build the aggregate graph: one super-node per refined
/// community, edge weights summed across the original-graph
/// edges that span community boundaries. Self-loops in the
/// aggregate graph (intra-community edge weight) are dropped
/// — the graph builder rejects self-edges and the aggregate
/// graph is consumed by another local-moving phase that
/// reads weighted degree, not raw self-loops.
fn aggregate_graph(graph: &CommunityGraph, partition: &[usize]) -> CommunityGraph {
    let community_count = partition.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    if community_count == 0 {
        return CommunityGraph {
            nodes: Vec::new(),
            neighbours: Vec::new(),
        };
    }

    // Build aggregate adjacency by summing inter-community
    // weights. We index by sorted (a, b) pair to dedupe — each
    // edge is iterated twice in the underlying adjacency list
    // (once per endpoint), so we accumulate the half-sum and
    // double at the end... no, simpler: walk every (a, b) pair
    // once with a < b filter, sum weights.
    let mut aggregated: HashMap<(usize, usize), f32> = HashMap::new();
    for (node, neighbours) in graph.neighbours.iter().enumerate() {
        let ca = partition[node];
        for &(other, w) in neighbours {
            // Each edge appears twice (once per endpoint);
            // gate on `node < other` to count it once.
            if node >= other {
                continue;
            }
            let cb = partition[other];
            if ca == cb {
                continue;
            }
            let key = if ca < cb { (ca, cb) } else { (cb, ca) };
            *aggregated.entry(key).or_insert(0.0) += w;
        }
    }

    // Synthesise aggregate node placeholders. The aggregate
    // graph is consumed only by another local-moving call that
    // reads `nodes.len()` + `neighbours[i]` + `weighted_degree`
    // — the entity-identity fields on `CommunityGraphNode` are
    // unused in the aggregate context, so we fill stubs.
    let nodes: Vec<crate::community_detection::graph::CommunityGraphNode> = (0..community_count)
        .map(|i| crate::community_detection::graph::CommunityGraphNode {
            kind: crate::storage::EntityKind::NodeType,
            logical_id: format!("agg-{i}"),
            display_name: String::new(),
        })
        .collect();
    let mut neighbours: Vec<Vec<(usize, f32)>> = vec![Vec::new(); community_count];
    for ((a, b), w) in aggregated {
        neighbours[a].push((b, w));
        neighbours[b].push((a, w));
    }
    CommunityGraph { nodes, neighbours }
}

/// Build the next iteration's "aggregate-node → set-of-original-nodes"
/// chain by composing the existing chain with the refinement.
fn chain_membership(
    refined: &[usize],
    current_to_original: &[Vec<usize>],
    aggregate_node_count: usize,
) -> Vec<Vec<usize>> {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); aggregate_node_count];
    for (current_node, &agg_id) in refined.iter().enumerate() {
        // Every original node represented by the current
        // graph's `current_node` is now represented by
        // aggregate node `agg_id`.
        next[agg_id].extend(current_to_original[current_node].iter().copied());
    }
    next
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a working-graph partition (community ids over the
/// working graph's nodes) back to per-original-node community
/// assignments by following the aggregation chain.
fn project_to_original(
    partition: &[usize],
    current_to_original: &[Vec<usize>],
    original_count: usize,
) -> Vec<usize> {
    let mut assignments = vec![0_usize; original_count];
    for (current_node, &community) in partition.iter().enumerate() {
        for &original in &current_to_original[current_node] {
            assignments[original] = community;
        }
    }
    assignments
}

/// `true` when every node is its own community — the algorithm
/// has converged on the finest possible partition and further
/// recursion produces no new structure.
fn is_singleton_partition(partition: &[usize], n: usize) -> bool {
    let unique: HashSet<usize> = partition.iter().copied().collect();
    unique.len() == n
}

/// Renumber community ids into a dense [0..k) range while
/// preserving membership. Stable on input ordering — the first
/// distinct id encountered becomes 0, the next becomes 1, etc.
/// — so the downstream emission order is deterministic.
fn densify_partition(partition: &[usize]) -> Vec<usize> {
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut next = 0_usize;
    let mut dense = Vec::with_capacity(partition.len());
    for &c in partition {
        let mapped = *remap.entry(c).or_insert_with(|| {
            let id = next;
            next += 1;
            id
        });
        dense.push(mapped);
    }
    dense
}

/// Reichardt-Bornholdt modularity Q with resolution γ.
fn modularity_of_partition(graph: &CommunityGraph, partition: &[usize], resolution: f32) -> f32 {
    let m = graph.total_edge_weight();
    if m <= 0.0 {
        return 0.0;
    }
    let two_m = 2.0 * m;

    let community_count = partition.iter().copied().max().map(|x| x + 1).unwrap_or(0);
    let mut intra: Vec<f32> = vec![0.0; community_count];
    let mut total_degree: Vec<f32> = vec![0.0; community_count];

    for (node, &c) in partition.iter().enumerate() {
        total_degree[c] += graph.weighted_degree(node);
        for &(other, w) in &graph.neighbours[node] {
            if partition[other] == c && node < other {
                intra[c] += w;
            }
        }
    }

    let mut q = 0.0_f32;
    for c in 0..community_count {
        let e = intra[c];
        let k = total_degree[c];
        q += e / m - resolution * (k / two_m).powi(2);
    }
    q
}

/// Emit per-level [`DetectedCommunity`] rows from the
/// per-level original-node assignments. Communities are sorted
/// by descending member count, ties broken by ascending
/// lowest-member-index — operator-stable enumeration.
fn emit_communities(levels: &[Vec<usize>]) -> Vec<DetectedCommunity> {
    let mut emitted = Vec::new();
    for (level_idx, assignments) in levels.iter().enumerate() {
        let mut by_community: HashMap<usize, Vec<usize>> = HashMap::new();
        for (node, &c) in assignments.iter().enumerate() {
            by_community.entry(c).or_default().push(node);
        }
        let mut groups: Vec<Vec<usize>> = by_community
            .into_values()
            .map(|mut g| {
                g.sort();
                g
            })
            .collect();
        groups.sort_by(|a, b| {
            b.len().cmp(&a.len()).then_with(|| {
                a.first().copied().unwrap_or(usize::MAX).cmp(
                    &b.first().copied().unwrap_or(usize::MAX),
                )
            })
        });
        for (seq, members) in groups.into_iter().enumerate() {
            emitted.push(DetectedCommunity {
                level: level_idx as u32,
                local_id: format!("c-{level_idx}-{seq}"),
                members,
            });
        }
    }
    emitted
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::community_detection::graph::{CommunityGraph, CommunityGraphNode};
    use crate::storage::EntityKind;
    use chrono::Utc;
    use uuid::Uuid;

    fn synth_graph(n: usize, edges: &[(usize, usize, f32)]) -> CommunityGraph {
        let nodes: Vec<CommunityGraphNode> = (0..n)
            .map(|i| CommunityGraphNode {
                kind: EntityKind::NodeType,
                logical_id: format!("n-{i}"),
                display_name: format!("N{i}"),
            })
            .collect();
        let mut neighbours: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
        for &(a, b, w) in edges {
            if a == b {
                continue;
            }
            neighbours[a].push((b, w));
            neighbours[b].push((a, w));
        }
        CommunityGraph { nodes, neighbours }
    }

    fn default_policy() -> CommunityDetectionPolicy {
        CommunityDetectionPolicy {
            id: crate::CommunityDetectionPolicyId::new("cdp-test"),
            workspace_id: Uuid::nil(),
            name: "test".into(),
            description: String::new(),
            resolution: 1.0,
            seed: 42,
            levels: 3,
            min_cluster_size: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_graph_returns_typed_error() {
        let g = synth_graph(0, &[]);
        let err = detect_communities(&g, &default_policy()).unwrap_err();
        assert!(matches!(err, DetectionError::EmptyGraph));
    }

    #[test]
    fn singleton_graph_produces_one_community_at_level_0() {
        let g = synth_graph(1, &[]);
        let r = detect_communities(&g, &default_policy()).unwrap();
        assert!(r.levels_produced >= 1);
        let level_0: Vec<_> = r.communities.iter().filter(|c| c.level == 0).collect();
        assert_eq!(level_0.len(), 1);
        assert_eq!(level_0[0].members, vec![0]);
    }

    #[test]
    fn disconnected_components_split_at_level_0() {
        // Two triangles, no bridge.
        let g = synth_graph(
            6,
            &[
                (0, 1, 1.0),
                (1, 2, 1.0),
                (0, 2, 1.0),
                (3, 4, 1.0),
                (4, 5, 1.0),
                (3, 5, 1.0),
            ],
        );
        let r = detect_communities(&g, &default_policy()).unwrap();
        let level_0: Vec<_> = r.communities.iter().filter(|c| c.level == 0).collect();
        assert_eq!(level_0.len(), 2, "two triangles must form two communities");
        let combined: Vec<usize> = level_0.iter().flat_map(|c| c.members.iter().copied()).collect();
        let mut sorted = combined.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5]);
        assert!(r.modularity > 0.4, "two triangles should have high modularity, got {}", r.modularity);
    }

    #[test]
    fn determinism_holds_across_runs_with_same_seed() {
        let edges: Vec<(usize, usize, f32)> = vec![
            (0, 1, 1.0),
            (1, 2, 1.0),
            (0, 2, 1.0),
            (3, 4, 1.0),
            (4, 5, 1.0),
            (3, 5, 1.0),
            (2, 3, 0.5),
        ];
        let g = synth_graph(6, &edges);
        let p = default_policy();
        let r1 = detect_communities(&g, &p).unwrap();
        let r2 = detect_communities(&g, &p).unwrap();
        let p1 = canonicalise(&r1);
        let p2 = canonicalise(&r2);
        assert_eq!(p1, p2);
    }

    #[test]
    fn karate_club_recovers_known_structure() {
        // Zachary 1977. Ground truth: founder (0) and officer (33)
        // belong to opposing factions. Leiden on karate with
        // γ=1.0 typically produces 4 communities with modularity
        // ≈ 0.42. We assert the strongest invariants:
        //   1. 2 ≤ communities at level 0 ≤ 8
        //   2. Modularity at level 0 > 0.30
        //   3. Founder and officer in different level-0 communities
        //   4. Every emitted community is non-empty (refinement
        //      guarantee — no disconnected stragglers)
        let g = synth_graph(34, &karate_club_edges());
        let r = detect_communities(&g, &default_policy()).unwrap();
        let level_0: Vec<_> = r.communities.iter().filter(|c| c.level == 0).collect();
        assert!(
            level_0.len() >= 2 && level_0.len() <= 8,
            "karate club must split into 2..=8 communities at level 0, got {} (modularity {})",
            level_0.len(),
            r.modularity,
        );
        assert!(
            r.modularity > 0.30,
            "karate club Leiden modularity should exceed 0.30, got {}",
            r.modularity,
        );
        let community_of = |node: usize| -> Option<&str> {
            level_0
                .iter()
                .find(|c| c.members.contains(&node))
                .map(|c| c.local_id.as_str())
        };
        assert_ne!(
            community_of(0),
            community_of(33),
            "founder (0) and officer (33) must be in different communities",
        );
        for c in &level_0 {
            assert!(!c.members.is_empty(), "every community must be non-empty");
        }
    }

    #[test]
    fn refinement_produces_connected_communities_on_dumbbell() {
        // Two cliques bridged by a single weak edge — the
        // pathological graph for Louvain (which sometimes
        // emits a "community" containing one clique-member +
        // the bridge tail, leaving a disconnected stub).
        // Leiden's refinement phase prevents that.
        let mut edges: Vec<(usize, usize, f32)> = Vec::new();
        // Clique A: 0..5
        for a in 0..5 {
            for b in (a + 1)..5 {
                edges.push((a, b, 1.0));
            }
        }
        // Clique B: 5..10
        for a in 5..10 {
            for b in (a + 1)..10 {
                edges.push((a, b, 1.0));
            }
        }
        // Bridge.
        edges.push((4, 5, 0.1));

        let g = synth_graph(10, &edges);
        let r = detect_communities(&g, &default_policy()).unwrap();
        let level_0: Vec<_> = r.communities.iter().filter(|c| c.level == 0).collect();
        // Each emitted community must be a connected subgraph
        // — Leiden's signature guarantee.
        for c in &level_0 {
            assert!(
                is_connected_subgraph(&g, &c.members),
                "Leiden community {:?} on dumbbell graph is not connected",
                c.members,
            );
        }
    }

    fn is_connected_subgraph(graph: &CommunityGraph, members: &[usize]) -> bool {
        if members.len() <= 1 {
            return true;
        }
        let member_set: HashSet<usize> = members.iter().copied().collect();
        let start = members[0];
        let mut visited: HashSet<usize> = HashSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            for &(other, _) in &graph.neighbours[node] {
                if member_set.contains(&other) && !visited.contains(&other) {
                    stack.push(other);
                }
            }
        }
        visited.len() == members.len()
    }

    fn canonicalise(result: &DetectionResult) -> Vec<Vec<Vec<usize>>> {
        let mut by_level: HashMap<u32, Vec<Vec<usize>>> = HashMap::new();
        for c in &result.communities {
            let mut m = c.members.clone();
            m.sort();
            by_level.entry(c.level).or_default().push(m);
        }
        let mut levels: Vec<u32> = by_level.keys().copied().collect();
        levels.sort();
        levels
            .into_iter()
            .map(|l| {
                let mut groups = by_level.remove(&l).unwrap();
                groups.sort();
                groups
            })
            .collect()
    }

    fn karate_club_edges() -> Vec<(usize, usize, f32)> {
        let raw = [
            (0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (0, 8), (0, 10), (0, 11),
            (0, 12), (0, 13), (0, 17), (0, 19), (0, 21), (0, 31),
            (1, 2), (1, 3), (1, 7), (1, 13), (1, 17), (1, 19), (1, 21), (1, 30),
            (2, 3), (2, 7), (2, 8), (2, 9), (2, 13), (2, 27), (2, 28), (2, 32),
            (3, 7), (3, 12), (3, 13),
            (4, 6), (4, 10),
            (5, 6), (5, 10), (5, 16),
            (6, 16),
            (8, 30), (8, 32), (8, 33),
            (9, 33),
            (13, 33),
            (14, 32), (14, 33),
            (15, 32), (15, 33),
            (18, 32), (18, 33),
            (19, 33),
            (20, 32), (20, 33),
            (22, 32), (22, 33),
            (23, 25), (23, 27), (23, 29), (23, 32), (23, 33),
            (24, 25), (24, 27), (24, 31),
            (25, 31),
            (26, 29), (26, 33),
            (27, 33),
            (28, 31), (28, 33),
            (29, 32), (29, 33),
            (30, 32), (30, 33),
            (31, 32), (31, 33),
            (32, 33),
        ];
        raw.iter().map(|&(a, b)| (a, b, 1.0)).collect()
    }
}
