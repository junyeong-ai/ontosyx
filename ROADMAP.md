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
- **Temporal queries.** `ox-compiler::temporal::rewrite_temporal` +
  `rewrite_temporal_with_renames` apply at QueryIR compile time:
  the snapshot's window is enforced (queries before the lineage's
  start or after its end are rejected), and label renames inside
  the snapshot are projected onto the IR's `MatchOp` labels so a
  query authored against today's labels still resolves against the
  pinned schema. The Cypher pipeline carries an empty
  `RewritePhase::Temporal=400` slot for the raw-Cypher path; the
  current QueryIR-compiled path covers every temporal use case
  shipping today.
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

### Federation (VOL)

- **`LinkMappingKind::Computed` — per-dialect parsing.** Seed-
  position Computed ships today (DataFusion SQL dialect, via
  `SessionContext::parse_sql_expr`). Extend and close-cycle
  positions still refuse; source-dialect-specific syntax (PG
  `ILIKE`, Snowflake `PARSE_JSON`, MySQL backticks) also falls
  through the DataFusion parser with a descriptive error. A follow-
  up slice delegates parsing to the underlying adapter when the
  Computed edge carries source-pinned SQL.
- **DuckDB / PG / MySQL / Snowflake / BigQuery `scan()`.** Only CSV
  and JSON adapters materialise rows today; the rest ship
  introspection but not scan. DuckDB is blocked on an arrow-version
  upgrade (the workspace pins arrow 55 via DataFusion 49; duckdb 1.x
  needs arrow 58). The upgrade bundles DataFusion + snowflake-api +
  gcp-bigquery-client together — that single arrow bump unblocks
  every other adapter at once.
- **Extended secret-store backends.** `env:`, `file:` (sandboxed via
  `with_allowed_roots`), and `gcp-sm:` (ADC chain) ship today. Vault
  / AWS Secrets Manager land under the same one-impl-plus-one-
  registration pattern when an operator brings the use case.

## How we test

- **Wire-shape handler tests.** `crate::test_support::TestApp::new(Router)`
  is the canonical harness — assemble a focused `Router` (route
  list + narrow state + auth/workspace layer helpers), drive it
  through `Service::oneshot`, parse the JSON envelope. `mockall`
  generates per-trait fakes (`MockApprovalStore` is the first;
  sister mocks land in `test_support` as more handlers grow
  coverage).
- **RLS integration tests.** `crates/ox-store/tests/rls_enforcement.rs`
  drives real PostgreSQL through the `OX_TEST_DATABASE_URL` env, gated
  `#[ignore]` so the default workspace test run stays hermetic. The
  same pattern covers `data_sources_integration.rs`.
- **Frontend Playwright fixtures.** `auto:true` fixture in
  `web/tests/fixtures.ts` seeds locale + onboarding + mock-workspace
  per spec; APIRequestContext bypass via `page.evaluate(fetch)`.
- **i18n auditor.** TypeScript-AST scope-aware scan
  (`scripts/i18n-audit.mjs`) catches missing translation keys before
  PR review.

## Out of scope

- **DataFusion-side ACL pre-execute.** Federation paths apply ACL
  via post-process (`enforce_acl_on_result`) on the materialised
  Arrow batches. A DataFusion `LogicalPlan` visitor that prunes
  denied scans + projects masked columns at plan time is the natural
  single-surface story, but the cost only becomes worth paying once
  federation traffic has measurable throughput pressure or policies
  grow row-filter shapes the post-process can't express. Trigger to
  reconsider: a workspace whose federation queries spend >20% of
  wall-clock in `enforce_acl_on_result`.
- **Snowflake / BigQuery compose-based integration tests.** The
  Snowflake adapter ships URL/identifier/quote unit tests; BigQuery
  ships behind the `bigquery-integration-tests` cargo feature with
  deterministic alphabetic-first table selection. A compose stack
  spinning up real Snowflake / BigQuery workspaces against credentialed
  fixtures is operator territory — the project ships the harness, not
  the credentialed CI shard.
- **OWL DL/RL reasoning, SHACL-SPARQL (`sh:sparql`), RDF-native
  ingest, SPARQL endpoint.** This is a graph operations workbench,
  not a description-logic reasoner. SHACL Core suffices for the
  validation contract. The semantic-web stack returns under a
  separate gateway crate when a concrete use case requires
  triple-store interoperability.
- **Neptune.** The runtime's AST is DB-agnostic in principle but
  every compiler / runtime / adapter shipped today targets Neo4j +
  Memgraph. Neptune support returns when a concrete use case
  arrives; the architecture supports it (new `GraphCompiler` +
  `GraphRuntime` implementations) but no speculative code lands
  ahead of demand.
- **Type taxonomy via inheritance.** Explicitly chosen against. The
  primitive is `InterfaceDef.implements: Vec<InterfaceId>` — the
  federation planner's `InterfaceExpander` resolves `(:Iface)` to
  the union of concrete implementers, supporting multiple inheritance.
  See `crates/ox-ontology/CLAUDE.md` Don't list.
- **Pulling NVL back in.** The removal was load-bearing — 62
  transitive packages, several ESM / SSR workarounds, two dynamic-
  import race conditions. XyFlow is now the only graph toolkit in
  the workbench; re-introducing a second one would require a concrete
  capability NVL provided that XyFlow doesn't, and no such capability
  is on the backlog today.
