# ox-federation

Virtual Ontology Layer (VOL) execution engine. Lowers a `QueryIR`
against an `OntologyIR` to a DataFusion `LogicalPlan`, then executes
it against registered `DataSourceAdapter` implementations.
See `docs/adr/0001-virtual-ontology-layer.md` and `0002-datafusion-federation.md`.

## Public API

- `build_query_ir_scoped(ontology, query, workspace_id, adapters) -> LogicalPlan` —
  primary entry. Injects `col(mapping.workspace_scope) = lit(workspace_id)` on every scan
  whose mapping declares a scope column.
- `build_query_ir` — same without workspace scoping. System-bypass path; scheduled jobs
  and CSV bring-up tests only.
- `build_match_op` / `build_match_plan` — lower a `QueryOp::Match` or a pre-planned
  `MatchPlanSpec` directly. Used by upstream callers that already built the spec.
- `FederationContext::execute_plan(plan) -> Vec<RecordBatch>` — runs the DataFusion
  plan and collects batches.
- `AdapterResolver` trait — maps `SourceId` → `Arc<dyn DataSourceAdapter>`.
  `InMemoryAdapterResolver` is the bring-up impl; ox-api wraps a per-workspace HashMap
  of them and lazily hydrates from the `data_sources` store.

## Planner stages

`planner/` is a pipeline of pure functions:

1. `LabelResolver` — resolve a `GraphLabel` to one or more `NodeTypeId`s
   (concrete type, or every implementer of an interface).
2. `InterfaceExpander` — walk implementers for interface labels.
3. `MappingResolver` — `NodeTypeId` → `Vec<&ObjectMappingDef>` sorted by precedence,
   honouring `valid_from`/`valid_to` for temporal pivots.
4. `MatchPlanner` — stitch the resolved scans + hops into a `MatchPlanSpec`.
5. `build_match_plan_full` (in `logical_plan_builder.rs`) — the orchestrator.
   Owns `JoinAssembler`; calls `build_single_scan` / `build_union_scan` for every
   `NodeScanSpec`, then `apply_joins` / `apply_filters` / `apply_projections` /
   `apply_order_by` / `apply_limit_skip`.

## Link-mapping matrix

`LinkMappingKind` has four variants; `apply_joins` lowers them at every hop position:

|                        | Seed | Extend | Close-cycle |
|------------------------|------|--------|-------------|
| ForeignKey             | ✓    | ✓      | ✓           |
| Federated              | ✓    | ✓      | ✓           |
| Bridge                 | ✓    | ✓      | ✓           |
| Computed               | refuse (slice 5d) — needs adapter-side SQL pushdown |

Multi-mapping hops (`link_mappings.len() > 1`) are supported at all three positions:
`seed_multi_mapping_hop`, `extend_multi_mapping_hop`, `close_cycle_multi_mapping_hop`.

## Load-bearing conventions

- **Qualified column refs**: every `col(...)` call uses `"<variable>.<field>"` form
  (e.g., `col("u.name")`). Scans alias their table by the bound variable, so
  qualified refs resolve unambiguously on join plans.
- **UNION strips qualifiers**: DataFusion's UNION node drops variable-level
  qualifiers from the merged schema. Multi-mapping hops therefore apply
  filter + projection **inside each branch** before the UNION — the branch's
  projection rewrites `u.name AS user_name`, and the merge sees matching schemas.
  `apply_joins` returns `(LogicalPlan, bool)`; the bool signals the caller to
  skip outer filter/project stages.
- **Bridge scans alias as `__br<hop_idx>_<branch_idx>`** — never a valid
  `VariableName`, so it cannot collide with a query-bound variable. The
  multi-branch suffix keeps per-mapping bridge scans isolated when the same
  physical bridge relation appears in multiple link mappings.
- **Workspace scope is not injected on bridge scans** — `LinkMappingDef` has
  no per-kind scope declaration. Authors who need workspace isolation on the
  bridge should promote it to a NodeType instead.
- **Composite-key bridges** — `Bridge.source_join` / `target_join` are
  `Vec<ColumnRef>`, zipped pairwise with `endpoint.key_columns` and
  `AND`-combined. Mismatched lengths refuse.

## Scan coverage

PostgreSQL / MySQL / BigQuery / CSV / JSON adapters ship `scan()` against
arrow-55 `RecordBatch`. JSON handles the top-level `records` table plus
nested `records_<a>_<b>...` relations of arbitrary depth (the adapter
walks `schema.tables` longest-prefix to disambiguate field names that
contain `_`, and flattens array-of-object hops top-down). DuckDB,
Snowflake, and MongoDB are introspection-only; each returns
`UnsupportedOperation` on scan until the adapter layer grows a
materialisation path. DuckDB specifically pairs that work with the
arrow 55 → 58 upgrade — duckdb 1.x already pulls arrow 58 transitively,
so its scan path needs the workspace to coordinate the bump across
DataFusion + snowflake-api + gcp-bigquery-client at the same slice.

## Don't

- Don't call provider APIs directly from this crate — adapters are the seam.
- Don't ignore the `AdapterResolver` abstraction when wiring a new caller;
  handing a raw `HashMap<SourceId, Arc<dyn …>>` bypasses the refusal-on-missing
  diagnostic and produces opaque DataFusion errors instead.
- Don't emit bare `col(name)` in new code — use `col("<variable>.<field>")` or
  go through `qualify_join_column` / `build_equi_join_predicate` / the
  `projection_to_df_expr` helper.
