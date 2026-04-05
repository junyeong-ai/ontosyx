# ox-runtime

Graph DB runtime drivers. Currently: Neo4j (via bolt protocol).

## Workspace Isolation

Two task-locals enforce multi-tenant isolation:
- `GRAPH_WORKSPACE_ID: Uuid` — injected per-request via branchforge's `ContextScope`.
- `GRAPH_SYSTEM_BYPASS: bool` — skips isolation for system tasks (migrations, health checks).

`isolation.rs` rewrites Cypher queries to inject workspace filters. `scope_cypher()` prepends workspace predicates to MATCH clauses.

## Enrichment

`enrichment.rs` post-processes query results: resolves node labels, adds display names, formats temporal values. Applied after execution, before returning to the agent.

## Adding a New Graph Backend

1. Implement `GraphRuntime` trait (schema DDL, query execution, load, sandbox, health).
2. Implement `TransienceDetector` for error classification (transient vs permanent).
3. Register in `registry.rs`.
