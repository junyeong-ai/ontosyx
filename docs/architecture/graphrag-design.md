# GraphRAG — community detection + hybrid retrieval

**Status:** Design sketch — Phase 7 of the long-horizon
work plan, **gated on instance-graph volume**. The trait
+ schema + integration points are documented here so the
next session can land the implementation on a workspace
that crosses the volume threshold without re-deriving the
contract.

## Volume gate

The first sentence of this design exists because GraphRAG's
community-detection layer adds value **only above a volume
threshold** (industry rule of thumb: ~100K nodes per
workspace). Below threshold:

- Communities are degenerate — Leiden / Louvain produces
  one giant cluster + a long tail of singletons. Summaries
  over those clusters carry less information than the raw
  schema RAG already surfaces.
- Storage + cron costs are real (per-cluster summary LLM
  calls, periodic re-clustering on instance-data churn)
  but the LLM gets no usable signal back.
- Operators on small graphs see "Microsoft GraphRAG-style
  global search" produce worse answers than the existing
  schema-grounded `translate_query` because the summary
  layer adds noise without recall.

So the v1 implementation ships behind a per-workspace
**feature flag** (`workspaces.graphrag_enabled: bool`,
default `false`) and a startup-time threshold check
(`SELECT count(*) FROM <node_table>` > threshold) that
refuses to enable the flag for under-volume workspaces.
Operators on large graphs opt in deliberately.

## Decision (sketch)

Three deliverables, each independently shippable:

### 1. Community-detection sweep + summary layer

A new cron task runs Leiden (Memgraph
`community_detection.get`) or Louvain (Neo4j GDS
`gds.louvain.write`) over the instance graph. Per cluster:

- compute degree / size statistics,
- pick representative entities (top-degree nodes),
- LLM-summarise the cluster's structural shape +
  representative entities into a short paragraph,
- persist the summary in a new
  `graph_community_summaries` table.

Schema (workspace-scoped, four-clause RLS per
`ox-store::CLAUDE.md`):

```sql
CREATE TABLE graph_community_summaries (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid,
    ontology_lineage_id text NOT NULL,
    cluster_key   text NOT NULL,                -- the algorithm-assigned cluster id
    parent_key    text,                          -- hierarchical clustering: pointer up
    member_count  integer NOT NULL,
    representative_node_ids text[] NOT NULL,
    summary       text NOT NULL,
    summary_render_hash text NOT NULL,           -- prompt-fingerprint for replay
    summarised_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, ontology_lineage_id, cluster_key)
);
ALTER TABLE graph_community_summaries ENABLE ROW LEVEL SECURITY;
ALTER TABLE graph_community_summaries FORCE ROW LEVEL SECURITY;
-- + ws_isolation + system_bypass policies per the canonical pattern
```

The cron task wraps in `try_advisory_lock`
(`ADVISORY_LOCK_CRON_GRAPHRAG_SUMMARY` per ADR-0024) so
multi-replica deploys run the sweep on one instance.

### 2. `QueryOp::HybridSearch` IR variant

A new `QueryOp` variant on `QueryIR`:

```rust
QueryOp::HybridSearch {
    /// Vector query — embeddings of the operator's
    /// question (or a sub-question from the
    /// translation pipeline).
    vector_query: Embedding,
    /// Lexical full-text query — narrows the candidate
    /// pool before vector ranking.
    fulltext_query: Option<String>,
    /// Optional graph-traversal predicate — restricts
    /// the candidate pool to nodes satisfying a
    /// pattern (e.g. "within 2 hops of customer X").
    graph_constraints: Option<PatternIR>,
    /// Fusion strategy — RRF (Reciprocal Rank Fusion)
    /// is the v1 default; weighted-sum is a future
    /// option once the cost model has the data to
    /// pick weights.
    fuse: FusionStrategy,
    /// Top-k retrieval count.
    top_k: u32,
}
```

The compiler lowers `HybridSearch` to:

- **Neo4j** —
  `db.index.vector.queryNodes` + `db.index.fulltext.queryNodes`
  + UNION + per-side rank + RRF score.
- **Memgraph** — symmetric (`vector_search.search` +
  `text_search.search` + same fusion).

The vector index DDL `cypher/schema.rs:315` already
emits CREATE VECTOR INDEX statements. Today the reader
side is dead code (per the revised plan's audit); this
variant ships the matching reader.

### 3. Retrieval mode dispatcher

The existing `translate_query` path becomes one of three
**retrieval modes** the new `RetrievalRouter` picks
between:

- **Local** (default for any workspace) — the existing
  `translate_query` path. Schema-RAG → `QueryIR` →
  Cypher / Federation execution. Best for "show me
  this entity / aggregate this set" intents.
- **Global** (gated on `graphrag_enabled`) — read the
  community summaries directly. Best for "what themes
  / clusters dominate this subgraph" intents that don't
  resolve to a single typed pattern. The mode emits a
  text answer assembled from the top-k summary rows
  selected by relevance to the question.
- **Hybrid** (gated on `graphrag_enabled`) — emit a
  `HybridSearch` `QueryOp` and execute through the
  matching graph runtime. Best for "find entities most
  semantically similar to X within these constraints"
  intents.

The router runs *before* `translate_query`; when it picks
Local, the existing path runs unchanged. The
classification is a small Brain method
(`classify_retrieval_mode(question, ontology)`) that
reuses the existing schema-RAG payload + an additional
"available retrieval modes" prompt slot.

## Integration points

- **No change to `QueryIR` consumers** for the Local
  path. Hybrid path consumers route through the new
  `QueryOp::HybridSearch` arm; existing exhaustive
  matches will fail compile, surfacing every code-path
  that needs an arm — exactly what we want for an IR
  extension.
- **PlanRouter** (per the matching architecture sketch)
  inspects `HybridSearch` and routes to the graph
  runtime by default; future cost-model can route to
  a federation-side ANN index when one ships.
- **Schema-RAG payload** gains a "graph community
  summaries" slot when `graphrag_enabled`. The Brain's
  prompt budget (per ADR-0028 / 0029, deferred) trims
  the summary list to the per-question token budget.
- **Provenance** — every Hybrid retrieval emits a
  `prov:Activity` with `kind: HybridRetrieval` so the
  audit trail captures which fusion strategy ran +
  which community summaries informed the answer.

## Test pyramid

- **Unit tests on the fusion strategy** — RRF
  reproducibility on synthetic candidate lists.
- **Integration tests in `ox-graph-runtime/tests/`** —
  fire `HybridSearch` against a Memgraph fixture with
  vector + fulltext indexes seeded; assert the
  ranked output matches the expected order.
- **Eval golden cases** — extend
  `tests/golden/nl2cypher.golden.json` with retrieval-
  mode cases (`expected_retrieval_mode: "global" |
  "hybrid"`) so the Global / Hybrid routing decisions
  are guarded by the eval gate.
- **Volume guard test** — assert that
  `graphrag_enabled` cannot be set on a workspace below
  threshold (the API endpoint refuses with a typed
  `ApiErrorCode::GraphRagInsufficientVolume` error
  per ADR-0017).

## Out of scope (v1)

- **Multi-modal indexes** — image / table / time-series
  embeddings alongside text. The ANN index abstraction
  generalises but v1 ships text only.
- **Real-time community re-detection** — the cron
  cadence is the only refresh trigger; a per-write
  delta-clustering path is a Phase 9-class follow-up.
- **PropertyGraphIndex from instance data** (LlamaIndex
  pattern) — the IR + community summary layers cover
  the dominant use cases; the per-instance index is a
  future cost-model decision.

## References

- Microsoft GraphRAG —
  <https://github.com/microsoft/graphrag>
- Neo4j GraphRAG —
  <https://neo4j.com/labs/genai-ecosystem/graphrag>
- LlamaIndex `PropertyGraphIndex`
- ADR-0001 / 0002 — VOL + DataFusion (the Hybrid path
  composes with federation when sources allow it).
- ADR-0017 — Typed error wire shape (the
  `GraphRagInsufficientVolume` typed code).
- ADR-0023 — `HeuristicProposal` (community-summary
  re-runs land here when the cron sweep detects
  drift).
- ADR-0024 — Advisory lock (the cron singleton key).
- `tests/golden/nl2cypher.golden.json` — eval dataset
  the routing decisions ride on.
- Phase 7 of the long-horizon plan (gated on volume
  per the revised plan's recommendation).
