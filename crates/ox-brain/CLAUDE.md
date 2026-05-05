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

## Evaluation capture hook

`DefaultBrain::call_structured_traced` records a
`latency_ms.<operation>` metric whenever **two** conditions hold:

1. The calling task has an `EvaluationContext` bound (set by
   `ox_store::scope_evaluation_context` higher up the stack —
   typically by an evaluation-run endpoint that's iterating
   cases).
2. The Brain was built with
   `Brain::with_evaluation_capture(arc)` — `Arc<dyn EvaluationCapture>`,
   typically the canonical `PostgresStore` upcast.

Both branches short-circuit when their condition is missing, so
production traffic without an evaluation scope pays nothing.
Capture-side write failures are logged at `warn` and dropped —
the LLM call already succeeded, the metric is observability,
not load-bearing. Mirrors the wider observability policy
(rewriter param-collision warnings, request-id correlation).

Adding a new captured axis:

- Extend `EvaluationCapture` with a fresh `record_<axis>` method
  in `ox-store/src/evaluation.rs`. Default to a noop so existing
  consumers (the `NullEvaluationCapture` test stub, plus any
  consumer that hasn't migrated to the new axis) keep working.
- Call `capture.record_<axis>(&ctx, …)` from the corresponding
  Brain hook site. Same dual-condition shape — `current_evaluation_context()
  + self.evaluation_capture.as_ref()`.

Don't bake the axis into `call_structured_traced` directly when
the new metric needs the LLM call's output (faithfulness,
relevance) — those route through a separate judge invocation
that builds its own capture call.

## Query Translation Pipeline

`translate_query()` follows a 3-tier fallback: StructuredMatchQuery (structured output, the LLM-oriented shape of `QueryOp::Match`) → QueryIR (JSON mode) → retry with error context. Each tier emits `ctx.progress()` events for real-time visibility.

After a QueryIR lands, a **pre-flight label check** (`OntologyIR::unknown_labels_in_query`) scans the extracted label set against the active ontology. If any label is unknown, Brain retries once with a `correction` template variable listing the offending labels — the LLM self-corrects more than 90% of the time on re-prompt. If the retry still produces unknowns, Brain returns the bad QueryIR and lets the runtime's `OntologyValidator` issue the deterministic rejection (the agent sees the error through the tool-result path and can loop).

The runtime's AST-level `OntologyValidator` remains the source of truth — the Brain check is a cheap pre-flight that saves 1-2 agent round-trips per hallucination. No logic duplication: the Brain check delegates to a shared helper on `OntologyIR`.
