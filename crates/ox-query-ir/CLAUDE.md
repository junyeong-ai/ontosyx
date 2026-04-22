# ox-query-ir

DB-agnostic query algebra. Two cooperating IRs:

- **`QueryIR`** — the compile target. Every downstream consumer
  (`ox-compiler` → Cypher, `ox-federation` → DataFusion LogicalPlan,
  `ox-runtime` validators, the agent / MCP surfaces) works against
  this shape. `src/query.rs`.
- **`PatternIR`** — the canvas-facing, UI-ergonomic form. Round-trips
  through the pair `compile → decompile`: compile is lossless,
  decompile is best-effort (only `QueryOp::Match` fully reconstructs;
  other QueryOp variants come back as non-editable stubs).
  `src/pattern/mod.rs`.

## `QueryIR` shape

```rust
QueryIR {
    schema_version: u32,          // on-wire version gate
    operation: QueryOp,           // one of 8 variants, see below
    limit: Option<usize>,
    skip: Option<usize>,
    order_by: Vec<OrderClause>,
    as_of: Option<DateTime<Utc>>, // temporal pivot (ontology snapshot)
}
```

### `QueryOp` variants (8)

- `Match { patterns, filter, projections, optional, group_by }` —
  the main workload. Every other IR layer can also lower this shape.
- `PathFind { start, end, edge_types, direction, max_depth,
  algorithm }` — separate from Match because `shortestPath()` has
  engine-specific compilation paths.
- `Aggregate { source, group_by, aggregations, having }` — GROUP BY.
  `having` compiles to `WITH … WHERE …` in Cypher (no HAVING keyword).
- `Union { queries, all }` — UNION / UNION ALL.
- `Chain { steps }` — sequential pipeline, compiles to `WITH` chains.
- `CallSubquery { inner, import_variables }` — Cypher `CALL { … }`.
- `Mutate { context, operations }` — CREATE / MERGE / DELETE / SET.
- `Analytics { … }` — engine-specific analytics (centrality, etc.).

## Schema-version gate

`QUERY_IR_SCHEMA_VERSION` is a `u32` constant. `QueryIR::schema_version`
defaults to it on deserialize; a wire value newer than what the
server knows is rejected. Bump the constant on breaking struct-shape
changes (new required fields, removed variants, reordered tagged
enums). Additive optional fields don't need a bump.

## `PatternIR` round-trip

- `PatternIR::compile(&self) -> OxResult<QueryIR>` — lossless for
  every PatternIR; the test suite pins a round-trip property.
- `PatternIR::decompile(query: &QueryIR) -> Self` — best-effort.
  Only `QueryOp::Match` reconstructs node/edge positions +
  projections. Other QueryOps surface as a non-editable representation
  the canvas renders read-only.

`/api/query/pattern/compile` and `/api/query/pattern/decompile`
expose both transforms as pure endpoints (no DB, no LLM).

## Structured-output form

`StructuredMatchQuery` in `src/structured_match.rs` is the simplified
shape LLMs generate in tool use. It converts to `QueryIR::Match`
before reaching the compiler. Keeping LLM output narrow (node /
edge / filter / projection arrays) massively shortens prompt sizes
and lets the `MatchQueryBuilder` in ox-brain do shape validation
before any downstream call.

## Identifier families

- **Compile-target / runtime IR**: `QueryIR`, `PatternIR`.
- **LLM structured output**: `StructuredXxxQuery`.
- **Input DTO (user / LLM pre-validation)**: `InputXxxDef`.
- **Analysis output**: `XxxReport`, `XxxInsight`.

These are project-wide conventions (see the root `CLAUDE.md`); this
crate is the canonical home for the first two families.

## Don't

- Don't bump `QUERY_IR_SCHEMA_VERSION` casually — every persisted
  `PatternIR` in ox-store's `saved_query_patterns` rides on the
  current value. A bump forces a migration story.
- Don't add new variants to `QueryOp` without also extending
  `PatternIR::compile` / `decompile` and the agent prompts — silent
  drop on either side produces opaque "query compiled but did nothing"
  bugs.
- Don't reference physical column names in this crate. Everything
  here is logical (label + property key). The physical mapping lives
  in `ox-ontology::mapping::ObjectMappingDef` and is resolved by
  `ox-federation`'s planner / `ox-compiler`'s Cypher emitter.
