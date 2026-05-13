# ox-agent

entelix-powered autonomous analysis agent. All domain tools implement entelix's [`SchemaTool`] trait; the agent build composes them through a `ToolRegistry` wrapped with `WorkspaceScope` (RLS task-locals) and `ToolEventLayer` (sink → SSE wire).

## Tool Registration

Tools are registered in `lib.rs` via `builder.tool(MyTool { domain })`. Each tool receives a shared `DomainContext` (ontology, runtime, compiler, store, user context).

## Mid-Session Ontology Updates

`DomainContext.ontology: Option<ArcSwap<OntologyIR>>` lets tools that mutate the ontology — `apply_ontology`, `edit_ontology` — publish a replacement into the shared slot *without rebuilding the DomainContext*. Tools that read the ontology call `domain.current_ontology()` at the start of each invocation; the `load_full()` under the hood returns a fresh `Arc<OntologyIR>` reflecting the latest publish.

Contract for tool authors:
- **Mutators** must call `self.domain.replace_ontology(new_ir)` after persisting to the store so downstream tools in the same session see the new labels. Skip this and the next `query_graph` call will reject valid queries against freshly-added nodes.
- **Readers** must take the snapshot at entry (`let ontology = self.domain.current_ontology().ok_or(...)?`) and use that for the whole invocation. Don't re-fetch mid-tool — a concurrent mutator could otherwise produce an inconsistent view across a single tool's logical operation.

The `GRAPH_ONTOLOGY.scope(Arc::clone(&ontology), runtime.execute_query(...))` wrap in `query_graph` uses the captured snapshot, so the runtime validator sees exactly what the tool saw at entry.

## Spawn Safety

`tokio::spawn` detaches from the task-local scope. Anything the spawned future writes to a workspace-scoped store (`create_analysis_result`, memory updates, knowledge records) must capture `ox_store::WORKSPACE_ID` / `SYSTEM_BYPASS` before the spawn and re-scope inside, or the pool's `before_acquire` hook ends up with no `app.workspace_id` and the INSERT hits the RLS deny-all branch. See `tools/execute_analysis.rs` for the reference pattern.

## Adding a New Tool

1. Create `tools/my_tool.rs` implementing `SchemaTool`.
2. Define `const NAME`, `DESCRIPTION`, `READ_ONLY`.
3. Register in `lib.rs` builder chain.
4. Add the tool name constant in `tools/mod.rs`.

## Tool Result Contract

`SchemaTool::handle` returns a JSON envelope that goes straight into
the LLM's tool-result context window. Three rules:

- **LLM-only fields.** Every field in the result struct must inform
  the model's *next decision*. Backend bookkeeping, persistence
  identifiers, FE rendering metadata, and per-step instrumentation
  are not LLM input.
- **FE rendering data goes through the persisted execution row.**
  `query_graph` writes the full `QueryExecution` (compiled target,
  provenance, timing breakdown, results) and the FE fetches via
  `/api/executions/{id}`. The chat panel does not parse the tool
  result JSON for rendering.
- **Real-time UI updates ride SSE progress events.**
  `ctx.progress("step").completed_with(ms, json!({...}))` is the
  streaming channel. Don't double-encode the same data on the
  result envelope as a "drain-mode fallback" — progress events
  arrive in drain mode too.

A field that crosses any of these lines belongs on the persisted
row or the SSE stream, not on the tool result.

**Removing a field is a contract change.** Grep every FE
`JSON.parse(...output...)` call site for the field name before
shipping the cut and re-route each consumer to the persisted-row
(`useExecution`) or SSE-stream (`toolCall.steps`) channel — silent
regressions surface in panels nobody opened during local testing.

## Sinks

Two domain sinks fan out off the agent's `AgentEventSink<ReActState>` channel:
- `EmbeddingSink` — embeds tool results into semantic memory (background, non-blocking).
- `RecoveryDetectionSink` — detects when a query failure is corrected, auto-creates knowledge entries for future RAG.

Both are observe-only — `send` always returns `Ok(())` and any internal failure is `tracing::warn!`-swallowed. Spawned write tasks capture `ContextScope::capture_current()` before `tokio::spawn` and `scope.run(...)` inside, so RLS task-locals survive the scheduler boundary.

## Schema Evolution Tool

`schema_evolution.rs` detects drift between source DB schema and ontology. Uses `PropertyType::check_compatibility_with()` for type mismatch detection. Generates deterministic schema checksums for fast change detection.
