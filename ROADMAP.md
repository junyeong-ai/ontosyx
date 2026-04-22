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
- **Federation (VOL) path.** `ox-federation` lowers a `QueryIR` to a
  DataFusion `LogicalPlan` via `build_query_ir_scoped` and executes
  it through `FederationContext::execute_plan`, emitting Arrow
  `RecordBatch`es. `POST /api/query/from-ir/federation` is the live
  HTTP surface; admin CRUD on `data_sources` (POST/GET/DELETE +
  `/refresh`) persists registrations in a workspace-scoped Postgres
  table and hydrates an `InMemoryAdapterResolver` per workspace on
  first query. Today's scan-ready adapters are CSV (top-level rows)
  and JSON (top-level records + single-level nested
  `records_<field>` tables). Link-mapping lowering covers the four
  variants: ForeignKey, Federated, Bridge, and multi-mapping UNION
  at seed position (including heterogeneous FK + Bridge branches in
  the same seed).

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

### Federation (VOL)

- **`LinkMappingKind::Computed` — per-dialect parsing.** Seed-
  position Computed ships today (DataFusion SQL dialect, via
  `SessionContext::parse_sql_expr`). Extend and close-cycle
  positions still refuse; source-dialect-specific syntax (PG
  `ILIKE`, Snowflake `PARSE_JSON`, MySQL backticks) also falls
  through the DataFusion parser with a descriptive error. A follow-
  up slice delegates parsing to the underlying adapter when the
  Computed edge carries source-pinned SQL.
- **New adapter kinds with scan().** Only CSV and JSON adapters
  materialise rows today. PostgreSQL / MySQL / Snowflake / BigQuery
  / DuckDB ship introspection but not scan. DuckDB specifically is
  blocked on an arrow-version upgrade (the workspace pins arrow 55
  via DataFusion 49; duckdb 1.x needs arrow 58). The upgrade bundles
  DataFusion + snowflake-api + gcp-bigquery-client together.
- **Extended secret-store backends for `data_sources.config`.** The
  `env:VAR_NAME` and `file:/path/to/secret` schemes work today via
  `EnvSecretResolver` + `FileSecretResolver` in `credential.rs`
  (the latter aimed at Kubernetes projected-volume mounts —
  reads the target file, trims trailing whitespace, refuses
  empty / relative-path references). Future schemes (`vault:`,
  `aws-sm:`, `gcp-sm:`) plug into `CompositeSecretResolver` via one
  `impl SecretResolver` + one `.register("<scheme>:", …)` call —
  the call-sites do not change.
- **Full axum handler test harness.** Pipeline-level tests cover
  every crate individually (federation planner, admin store CRUD,
  Arrow conversion); a test that constructs a hand-built `AppState`
  and exercises the HTTP handlers through `Router::oneshot` would
  pin the JSON wire format and the auth/ACL plumbing at the same
  time.
- **Frontend admin UI for federation adapters.** `/api/admin/federation/adapters`
  is curl-only today. The workbench needs a small registration form
  (source_id + kind + payload) and a listing panel in the admin
  surface.

### Graph surfaces

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
