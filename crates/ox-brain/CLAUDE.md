# ox-brain

LLM orchestration via branchforge. Translates natural language to IR types.

## Adding a New LLM Operation

1. Add the operation string to the Brain trait method (e.g., `"my_operation"`).
2. Call `self.call_structured("prompt_name", version, "my_operation", &vars, "log msg")`.
3. If it's a cheap operation, add it to `FAST_OPERATIONS` in `model_resolver.rs`.
4. Add a TOML prompt template in `prompts/my_operation.toml`.

## Prompt Caching

All `structured_completion` calls use `SystemPrompt::Blocks` with `CacheTtl::OneHour`. This is automatic — don't use `SystemPrompt::text()`.

## Schema RAG

`schema_rag.rs` selects a relevant subset of the ontology for LLM context. Edge properties are pruned via `MAX_DESCRIBED_PROPS_PER_EDGE`. Large ontologies are truncated to fit context windows.

## Knowledge RAG

`knowledge_rag.rs` retrieves learned corrections from the knowledge store. These are injected into the LLM prompt to prevent repeat mistakes. Corrections are per-ontology and version-scoped.

## Query Translation Pipeline

`translate_query()` follows a 3-tier fallback: MatchQueryIR (structured) → QueryIR (JSON mode) → retry with error context. Each tier emits `ctx.progress()` events for real-time visibility.
