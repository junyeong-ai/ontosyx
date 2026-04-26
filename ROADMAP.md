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
  plus two pipelines: `CypherRewriter` for injection passes
  (`WorkspaceScopeRewriter` → `AclRewriter` (Deny + Mask) →
  `SoftDeleteRewriter` → `Custom`, sorted by `RewritePhase`) and
  `CypherValidator` for inspection passes (`SafetyValidator` with the
  reserved `system_properties` write guard, ontology conformance,
  post-rewrite scope check). Every cross-cutting Cypher concern plugs
  into one of these two pipelines.
- **ACL enforcement.** `acl_policies` rows project onto an
  `AclSnapshot` loaded once per request inside the
  `workspace_context` middleware and threaded through
  `GRAPH_ACL_SNAPSHOT` + `GRAPH_PRINCIPAL` task-locals. The Cypher
  pipeline's `AclRewriter` reads the snapshot and rewrites Deny
  (`WHERE false` injection) + Mask (direct, chained, and bare-var
  projections rewritten to Cypher map projections). DataFusion
  federation post-processes `enforce_acl_on_result` because the
  LogicalPlan executes outside the Cypher pipeline.
- **Soft-delete + retention.** `SoftDeleteRewriter` injects
  `<var>._deleted_at IS NULL` on every read and rewrites
  `DELETE` / `DETACH DELETE` to `SET <var>._deleted_at = timestamp()`,
  with `DETACH DELETE` additionally hard-detaching edges so traversals
  don't dead-end on tombstoned nodes. A daily retention compactor
  hard-deletes rows whose tombstone is older than the configured
  cutoff (default 90 days).
- **PII surface.** `ox-ontology::pii` is the single source of truth:
  `PiiClassifier` suggests annotations from column signals,
  `PiiAnnotation` carries operator-confirmed kinds into ontology
  design, `PropertyDef.pii_kind` is the runtime annotation, and
  `redact_value` / `redact_column_stats` apply deterministic shape-
  preserving redaction (email keeps the local prefix + TLD, numeric-
  tail kinds keep the trailing 4, secrets become `<redacted>`).
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

- **Temporal / as-of queries.** The Cypher rewriter pipeline already
  sorts a `RewritePhase::Temporal=400` slot above `SoftDelete`; the
  matching `TemporalRewriter` lands when the PatternIR surface
  exposes the as-of pivot via a side panel.
- **DataFusion-side ACL pre-execute.** Federation paths still
  post-process `enforce_acl_on_result`. A pre-execute hook on the
  DataFusion plan that mirrors the Cypher rewriter shape would let
  the federation path drop the post-process in favour of a single
  consistent enforcement surface.
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
- **BigQuery integration tests in CI.** The matrix lives behind the
  `bigquery-integration-tests` cargo feature with deterministic
  alphabetic-first table selection. `OXY_REQUIRE_BIGQUERY_TESTS=true`
  escalates the silent skip into a hard failure on CI shards with
  workload-identity wired up.

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
- **Extended secret-store backends for `data_sources.config`.**
  `env:`, `file:` (sandboxed to allowed roots, trims trailing
  whitespace), and `gcp-sm:` (ADC chain — `GOOGLE_APPLICATION_CREDENTIALS` →
  `~/.config/gcloud/application_default_credentials.json` → workload
  identity, opt-in via `[server.gcp_sm]` config) ship today.
  Vault / AWS Secrets Manager land under the same one-impl-plus-one-
  registration pattern when an operator brings the use case.
- **Wire-shape handler tests.** `crate::test_support::TestApp::new(Router)`
  is the canonical harness — assemble a focused `Router` (route
  list + narrow state + auth/workspace layer helpers), drive it
  through `Service::oneshot`, parse the JSON envelope. `mockall`
  generates per-trait fakes (`MockApprovalStore` is the first;
  sister mocks land in `test_support` as more handlers grow
  coverage).
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
