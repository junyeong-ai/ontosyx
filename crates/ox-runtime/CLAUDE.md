# ox-runtime

Graph DB runtime drivers. Currently: Neo4j + Memgraph (both via bolt protocol).

## Pre-Execute Pipeline

Every `GraphRuntime::execute_query` / `execute_load` call routes through `bolt::pipeline::run_pre_execute`, which runs three cross-cutting passes:

1. **Pre-rewrite validation**: `SafetyValidator` (always) + `OntologyValidator` (iff `GRAPH_ONTOLOGY` task-local is set).
2. **Rewrite**: the active `GraphIsolationStrategy` injects workspace predicates / property assignments.
3. **Post-rewrite validation**: `WorkspaceScopeValidator` checks the rewritten statement textually references the scope property (iff the strategy exposes one and `GRAPH_SYSTEM_BYPASS` is off).

Errors from any pass collapse into a single `OxError::Validation { field: "cypher_query", message }` with every issue on its own line — one LLM retry can fix all of them.

## Task-Locals

Three task-locals drive the pipeline:
- `GRAPH_WORKSPACE_ID: Uuid` — injected per-request via branchforge's `ContextScope` / middleware. Drives the rewriter.
- `GRAPH_SYSTEM_BYPASS: bool` — skips isolation for system tasks (migrations, health checks).
- `GRAPH_ONTOLOGY: Arc<OntologyIR>` — active ontology snapshot. When unset, the ontology validator is skipped (server-internal paths like `search_nodes`, profiler, introspection rely on this).

## Enrichment

`enrichment.rs` post-processes query results: resolves node labels, adds display names, formats temporal values. Applied after execution, before returning to the agent.

## Adding a New Graph Backend

1. Implement `GraphRuntime` trait (schema DDL, query execution, load, sandbox, health).
2. Implement `TransienceDetector` for error classification (transient vs permanent).
3. Register in `registry.rs`.
