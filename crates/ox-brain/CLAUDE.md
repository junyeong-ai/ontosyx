# ox-brain

LLM orchestration through entelix. Translates natural language into IR types.

## Layered dispatch — single funnel

Every LLM call goes through `DefaultBrain::call_structured_traced` (structured output) or `call_text_traced` (free-form). The dispatch primitives `provider::structured_completion` / `text_completion` are `pub(crate)` — direct callers cannot bypass the traced funnel.

Two cross-cutting layers ride every `ChatModel` minted by `ChatModelRegistry` (composed in `chat_model_factory::apply_layers`):

1. **`entelix::RetryLayer`** (innermost, `RetryPolicy::standard()`) — exponential backoff retries on transient provider errors (network / TLS / DNS / 408 / 425 / 429 / 5xx). Wraps the leaf dispatch so a retried call still observes a single PolicyLayer pre-check + single post-call charge.
2. **`entelix::PolicyLayer`** (outermost, registered when `ChatModelRegistry::with_policy_registry(...)`) — `RunBudget` pre-call cost + token gates, per-tenant cost ledger charging on the `Ok` branch, optional PII redaction. `RunBudget::check_pre_request_{cost,tokens}` fires before the wire roundtrip so 1-call slop is closed.

`ChatModel::layer_names()` lands at boot-log so operator dashboards see the resolved stack without spelunking source.

## Brain trait surface

Every Brain trait method takes `ctx: &ExecutionContext` as the **last** parameter (entelix convention — work args first, request-scope ctx last). The chat path threads its chat-wide `RunBudget` + `ProgressReporter` through `ctx`; background callers (eval worker, cron, MCP, admin routes) pass `&ExecutionContext::default()` and the Brain enriches with `with_default_run_budget(caps)` when the ctx carries no budget.

## RunBudgetCaps — six-axis cap recipe

`RunBudgetCaps` (request count / tool calls / input·output·total tokens / cost USD) is the workspace's single source of truth for budget recipes. `ox-agent::build_execution_context(caps, thread_id, workspace_id)` mints a fresh `entelix::RunBudget` from it for the chat path; `DefaultBrain::with_default_run_budget(caps)` stamps the same recipe as the process-wide default for ctx-less Brain calls. Each materialisation is fresh — counters are per-call, never shared.

## Adding a new LLM operation

1. Add the operation string to the Brain trait method.
2. Call `self.call_structured("prompt_name", version, "my_operation", &vars, "log msg", ctx)` — `ctx` is the last arg, threaded from the trait method.
3. If the operation is cheap, add it to `FAST_OPERATIONS` in `model_resolver.rs`.
4. Add a TOML prompt template in `prompts/my_operation.toml`.

## Prompt caching

`structured_completion` / `text_completion` both wrap the system prompt with `SystemPrompt::cached(system, CacheControl::one_hour())` — the Anthropic / Bedrock-on-Claude codecs emit the cache breakpoint natively. Don't construct prompts via `SystemPrompt::text()`; the cached form is the only path that lets the `cached_input_tokens` axis on `TokenUsage` light up.

## Prompt budget — token-aware

`call_structured_traced` resolves an `entelix::TokenCounter` from the `(provider, model)` pair via `DefaultBrain::with_token_counter_registry` and gates the rendered prompt through `design::assert_within_budget`. `o200k_base` for newer OpenAI, `cl100k_base` for GPT-4-class, `ByteCountTokenCounter` fallback for Anthropic / unmapped families. CJK / Korean payloads count correctly under vendor-accurate tokenisers — char heuristics over- or under-shoot by 2-3× for non-Latin scripts. New prompt-template names land in `PromptBudget::default_for_unmapped` (generous); per-surface templates carry tighter caps in `PromptBudget::for_prompt`.

## Cost catalogue — wired through `ChatModelRegistry`

`ChatModelRegistry::with_policy_registry(Arc<PolicyRegistry>)` carries the workspace's pricing catalogue. The registry's `CostMeter` reads the `PricingTable` built from `PostgresStore::list_active_model_prices` via `ox_brain::pricing::pricing_table_from` (boot-time one-shot). `UnknownModelPolicy::WarnOnce` + an `UnknownModelSink` lift catalogue-drift signals into operator metrics.

Admin pricing writes invoke `PolicyRegistry::mutate_fallback` to swap the entire policy or `CostMeter::replace_model_pricing(model, pricing)` for single-row updates without full-table reseed. `CostMeter::pricing_snapshot()` returns an owned `PricingTable` clone for diff / external-store reconciliation paths.

## Schema RAG

`schema_rag.rs` selects a relevant subset of the ontology for LLM context. Edge properties are pruned via `MAX_DESCRIBED_PROPS_PER_EDGE`. Large ontologies are truncated to fit context windows.

## Knowledge RAG

`knowledge_rag.rs` retrieves learned corrections from the knowledge store. These are injected into the LLM prompt to prevent repeat mistakes. Corrections are per-ontology and version-scoped.

## Evaluation capture hook

`call_structured_traced` records a `latency_ms.<operation>` metric whenever both conditions hold:

1. The calling task has an `EvaluationContext` bound (set by `ox_store::scope_evaluation_context`).
2. The Brain was built with `Brain::with_evaluation_capture(arc)` — typically the canonical `PostgresStore` upcast.

Both branches short-circuit when their condition is missing — production traffic without an evaluation scope pays nothing. Capture-side write failures are logged at `warn` and dropped (observability, not load-bearing).

Adding a new captured axis:

- Extend `EvaluationCapture` with a fresh `record_<axis>` method in `ox-store/src/evaluation.rs`. Default to noop.
- Call `capture.record_<axis>(&ctx, …)` from the corresponding Brain hook site. Same dual-condition shape.

Don't bake the axis into `call_structured_traced` directly when the new metric needs the LLM call's output — those route through a separate judge invocation that builds its own capture call.

## EvaluationJudge — RAGAS scoring

`EvaluationJudge::judge_evaluation_case(question, expected, actual, ctx)` returns an `EvaluationJudgement` carrying four independent axes (`faithfulness`, `answer_relevance`, `context_precision`, `context_recall`), each with a `score` in `[0.0, 1.0]` and a one-or-two-sentence `reasoning`. The judge prompt (`prompts/evaluation_judge.toml`) defines each axis precisely and forces structured JSON output.

`EvaluationJudgement::axes()` is the canonical iterator the case-judge endpoint walks to record one `evaluation_metrics` row per axis. Adding a new axis = one `EvaluationJudgement` field + one canonical entry + one prompt-template revision.

The judge runs through the same `call_structured_traced` pathway every other LLM op uses, so judging inside an `EvaluationContext` scope automatically records `latency_ms.evaluation_judge`.

## entelix wire-code drift protocol

`entelix_error::classify_wire_code` maps entelix's `Error::wire_code` bucket onto our 11 `LlmErrorCode` variants. The wildcard arm falls back to `LlmErrorCode::ProviderUnavailable` with a `tracing::warn!` so a new entelix bucket lights up the moment it fires.

The fallback is intentionally lossy — a new entelix bucket like `policy_blocked` (PII redactor) or `tool_call_validation` is *not* a "provider unavailable" condition, but mapping it that way surfaces an actionable warn line instead of a silent miscategorisation. Every entelix minor bump requires:

1. Diff `entelix::Error::wire_code`'s `match` arms against the previous release.
2. For any new variant: add a corresponding `LlmErrorCode` (if the semantic is distinct) or fold into an existing one, then update both the `classify_wire_code` match and the `errors.llm_<code>` i18n templates.
3. Confirm `pnpm error-code-parity-audit` still passes — the variant + as_str + class + ko + en parity gate catches partial migration.

The wildcard arm's `tracing::warn!` line carries `entelix_wire_code = <bucket>` so operators searching dashboards can identify drift before a user-facing locale gap shows up.

## Query translation pipeline

`translate_query()` follows a 3-tier fallback: `StructuredMatchQuery` (LLM-oriented shape of `QueryOp::Match`) → `QueryIR` (JSON mode) → retry with error context. Each tier emits `ctx.progress()` events for real-time visibility.

After a QueryIR lands, a **pre-flight label check** (`OntologyIR::unknown_labels_in_query`) scans the extracted label set against the active ontology. If any label is unknown, Brain retries once with a `correction` template variable listing the offending labels — the LLM self-corrects on re-prompt for most cases. If the retry still produces unknowns, Brain returns the bad QueryIR and lets the runtime's `OntologyValidator` issue the deterministic rejection.

The runtime's AST-level `OntologyValidator` remains the source of truth — the Brain check is a cheap pre-flight that saves agent round-trips. No logic duplication: the Brain check delegates to a shared helper on `OntologyIR`.
