# Ontosyx Roadmap

Forward-looking view of the workbench. Finished items live in `git log`;
this document only describes the next-horizon concerns so a reader
doesn't have to reconstruct the future from the past.

For the 15-commit refactor that brought the workbench to its current
shape (Cypher AST + Rewriter/Validator pipelines, DataSourceAdapter +
IntrospectionKernel, PatternIR ↔ QueryIR split, GraphCanvas + NVL
removal), see the commit history from `7b6252e` through `d4f377f`.

For the follow-up deep-refactor wave that hardened the runtime around
that foundation — `CypherValidator` runtime integration, ontology
lineage naming, ArcSwap live-refresh, parse-once pipeline, JSONB
`schema_version` gate, RLS spawn-leak fixes, pluggable trait
`name()` unification — see the commit history from `110ea1d` through
`700e03c`.

## Current foundations

- **Runtime Cypher pipeline.** `ox-runtime::cypher` owns a partial AST
  plus two pipelines: `CypherRewriter` for injection passes (workspace
  isolation lives here; ACL / soft-delete / temporal slot in without
  forking the surface) and `CypherValidator` for inspection passes
  (safety gate, ontology conformance, post-rewrite scope check). Every
  cross-cutting Cypher concern plugs into one of these two pipelines.
- **Data-source layer.** `DataSourceAdapter` exposes five atomic
  primitives (`list_tables`, `describe_table`, `count_rows`,
  `sample_column`, `list_foreign_keys`). `IntrospectionKernel` owns
  retry / concurrency / caching / warning aggregation once; adapters
  never re-implement those concerns. Seven adapters on the same shape:
  PostgreSQL, MySQL, MongoDB, CSV, JSON, DuckDB, Snowflake, BigQuery.
- **Query IR family.** `QueryIR` is the DB-agnostic compile target.
  `PatternIR` is the canvas-facing shape that round-trips through
  `compile` (lossless) and `decompile` (best-effort — only
  `QueryOp::Match` fully reconstructs). `/api/query/pattern/*` exposes
  both transforms as pure endpoints.
- **Graph surfaces.** `GraphCanvas` is the shared XyFlow shell;
  `OntologyCanvas`, `QueryCanvas`, `ExploreCanvas` are its
  specializations. Legacy DIV / NVL surfaces are gone. Interaction
  hooks (`useGraphInteractions`, `useGraphContextMenu`,
  `useTypeFilter`) live in `web/src/lib/use-*.ts` and compose across
  every graph surface.

## Near-term

### Cypher pipelines

- **Future validators** slot into the `bolt::pipeline::run_pre_execute`
  pass: `ComplexityValidator` (reject obvious cartesian joins before
  they hit the DB), `AclValidator` (post-rewrite row-level policy check),
  `SoftDeleteRewriter` (inject tombstone predicates in the same pipeline
  position as workspace isolation). The trait surface is intentionally
  small so adding any of these is "new file, one `impl`, one pipeline
  registration."
- **Warnings / info surface.** `ValidationReport` already carries
  `Warning` / `Info` levels, but `run_pre_execute` currently drops
  non-error issues. Once a request-scoped progress channel reaches the
  runtime, validators can emit lower-severity diagnostics for UI
  tooltips without blocking execution.

### Adapter layer

- **Snowflake integration test harness.** The adapter compiles against
  `snowflake-api 0.12`; unit tests cover URL parsing / identifier
  validation / quoting helpers only. A compose-based integration suite
  that spins up a credentialed Snowflake workspace is out of scope
  today but belongs in the CI backlog.
- **BigQuery equivalent** — same gap. `gcp-bigquery-client 0.22` exposes
  Application Default Credentials directly; the integration test path
  is blocked only on a GCP project provisioned for CI.
- **Per-column PII sampling.** `sample_column` returns `ColumnStats`
  today. PII-aware sampling (masking emails in samples, redacting
  payment-card column values) would live inside `sample_column` itself
  so every downstream consumer inherits the guarantee.

### Query IR

- **Canvas layout persistence.** `PatternIR.layout_hints` and
  `PatternNode.position` exist on the wire; `/api/query/pattern/compile`
  drops them (intentional — QueryIR is canvas-agnostic). A saved-query
  endpoint that round-trips the *PatternIR* rather than the QueryIR
  would let users reopen an in-progress canvas with positions intact.
  API surface: a new resource, not a modification to `/compile`.
- **Temporal / as-of queries.** The Cypher rewriter pipeline is the
  right place for `as_of` injection (same position as workspace
  isolation); the PatternIR surface needs a new field plus a side
  panel. Both changes are additive — no migration pressure on today's
  call-sites.

### Graph surfaces

- **`useGraphInteractions` for OntologyCanvas.** OntologyCanvas still
  ships its own context-menu plumbing in `useCanvasContextMenu`. The
  shared hook could replace that with identical behaviour; the
  migration is only waiting on the canvas' own large state machine
  stabilising first.
- **ExploreCanvas worker layout.** ELK's `stress` algorithm runs on
  the main thread today. Graphs above ~100 nodes would benefit from
  the worker path the ontology canvas already uses; the blocker is
  the hardcoded layered options in `elk-layout.worker.ts` — a small
  refactor to parameterize algorithm per canvas.
- **Dashboard widget cross-filter.** `useTypeFilter` works per-widget
  today. A dashboard-scoped version (multiple widgets share a hidden
  set) is the natural next step for the dashboards surface.

## Out of scope

- **Neptune.** Kept deliberately. The runtime's AST is DB-agnostic in
  principle but every compiler / runtime / adapter shipped today
  targets Neo4j + Memgraph. Neptune support returns when a concrete
  use case arrives; the architecture supports it (new
  `GraphCompiler` + `GraphRuntime` implementations) but no speculative
  code lands ahead of demand.
- **Pulling NVL back in.** The removal was load-bearing — 62
  transitive packages, several ESM / SSR workarounds, two dynamic-
  import race conditions. XyFlow is now the only graph toolkit in
  the workbench; re-introducing a second one would require a concrete
  capability NVL provided that XyFlow doesn't, and no such capability
  is on the backlog today.
