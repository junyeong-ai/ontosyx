# ADR-0034 — `RetrievalProfile` + `CommunityDetectionPolicy` as typed first-class data

## Status

Accepted — Φ10.1+10.2, 2026-05-08.

## Context

Pre-Φ10 the GraphRAG retrieval shape lived as inline literals
scattered across `crates/ox-agent/src/tools/query_graph.rs`:

```rust
expand_options.depth = 2;
expand_options.max_nodes = 40;
search_entry_points(top_k = 8);
search_community_summaries(top_k = 4);
```

Two structural problems audit-flagged as P0:

1. **Retrieval was hand-tuned, not evaluated.** Comparing
   "depth 2 with weight 1.0 on every edge" against
   "depth 3 with weight 2.0 on `OWNS` edges" required a code
   edit + redeploy. The eval surface (Φ8.3 `EvaluationFingerprint`)
   could pin the fingerprint of a run but had no way to vary
   the retrieval policy because the policy wasn't data.
2. **Edge types weighted equal.** Rare edges (`OWNS`) and
   high-fanout edges (`HAS_TAG`) carried identical hop priority.
   The literature (Microsoft GraphRAG, LightRAG dual-level) is
   unanimous that edge-type weighting is the single most impactful
   retrieval lever; Ontosyx had no way to express it.

Φ8.3 had already declared `RetrievalProfileId` as a forward-compat
newtype in `EvaluationFingerprint` so eval runs could pin a
profile by id. This ADR ships the actual struct + persistence the
id refers to.

## Decision

Two typed structs in `ox-ontology`, two backing tables in
`ox-store`, two store traits.

**`RetrievalProfile`** (`crates/ox-ontology/src/retrieval.rs`) —
the closed bundle every GraphRAG invocation pins:

```rust
pub struct RetrievalProfile {
    pub id: RetrievalProfileId,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub edge_weights: BTreeMap<EdgeTypeId, f32>,
    pub default_edge_weight: f32,
    pub traversal: TraversalStrategy,
    pub limits: RetrievalLimits,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum TraversalStrategy {
    Bfs { max_depth: u8 },
    Ppr { restart_probability: f32, iterations: u8, max_depth: u8 },
    BeamSearch { width: u8, max_depth: u8 },
}

pub struct RetrievalLimits {
    pub max_nodes: u32,
    pub max_tokens: u32,
    pub anchor_top_k: u32,
    pub community_top_k: u32,
}
```

`weight_for(edge_type_id)` returns the per-edge override or the
default; negative + non-finite weights clamp to zero so the
traversal layer never sees a malformed score.

**`CommunityDetectionPolicy`** is the orthogonal axis — drives
the offline cron that materialises `ontology_community_summaries`
rows the retrieval layer consumes:

```rust
pub struct CommunityDetectionPolicy {
    pub id: CommunityDetectionPolicyId,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub algorithm: CommunityAlgorithm,
    pub levels: u8,
    pub min_cluster_size: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum CommunityAlgorithm {
    Leiden { resolution: f32, seed: u64 },
    Louvain { resolution: f32 },
    LabelPropagation,
}
```

Detection runs offline, retrieval consumes the result online.
Splitting the two policies lets a workspace A/B detection
algorithms independently from retrieval shape — an algorithm
change shouldn't coincide with a tariff change.

## Schema

Two workspace-scoped tables, 4-clause RLS, `(workspace_id, name)`
UNIQUE so operators reference profiles by name. JSONB for
extensible inner fields (edge weights map, traversal /
limits / algorithm parameter blobs); typed at the Rust layer.

```sql
CREATE TABLE retrieval_profiles (
    id TEXT PRIMARY KEY,
    workspace_id UUID NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    edge_weights JSONB NOT NULL,
    default_edge_weight DOUBLE PRECISION NOT NULL,
    traversal JSONB NOT NULL,
    limits JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, name),
    CONSTRAINT retrieval_profiles_default_edge_weight_non_negative
        CHECK (default_edge_weight >= 0)
);

CREATE TABLE community_detection_policies (
    id TEXT PRIMARY KEY,
    workspace_id UUID NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    algorithm JSONB NOT NULL,
    levels SMALLINT NOT NULL,
    min_cluster_size INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, name),
    CONSTRAINT cdp_levels_positive CHECK (levels >= 1),
    CONSTRAINT cdp_min_cluster_size_positive CHECK (min_cluster_size >= 1)
);
```

`RetrievalProfileStore` + `CommunityDetectionPolicyStore` traits
expose the canonical CRUD surface — `upsert_*` (natural key
UPSERT), `get_*`, `find_*_by_name`, `list_*`, `delete_*`. Both
land on the workspace `Store` supertrait + blanket impl.

## `BTreeMap<EdgeTypeId, f32>` — why ordered

`edge_weights` is `BTreeMap`, not `HashMap`, so JSONB
serialisation sorts keys deterministically. This matters for
the eval fingerprint digest (Φ8.3): two profiles with the same
weights but different insertion order must hash identically. The
trade-off is `Ord` on every id newtype — the
`define_id_newtype!` macro now derives `Ord` + `PartialOrd`
across the workspace.

## Consequences

- **Retrieval is data, not code.** Operators upsert a profile
  via the (future) admin route; query_graph reads the active
  profile by id and retrieves accordingly. A retrieval change
  is a write, not a redeploy.
- **Eval surface unblocked.** A run that pins
  `EvaluationFingerprint.retrieval_profile_id = Some(rp-foo)`
  now has a real row to resolve. RAGAS A/B over different
  retrieval shapes is one column on the run + a join.
- **Edge-type weighting expressible.** `OWNS` at 2.0,
  `HAS_TAG` at 0.3 lives in one row, fingerprinted at run time.
- **Detection orthogonal.** Switching algorithms (Leiden →
  Louvain) doesn't touch retrieval profile rows; the cron's
  next sweep emits `CommunitySummary` rows under the new
  algorithm + the existing retrieval profile consumes them
  unchanged.
- **Substrate-only ship.** Φ10.1+10.2 lands the types + tables
  + traits + impls. Φ10.3 wires `query_graph` to read the active
  profile (replacing inline literals). Φ10.4 ships the
  `CommunityDetectionCron`. Each follow-up is a self-contained
  consumer phase.

## Alternatives considered

- **Unify retrieval + detection into one policy struct.**
  Rejected — algorithm + levels are detection-time concerns
  (offline, batched), edge weights + traversal are query-time
  concerns (online, per-call). Tying them forces an algorithm
  change to coincide with a tariff change.
- **Hardcode a default profile in code.** Rejected — that's
  exactly the shape Φ10 exists to replace. The audit's
  fundamental complaint is that retrieval-as-code defeats the
  eval-as-data invariant.
- **Skip the typed enum + use free-form JSON.** Rejected —
  the closed enum gives the runtime emitter a deterministic
  match for backend dispatch (Bfs → expand_neighbors,
  Ppr → graph algorithm crate, BeamSearch → custom). Free-form
  JSON re-introduces "is this a known algorithm?" runtime
  validation we already paid for at the type system.
- **Persist `EdgeTypeId` weights as a separate
  `retrieval_profile_edge_weights` table.** Rejected — JSONB
  serialisation order is deterministic with `BTreeMap` (the
  fingerprint requirement) and the read pattern is "load whole
  profile" not "look up one edge weight". Normalising to a
  side table would force a JOIN every retrieval call without
  buying anything.

## Φ10.3 — `query_graph` consumer wiring (landed 2026-05-08h)

`crates/ox-agent/src/tools/query_graph.rs::try_retrieve_subgraph_md`
now resolves the active retrieval profile via a new
`resolve_retrieval_profile(domain)` helper:

1. `store.find_retrieval_profile_by_name("default")` →
   `Some(profile)` when the workspace has authored one.
2. Lookup miss / DB error → `RetrievalProfile::workspace_default(workspace_id)`
   in-memory factory. Mirrors the pre-Φ10 inline literals
   (`depth=2`, `max_nodes=40`, `anchor_top_k=8`,
   `community_top_k=4`, `max_tokens=1500`) so the fallback is
   behaviour-preserving.

Four hardcoded literals removed in the same sweep:

- `EntryPointSearchOptions::new(version_id, question, 8)` →
  `profile.limits.anchor_top_k`.
- `search_community_summaries(version_id, question, 4)` →
  `profile.limits.community_top_k`.
- `expand_options.depth = 2` →
  `profile.traversal.max_depth()`.
- `expand_options.max_nodes = 40` /
  `LlmRenderOptions { max_nodes: 40, max_tokens: Some(1_500) }`
  → `profile.limits.max_nodes` / `profile.limits.max_tokens`.

`grep "depth\s*=\s*[0-9]\|max_nodes\s*=\s*[0-9]\|top_k\s*=\s*[0-9]"
crates/ox-agent/src/tools/query_graph.rs` returns **0** rows.
The retrieval-as-code pattern is gone.

## Φ10.5 — Workspace bootstrap auto-seed + Perspective FK (landed 2026-05-08i)

Two follow-ups land in Φ10.5:

1. **Auto-seed on workspace creation.**
   `WorkspaceStore::create_workspace` impl now runs in two
   stages: (a) INSERT the workspaces row, (b) inside an
   explicit `WORKSPACE_ID.scope(new_id, ...)` block, upsert a
   `default` retrieval profile + a `default` community
   detection policy via `RetrievalProfile::workspace_default()`
   / `CommunityDetectionPolicy::workspace_default()`. Every
   new workspace lands with persistent rows that match the
   in-memory fallback exactly — `query_graph`'s
   `find_retrieval_profile_by_name("default")` now hits a row
   on the happy path. The fallback is reachable only on a
   degraded read (RLS denial / DB outage), as a graceful-
   degradation last resort.

2. **`WorkbenchPerspective.retrieval_profile_id` FK.**
   `Option<RetrievalProfileId>` on the struct +
   `retrieval_profile_id text` column on
   `workbench_perspectives` + `ON DELETE SET NULL` FK
   constraint to `retrieval_profiles(id)`. The FK is added via
   a forward-reference `ALTER TABLE` clause after
   `retrieval_profiles` is defined later in the baseline
   (DDL execution stays linear). Different perspectives can
   now pin different retrieval shapes — Customer-centric
   perspective wants different edge weights than a
   Product-centric one.

`UpsertPerspectiveRequest` gains an optional
`retrieval_profile_id: Option<String>` field so the FE can
choose the perspective's profile at save time. Existing
perspectives that don't supply one keep `retrieval_profile_id =
None` and fall back to the workspace default at retrieval
time.

`CommunityDetectionPolicy::workspace_default(workspace_id)`
factory: Leiden at resolution 1.0 + seed 42 (deterministic
re-runs against the same graph), 3 hierarchy levels,
`min_cluster_size = 4` to suppress singletons.

## Outstanding (next phases)

- **Φ10.4 — `CommunityDetectionCron`.** Singleton-locked cron
  reads each workspace's policy + runs the algorithm over the
  current canonical ontology version → emits
  `ontology_community_summaries` rows. Replaces the
  manual-upsert path the existing community store carries.
  External dep decision: Rust `graphalgs` / `petgraph-clu`
  crate vs Python sandbox via `graspologic`. Pending.

## References

- `crates/ox-ontology/src/retrieval.rs`
- `crates/ox-store/src/store/retrieval.rs`
- `crates/ox-store/src/store/community_policy.rs`
- `crates/ox-store/src/postgres/retrieval.rs`
- `crates/ox-store/src/postgres/community_policy.rs`
- `crates/ox-store/migrations/0001_schema.sql` — retrieval +
  community_detection sections
- ADR-0032 — `EvaluationFingerprint` (the surface that pins
  this profile by id for reproducibility)
- `crates/ox-store/src/community.rs` — existing
  `CommunitySummary` substrate (the cron's output the retrieval
  layer consumes)
