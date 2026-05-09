---
status: accepted
date: 2026-05-09
deciders: junyeong-ai
---

# ADR-0037 — Pure-Rust Leiden community detection

## Context

Φ10.1 substrate (ADR-0034) sketched a `CommunityAlgorithm`
enum with three variants (Leiden / Louvain / LabelPropagation),
anticipating an algorithm-pluggable cron. Φ10.5 bootstrap
auto-seeds a `default` policy per workspace. What was
missing: the **producer** — the cron that runs the algorithm
and writes `community_summaries` rows the GraphRAG retrieval
path consumes.

The implementation language and algorithm were the open
questions. Two language paths:

- **Pure Rust** — implement ourselves. No mature pure-Rust
  Leiden in crates.io today; paper-driven implementation.
- **Python sandbox** — embed `leidenalg` / `igraph` via PyO3
  or a sidecar service. Reference-quality algorithms, peer-
  reviewed. Adds Python runtime + native C deps to a
  Rust-only stack.

Two algorithm paths within pure Rust:

- **Phased rollout** — ship Label Propagation first (simple,
  always-available baseline), add Louvain / Leiden
  incrementally. The earlier draft of this ADR proposed this.
- **Direct Leiden commitment** — ship Leiden as the
  foundational algorithm. Microsoft GraphRAG canonical;
  Louvain's known disconnected-community pathology is fixed
  by Leiden's refinement phase.

## Decision

Pure Rust **Leiden** as the foundational and only community
detection algorithm. The platform commits to Leiden as the
production algorithm; the policy carries Leiden's tuning
surface (`resolution`, `seed`, `levels`, `min_cluster_size`)
directly rather than indirecting through an algorithm enum.

## Rationale

### Algorithm choice

- **Microsoft GraphRAG canonical.** The reference
  open-source GraphRAG stack converges on Leiden as the
  production algorithm. Phoenix Arize, scikit-network,
  Neo4j GDS all default to Leiden.
- **Connectivity guarantee.** Louvain's disconnected-community
  pathology — communities whose members are graph-isolated
  from each other — is fixed by Leiden's refinement phase
  (Traag-Waltman-van Eck 2019, §2.2). The dumbbell test in
  this ADR's implementation pins that guarantee.
- **Hierarchical aggregation.** Leiden recursively
  aggregates the refined partition into super-nodes,
  producing a multi-level hierarchy the GraphRAG retrieval
  path walks at the granularity it needs (broad → narrow).
  Flat algorithms (Label Propagation) don't expose this
  axis.
- **Deterministic with seed.** Local-moving + refinement
  consume `rngs::StdRng::seed_from_u64(policy.seed)`. Two
  runs against the same ontology version produce
  byte-identical partitions — the operator-trust contract.

### Language choice (pure Rust)

- **Ontology-scale graphs are tiny.** A workspace's
  schema-level graph carries hundreds of nodes (NodeType +
  EdgeType + GlossaryTerm + Concept + Segment).
  Single-thread iteration cost dominates; even Leiden's
  three-phase recursion is sub-second up to 10⁴ nodes.
- **No Python dependency surface.** Embedding `leidenalg`
  (Python + igraph C core) adds a Python runtime + native
  deps to a Rust-only stack — operationally
  disproportionate for a sub-second computation.
- **Algorithm well-defined.** Traag 2019 describes Leiden in
  ~10 pages of pseudocode. Implementation correctness is
  observable via Zachary's karate club + dumbbell synthetic
  benchmark; not theoretical.

### No algorithm enum

Single algorithm = no need for plug-in trait. The earlier
draft proposed a `CommunityDetector` trait + `DetectorRegistry`
to support the eventual addition of Louvain / Label Propagation;
that's premature flexibility (YAGNI). The platform commits to
Leiden as the canonical algorithm — if a future need surfaces
to plug in something else, the trait surface is one
straightforward refactor away. Today the cron calls
[`detect_communities(graph, &policy)`] directly.

## Architecture

`crates/ox-ontology/src/community_detection/`:

- `mod.rs` — module root.
- `graph.rs` — `CommunityGraph` (undirected weighted
  adjacency list) + `build_ontology_graph(&OntologyIR)`
  projection. Edge weights heuristic but stable
  (NodeType ↔ EdgeType 1.0, NodeType ↔ Concept 0.7,
  GlossaryTerm ↔ Concept 0.6, Segment ↔ NodeType 0.8).
- `leiden.rs` — Leiden algorithm: local moving / refinement
  / aggregation / recursion. Single entry point
  [`detect_communities`] consuming a graph + policy and
  returning a hierarchical [`DetectionResult`].

`crates/ox-store/migrations/0001_schema.sql` —
`community_detection_policies` carries `resolution DOUBLE
PRECISION`, `seed BIGINT`, `levels SMALLINT`,
`min_cluster_size INT` directly (no JSONB algorithm column).

`crates/ox-api/src/background/community_detection.rs`:
- `spawn_community_detection_sweep(store, pool, cancel)` —
  6-hour singleton-locked cron via
  `ADVISORY_LOCK_CRON_COMMUNITY_DETECTION`.
- Per-workspace fan: `WORKSPACE_ID.scope` per id.
- Per-workspace step: load policy + canonical IR → build
  graph → `detect_communities` → UPSERT `community_summaries`
  filtered by `policy.min_cluster_size`.
- Synthetic structural title + member listing as the summary
  body until LLM summarisation lands.

## Leiden algorithm specifics

Three phases per recursion level:

1. **Local moving.** Greedy modularity-gain sweep. For each
   node, evaluate ΔQ for moving into each neighbour
   community; take the move with largest positive gain.
   Repeat until no node moves in a full sweep.
2. **Refinement.** For each phase-1 community P, every node
   starts as a singleton sub-community; a constrained
   local-moving pass merges only with sub-communities the
   node is well-connected to (Traag 2019 §2.2). Result is a
   refinement of P that guarantees γ-connectedness — every
   sub-community is a connected sub-graph.
3. **Aggregation.** Collapse the refined partition into
   super-nodes; super-edges weighted by inter-community
   sums. Recurse with the aggregate graph.

Recursion stops when:
- partition has converged to singletons (no further
  meaningful merging), or
- `policy.levels` cap reached.

Reichardt-Bornholdt modularity:

```
Q(P) = (1/m) Σ_C [ e_C - γ (k_C / 2m)² · 2m ]
     = Σ_C [ e_C / m - γ (k_C / 2m)² ]
```

where `e_C` is intra-community weight, `k_C` is community
total degree, `m` is total edge weight, γ is
`policy.resolution`. γ = 1 recovers Newman 2006.

## Tests

Unit suite in `community_detection::leiden::tests`:
- `empty_graph_returns_typed_error` — `DetectionError::EmptyGraph`
  shape pin.
- `singleton_graph_produces_one_community_at_level_0` —
  trivial-case correctness.
- `disconnected_components_split_at_level_0` — two
  triangles, no bridge → two communities.
- `determinism_holds_across_runs_with_same_seed` —
  byte-identical partition reproducibility.
- `karate_club_recovers_known_structure` — Zachary 1977
  golden test: 2 ≤ communities ≤ 8 at level 0,
  modularity > 0.30, founder (0) ≠ officer (33).
- `refinement_produces_connected_communities_on_dumbbell` —
  pins Leiden's signature γ-separation guarantee on a
  graph that historically broke Louvain.

10 tests total covering the algorithm + the IR projection.

## Risks

| Risk | Mitigation |
|------|------------|
| Leiden self-implementation bug | Karate-club + dumbbell golden tests + determinism test pin signature properties |
| ontology-scale graphs grow beyond expectations | Leiden is O(E log V) — single-threaded sub-second up to 10⁴ nodes |
| LLM summarisation phase changes wire shape | UPSERT contract on `community_summaries` is unchanged — only the `summary` field's *content* swaps |
| Future need for an algorithm other than Leiden | Single-line refactor: lift `detect_communities` into a trait + registry — substrate cost amortised against actual need, not hypothetical |

## Related ADRs

- ADR-0034 — `RetrievalProfile` + `CommunityDetectionPolicy` data
- Φ10.5 — workspace bootstrap auto-seed (project session 2026-05-08i)
