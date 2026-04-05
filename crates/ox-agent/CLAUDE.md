# ox-agent

Branchforge-powered autonomous analysis agent. All 11 tools implement branchforge's `SchemaTool` trait.

## Tool Registration

Tools are registered in `lib.rs` via `builder.tool(MyTool { domain })`. Each tool receives a shared `DomainContext` (ontology, runtime, compiler, store, user context).

## Adding a New Tool

1. Create `tools/my_tool.rs` implementing `SchemaTool`.
2. Define `const NAME`, `DESCRIPTION`, `READ_ONLY`.
3. Register in `lib.rs` builder chain.
4. Add the tool name constant in `tools/mod.rs`.

## Hooks

Two domain hooks run during agent execution:
- `EmbeddingHook` — embeds tool results into semantic memory (background, non-blocking).
- `RecoveryDetectionHook` — detects when a query failure is corrected, auto-creates knowledge entries for future RAG.

## Schema Evolution Tool

`schema_evolution.rs` detects drift between source DB schema and ontology. Uses `PropertyType::check_compatibility_with()` for type mismatch detection. Generates deterministic schema checksums for fast change detection.
