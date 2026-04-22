# ADR 0004: Graph database as optional cache backend

- Status: Accepted
- Date: 2026-04-20

## Context

ADR 0001 commits Ontosyx to running queries over original sources. That
leaves the existing graph-DB integration (`ox-runtime` with Neo4j and
Memgraph) without a default role. Two wrong answers are available.

1. **Delete it.** Loses a real escape hatch: some customer workloads are
   fundamentally graph (5+ hop reachability, shortest path, community
   detection) and the source DB will not execute them efficiently.
2. **Keep it as a co-equal backend.** Recreates the migration story by
   the back door — users will be nudged toward "just copy everything to
   Memgraph" and we are back where we started.

Both are wrong because they treat the graph DB as a backend. It is a
**cache**.

## Decision

The graph database is an **optional cache backend**, not a primary
execution path.

- `ox-runtime` is repurposed: it implements a `GraphCacheBackend` trait
  defined in `ox-federation`. Its Cypher emitter remains, but as an
  alternative plan target the planner may choose — never as the
  default.
- Each `ObjectMappingDef` carries a `refresh_hint: CacheHintKind`:
  - `None` — never cache; always go to source.
  - `GraphCache { ttl: Duration, schedule: RefreshSchedule }` — the
    planner may route reads to the graph cache, subject to the TTL
    and schedule.
- The planner dispatches per-query:
  - If the plan is within the source's capability envelope and under
    cost budget → run on source.
  - If the plan requires graph-native ops (e.g. unbounded variable-
    length path) AND the involved mappings are `GraphCache`-hinted
    AND the cache is fresh → run on cache.
  - Otherwise → reject with a specific error
    (`UNSUPPORTED_PATH_ON_SOURCE` / `CACHE_STALE` / `CACHE_NOT_CONFIGURED`),
    never fall back silently.
- Cache refresh is **explicit**, not CDC:
  - Scheduled (`RefreshSchedule`) pulls from source through the same
    `TableProvider` scan path.
  - Manual ("pin to cache") is a privileged action with approval.
  - A schema drift (`SchemaDriftDef`) on the source invalidates the
    cache until reconciled.
- Neo4j / Memgraph deployment is **not required** for a functional
  Ontosyx install.

## Consequences

### Positive

- The deploy story simplifies: Postgres + Ontosyx is the floor; graph
  cache is a performance knob, not infrastructure.
- The graph-DB investment (Cypher emitter, runtime, rewriter
  pipeline) is preserved as an IR-level backend.
- Cache semantics are explicit. TTL + schedule + drift invalidation
  are written down; there is no pretense of real-time consistency.
- Costs align with use: customers who need the hot path pay for the
  graph DB; others do not.

### Negative

- Two execution paths require equivalence testing. We commit to
  result-parity tests (same QueryIR, run on source vs. cache, must
  match to within specified ordering).
- Cache staleness is user-visible. The UI shows "served from cache,
  updated N minutes ago" on affected results.
- Drift reconciliation has to be a workflow, not an afterthought — it
  becomes a first-class Phase 8 deliverable (`SchemaDriftDef` entity
  with a state machine).

### Trade-offs

- We trade "always-fresh everywhere" for "always-honest about
  freshness." A small surface of queries will serve from cache; the
  system declares when that happens and why.

## Alternatives considered

1. **Remove graph-DB support entirely.** Rejected — loses the hot-path
   escape hatch and wastes the existing investment.
2. **Graph-DB as equal backend, chosen by heuristic.** Rejected —
   silently routing between backends without an explicit cache
   contract reintroduces the sync-debt problem.
3. **CDC-based materialization.** Rejected as a Phase 1..10 scope;
   possible later but not required for the product thesis.

## Related

- ADR 0001 — VOL as first-class.
- ADR 0002 — DataFusion is the primary engine; this ADR is what happens
  when DataFusion is not the right fit.
- ADR 0003 — `ObjectMappingDef.refresh_hint` wires the policy.
- ADR 0009 — Partial-failure: `DegradedFromCache` is the only mode that
  implicitly uses the cache on source failure.
