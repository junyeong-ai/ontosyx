# ADR 0002: Apache DataFusion as the federation engine

- Status: Accepted
- Date: 2026-04-20

## Context

ADR 0001 commits Ontosyx to the Virtual Ontology Layer — queries run over
the original sources rather than a migrated graph DB. That decision needs
a concrete federation engine: something that accepts a logical plan,
talks to heterogeneous sources, pushes down predicates and projections,
streams results, and integrates into a Rust backend without FFI friction.

Candidates evaluated:

| Engine            | Runtime    | Notes |
|-------------------|------------|-------|
| Apache DataFusion | Rust       | Arrow-native, pluggable `TableProvider`, query optimizer, UDFs, streaming. Production use: InfluxDB 3, Delta Lake, Comet. |
| Apache Calcite    | JVM        | Mature optimizer, but JVM sidecar is operational baggage. |
| Trino / Starburst | JVM        | Best-in-class federation semantics, but out-of-process, 5–10× our ops surface. |
| DuckDB            | Embedded   | Superb as a single-process engine; federation via extensions is real but not first-class. |
| Polars            | Rust       | Excellent dataframe; query planner is not designed for federation. |
| Custom            | Rust       | NIH; loses the Arrow ecosystem. |

Ontosyx is a Rust workspace with tokio + axum + sqlx. In-process, Arrow-
native, open optimizer hooks, and `TableProvider` as the integration
surface are what matter.

## Decision

**Apache DataFusion is the federation execution engine.**

- Each data-source adapter exposes a `SourceTableProvider<A: DataSourceAdapter>`
  that implements DataFusion's `TableProvider` trait.
- The existing five introspection primitives (`list_tables`,
  `describe_table`, `count_rows`, `sample_column`, `list_foreign_keys`)
  stay put for ontology design; a new `scan(projection, filters, limit)`
  → `SendableRecordBatchStream` method on each adapter powers execution.
- `ox-federation::QueryPlanner` compiles `QueryIR` to a DataFusion
  `LogicalPlan` through the 14-stage pipeline documented in
  `architecture/6-axes.md`.
- User-Defined Functions (`FunctionDef`) register as DataFusion scalar /
  aggregate UDFs.
- Variable-length paths compile to source-native recursive CTE where
  supported; otherwise the planner routes to `GraphCacheBackend`
  (ADR 0004) or rejects with `UNSUPPORTED_PATH_ON_SOURCE`.
- Results stream out as Arrow `RecordBatch`es and flow into the existing
  SSE/WebSocket surface in `ox-api`.

## Consequences

### Positive

- Predicate and projection pushdown come for free per `TableProvider`.
- Arrow streaming maps 1-to-1 onto SSE batching; no intermediate
  materialization.
- UDF registration makes `FunctionDef` (derived properties) a compile
  target rather than a special case.
- The optimizer is open code we can influence; unlike a JVM sidecar we
  can add a bloom-join hint without forking a release train.
- Production use (InfluxDB 3, Delta-rs, Comet) means the stability
  bar is already high.

### Negative

- DataFusion evolves quickly; we pin a version per release cycle and
  schedule an upgrade sweep each cycle (Phase 7).
- Some operators (full-text, geospatial, path-specific) are either
  absent or nascent. We keep per-source pushdown where present and
  cache-fallback where absent.
- Cross-source hash joins are memory-bounded. The `CostEstimator` must
  flag plans whose join-side cardinality crosses a per-workspace
  threshold.

### Trade-offs made deliberately

- We accept an in-process engine. High-fanout analytical workloads that
  exceed one process will use the graph cache for the hot path or will
  pre-aggregate at the source; we explicitly do not scale DataFusion
  out in this phase.
- We accept the Rust/Arrow lock-in. Moving to a different engine later
  is a `TableProvider` rewrite, not a total re-architecture, because
  the planner produces engine-agnostic `QueryIR` first.

## Alternatives considered

1. **Trino sidecar.** Rejected — operational cost dominates benefit at
   our scale and it dilutes the "single Rust service" deploy story.
2. **DuckDB.** Retained as a local-development convenience (and as one
   of our adapters), but not as the federation engine: its cross-source
   story relies on extensions rather than a `TableProvider` trait.
3. **Hand-rolled planner on top of raw adapter `scan` calls.** Rejected
   — we would be re-implementing pushdown, join ordering, and streaming
   without getting the optimizer.

## Related

- ADR 0001 — VOL as first-class execution.
- ADR 0003 — Mapping as first-class (is what the planner resolves).
- ADR 0006 — SHACL Core as rule model (validators run around the plan).
- ADR 0009 — Partial-failure policy (required once federation is real).
