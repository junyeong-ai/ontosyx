# Crate Dependency Graph

The Rust workspace is composed of narrow crates with an enforced DAG.
`cargo-deny` (see `deny.toml`) is extended in Phase 1 to reject
reverse-direction edges automatically.

## Target shape

```
                      ┌─────────┐
                      │ ox-core │
                      └────┬────┘
                           │
         ┌─────────────────┼─────────────────┐
         │                 │                 │
         ▼                 ▼                 ▼
  ┌─────────────┐   ┌────────────┐   ┌────────────┐
  │ ox-ontology │   │  ox-source │   │  ox-store  │
  └──────┬──────┘   └──────┬─────┘   └──────┬─────┘
         │                 │                │
         ▼                 │                │
  ┌─────────────┐          │                │
  │ ox-query-ir │          │                │
  └──────┬──────┘          │                │
         │                 │                │
         └────────┬────────┘                │
                  ▼                          │
           ┌───────────────┐   (reads) ─────┘
           │ ox-federation │
           └───────┬───────┘
                   │
                   │  (optional dep)       ┌────────────────┐
                   ├──────────────────────▶│  ox-inference  │
                   │                       └────────────────┘
                   │
                   │  (GraphCacheBackend impl)
                   │                       ┌────────────────┐
                   ├──────────────────────▶│   ox-runtime   │
                   │                       └────────────────┘
                   │
                   │                       ┌────────────────┐
                   │          ┌───────────▶│   ox-memory    │
                   │          │            └────────────────┘
                   │          │                    ▲
                   │          │                    │
                   │     ┌────┴─────┐       ┌──────┴──────┐
                   │     │ ox-brain │◀──────│             │
                   │     └────┬─────┘       │             │
                   │          │             │             │
                   └────────┐ │             │             │
                            ▼ ▼             │             │
                        ┌─────────┐         │             │
                        │ox-agent │─────────┘             │
                        └────┬────┘                       │
                             │                            │
                             ▼                            │
                        ┌─────────┐                       │
                        │ ox-api  │───────────────────────┘
                        └─────────┘
```

## Responsibilities

| Crate           | Owns                                                                                     | Does NOT own |
|-----------------|------------------------------------------------------------------------------------------|--------------|
| `ox-core`       | `OxError`, `OxResult`, `Id<Tag>` macro, time types, small shared utilities               | Any domain concept |
| `ox-ontology`   | `*Def` types for Node/Edge/Interface/Property/Rule/Function/Action/Metric/Enrichment/Glossary/Mapping/DataQuality/Provenance/Drift/Audit; IRI scheme | Execution, LLM, persistence |
| `ox-query-ir`   | `QueryIR`, `PatternIR`, `PlanIR`, `Expr`, `PathSpec`; compile target (DB-agnostic)       | Any backend |
| `ox-source`     | `DataSourceAdapter`, `IntrospectionKernel`, `SemanticTyper`, 8 adapters, Arrow normalisation | Execution planning |
| `ox-store`      | PostgreSQL RLS persistence of ontology / glossary / rules / mappings / audit / drift     | Business logic |
| `ox-memory`     | Embeddings, `KnowledgeStore`, `RecoveryDetectionHook`                                    | LLM calls, tool orchestration |
| `ox-federation` | Planner pipeline, DataFusion integration, `TableProvider` wrapping, cost, cache, dispatch | Prompt templates, persistence |
| `ox-runtime`    | `GraphCacheBackend` implementation (Neo4j / Memgraph)                                    | Primary execution |
| `ox-inference`  | `RuleEngine`, `InferenceEngine` traits; SHACL reference validator; slot for OWL RL / Datalog | Primary rule evaluation path (that lives in ox-federation for SHACL Core) |
| `ox-brain`      | LLM orchestration: `PromptRegistry`, `ClientPool`, `ModelResolver`, RAG                   | Tool surfaces |
| `ox-agent`      | Tool set, approval state machine, progress channel                                       | Prompt authoring |
| `ox-api`        | axum REST + WS + MCP + OpenAPI; `spawn_scoped`; middleware                                | Business logic |

## Enforcement

- Every crate's `Cargo.toml` lists only crates one level closer to
  `ox-core` in the DAG above.
- `deny.toml` gains a `[bans.deny]` block listing forbidden edges
  (Phase 1). `cargo deny check bans` runs in CI.
- Reverse dependency on `ox-brain` / `ox-agent` from `ox-store` /
  `ox-source` / `ox-runtime` / `ox-federation` is a build-break.

## Migration notes from v1/v2

- `ox-core::ontology_ir` → `ox-ontology` (Phase 3).
- `ox-core::query_ir` → `ox-query-ir` (Phase 3).
- `ox-runtime::cypher::*` rewriters / validators → `ox-federation::{rewrite,
  validate}` (Phase 7). `ox-runtime` retains the Cypher emitter as the
  graph-cache implementation.
- `ox-compiler` (legacy) is folded into `ox-federation` (planner) and
  `ox-runtime` (Cypher emitter); it ceases to exist as a separate crate.
