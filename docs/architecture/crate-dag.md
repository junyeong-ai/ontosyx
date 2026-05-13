# Crate Dependency Graph

The Rust workspace is composed of narrow crates with an enforced
DAG. `cargo-deny` (`deny.toml`) rejects reverse-direction edges in
CI.

## Shape

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
         ▼                 ▼                │
  ┌─────────────┐   ┌──────────────┐        │
  │ ox-compiler │   │ox-federation │◀───────┘
  └──────┬──────┘   └──────┬───────┘
         │                 │
         ▼                 │
  ┌─────────────────┐      │
  │ ox-graph-runtime│      │
  └────────┬────────┘      │
           │               │
           │  ┌─────────┐  │
           │  │ox-memory│  │
           │  └────┬────┘  │
           │       │       │
           │  ┌────┴─────┐ │
           │  │ ox-brain │◀┘
           │  └────┬─────┘
           │       │
           ▼       ▼
        ┌──────────────┐
        │   ox-agent   │
        └──────┬───────┘
               │
               ▼
        ┌──────────────┐
        │    ox-api    │
        └──────────────┘
```

## Responsibilities

| Crate              | Owns                                                                              | Does NOT own |
|--------------------|-----------------------------------------------------------------------------------|--------------|
| `ox-core`          | `OxError`, `OxResult`, `Id<Tag>` macro, time types, small shared utilities        | Any domain concept |
| `ox-ontology`     | `*Def` types for Node/Edge/Interface/Property/Rule/Function/Action/Metric/Enrichment/Glossary/Mapping/DataQuality/Provenance; IRI scheme | Execution, LLM, persistence |
| `ox-query-ir`      | `QueryIR`, `PatternIR`, `Expr`, `PathSpec`; compile target (DB-agnostic)          | Any backend |
| `ox-source`        | `DataSourceAdapter`, `IntrospectionKernel`, `SemanticTyper`, adapters, Arrow normalisation | Execution planning |
| `ox-store`         | PostgreSQL RLS persistence of ontology / glossary / rules / mappings / audit / drift | Business logic |
| `ox-memory`        | Embeddings, `KnowledgeStore`                                                       | LLM calls, tool orchestration |
| `ox-compiler`      | `QueryIR` → Cypher / DataFusion lowering; cost estimation; OWL/Turtle/SHACL export | Backend execution |
| `ox-federation`    | Planner pipeline, DataFusion integration, `TableProvider` wrapping, dispatch       | Prompt templates, persistence |
| `ox-graph-runtime` | Graph backend drivers (Neo4j / Memgraph), Cypher pipeline, validators, dialects   | LLM, prompt logic |
| `ox-brain`         | LLM orchestration: `PromptRegistry`, `ChatModelRegistry`, `ModelResolver`, RAG, PlanRouter | Tool surfaces |
| `ox-agent`         | Tool set, approval state machine, sinks (Embedding, RecoveryDetection, FanOut), progress channel | Prompt authoring |
| `ox-api`           | axum REST + WS + MCP + OpenAPI; `spawn_scoped`; middleware                         | Business logic |

## Enforcement

- Every crate's `Cargo.toml` lists only crates one level closer to
  `ox-core` in the DAG above.
- `deny.toml` carries a `[bans.deny]` block listing forbidden edges.
- A reverse dependency on `ox-brain` / `ox-agent` from `ox-store` /
  `ox-source` / `ox-graph-runtime` / `ox-federation` is a CI break.
