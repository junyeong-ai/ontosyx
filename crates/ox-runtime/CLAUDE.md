# ox-runtime

Graph DB runtime drivers. Currently: Neo4j + Memgraph (both via bolt protocol).

## Pre-Execute Pipeline

Every `GraphRuntime::execute_query` / `execute_load` call routes through `bolt::pipeline::run_pre_execute`, which parses the incoming Cypher **once** into a `CypherAst` and threads that AST through three passes:

1. **Pre-rewrite validation** (`run_pre_rewrite_validation`): `SafetyValidator` (always) + `OntologyValidator` (iff `GRAPH_ONTOLOGY` task-local is set). Runs on the freshly-parsed AST.
2. **Rewrite** (`GraphIsolationStrategy::scope_ast`): the active strategy injects workspace predicates / property assignments on the AST directly. `PropertyStrategy` adds `WHERE var._workspace_id = $_ws_id` / `SET var._workspace_id = $_ws_id`; `DatabaseStrategy` is a passthrough.
3. **Post-rewrite validation**: `WorkspaceScopeValidator` checks the rewritten AST textually references the strategy's `scope_property` (iff the strategy exposes one and `GRAPH_SYSTEM_BYPASS` is off).

Only after all three passes does the pipeline call `ast.render()` to produce the final Cypher text. One parse, three passes — the triple-surface design documented in `cypher/` keeps rewriter and validator behaviour composable without paying the parse cost three times.

Errors from any pass collapse into a single `OxError::Validation { field: "cypher_query", message }` with every issue on its own line — one LLM retry can fix all of them.

The pipeline **refuses execution** when a strategy declares a `scope_property` (i.e. isolation must show up in the rewritten text) but no `GRAPH_WORKSPACE_ID` is bound and we're not in system-bypass. Returns `OxError::Runtime` naming the missing task-local rather than silently passing the query through.

## Strategy Parameter Merge Policy

`ScopedAst.params: Vec<(String, String)>` lets a strategy introduce any number of bind parameters during rewriting. `run_pre_execute` merges these into the caller's original `params: HashMap<String, PropertyValue>` with **strategy-wins semantics** — if a user-supplied parameter collides with a rewriter-injected one (e.g. both name `_ws_id`), the rewriter's value overwrites. This is intentional: scope parameters are system-critical and must not be spoofed from the outside.

## Task-Locals

Three task-locals drive the pipeline:
- `GRAPH_WORKSPACE_ID: Uuid` — injected per-request via branchforge's `ContextScope` / middleware. Drives the rewriter.
- `GRAPH_SYSTEM_BYPASS: bool` — skips isolation for system tasks (migrations, health checks).
- `GRAPH_ONTOLOGY: Arc<OntologyIR>` — active ontology snapshot. When unset, the ontology validator is skipped (server-internal paths like `search_nodes`, profiler, introspection rely on this). The agent tool loads it once per tool call via `DomainContext::current_ontology()` so mid-session edits (apply_ontology / edit_ontology) propagate on the next invocation.

The `GRAPH_` prefix is intentional. `ox-store` also owns
`WORKSPACE_ID` / `SYSTEM_BYPASS` task-locals for its Postgres RLS
layer. A request typically crosses both layers in the same tokio task
scope; keeping the graph-layer names prefixed ensures spawn helpers
(`spawn_scoped::spawn_with_ws`) can capture both pairs without
disambiguating the same bare identifier twice.

## Enrichment

`enrichment.rs` post-processes query results: resolves node labels, adds display names, formats temporal values. Applied after execution, before returning to the agent.

## Adding a New Graph Backend

1. Implement `GraphRuntime` trait (schema DDL, query execution, load, sandbox, health).
2. Implement `TransienceDetector` for error classification (transient vs permanent).
3. Register in `registry.rs`.
