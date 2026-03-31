# ox-brain

LLM orchestration via branchforge. Translates natural language to IR types.

## Key Types

- `DefaultBrain` — holds `ClientPool` + `ModelResolver` + `PromptRegistry`
- `ModelResolver` trait — resolves operation name → (provider, model_id). Implementations: `StaticModelResolver`, `DbModelRouter`.
- `ClientPool` — shared branchforge clients keyed by provider identity. Use `get_by_provider()` for cached lookup.
- `LlmProviderConfig` — canonical provider config type, used by both ox-api and ox-brain.

## Adding a New LLM Operation

1. Add the operation string to the Brain trait method (e.g., `"my_operation"`).
2. Call `self.call_structured("prompt_name", version, "my_operation", &vars, "log msg")`.
3. If it's a cheap operation, add it to `FAST_OPERATIONS` in `model_resolver.rs`.
4. Add a TOML prompt template in `prompts/my_operation.toml`.

## Prompt Caching

All `structured_completion` calls use `SystemPrompt::Blocks` with `CacheTtl::OneHour`. This is automatic — don't use `SystemPrompt::text()`.
