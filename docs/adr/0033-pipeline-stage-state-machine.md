# ADR-0033 — `PipelineStage` typed state machine + `InferenceSession` / `InferenceAttempt` history

## Status

Accepted — Φ9.1+9.2, 2026-05-08.

## Context

The pre-Φ9 `ox-agent` ran a flat tool-loop: the LLM picked the
next tool from a 6-element bag (`consult_knowledge`,
`query_graph`, `introspect_source`, `resolve_ambiguity`,
`execute_analysis`, `schema_evolution`), the agent dispatched,
the result fed the next prompt. Two structural problems:

1. **Observability was un-shaped.** "Where did this run fail?"
   only resolved by reading every tool-call span and inferring
   the stage from the tool name. There was no canonical answer
   to "what stage was this on" because the agent didn't track
   one. Operators triaging a regression read 20+ spans of
   tool-call output before locating the failure.
2. **Refine had no typed history to fold over.** The user's
   master plan called out (D4) that the agent's "self-correction
   loop" was loop-thinking — the retry mechanic was a re-prompt
   with a fresh context, not a structured fold over prior
   failure outcomes. There was no `Vec<InferenceAttempt>` to
   inject as ICL because attempts weren't persisted as typed
   units.

The `ontive` reference impl (peer comparative analysis,
2026-05-07) had already validated the inverse pattern: an 8-stage
declarative state machine with typed StageOutcome transitions,
operating on a frozen attempt history. That shape gives
observability + replay + checkpointing for free.

## Decision

Promote the inference pipeline into a closed enum + a static
transition table + a typed attempt history. Three substrate
layers:

**`PipelineStage`** (`crates/ox-ontology/src/inference_pipeline.rs`)
— closed 9-variant enum:

```
SafetyGate → Retrieve → Ground → Compile → Validate →
                                    ↑           ↓
                                    └── Refine ─┘
                                                ↓
                                  Select → Compose → Done
```

`#[repr(u8)]` pins the layout so const-fn equality compiles.
`StageOutcome { Pass, Fail, Skip }` discriminates per-stage
transitions. `ErrorClassification` (ValidationFailure /
RuntimeError / Timeout / OutOfBudget / SafetyReject / Internal)
sub-classifies a `Fail` outcome — routing happens at the agent
layer based on the classification.

**`TRANSITIONS`** const slice — the entire DAG written down
once, exhaustively. Forward path on Pass/Skip; Compile +
Validate's `Fail` routes to Refine; Refine `Pass` loops back to
Compile (one more try); Refine `Fail` fans to Done (retries
exhausted).

**Const-fn exhaustiveness assertion** at the bottom of the
module:

```rust
const _: () = {
    // For every (non-terminal stage, outcome) pair, assert
    // exactly one TRANSITIONS entry exists.
    // …
    assert!(found == 1, "TRANSITIONS missing or duplicating …");
};
```

A new `PipelineStage` variant added without updating the
transition table fails the build at compile time. The runtime
cannot observe `next(Done, _)` returning unexpectedly — the
table is total by construction.

**`InferenceSession` / `InferenceAttempt`** types — typed
persistence shape.

```rust
pub struct InferenceSession {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub question: String,
    pub initiator: AgentRef,
    pub final_outcome: Option<SessionOutcome>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

pub struct InferenceAttempt {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub parent_attempt_id: Option<Uuid>,
    pub attempt_index: u32,
    pub emitted_at_stage: PipelineStage,
    pub query_ir_candidate: Option<serde_json::Value>,
    pub outcome: AttemptOutcome,
    pub provenance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
```

- **Attempt chain** — `parent_attempt_id` walks back to root in
  one foreign key hop. A `Refine` fold reads
  `Vec<InferenceAttempt>` directly from the store.
- **PROV-O continuity** — every LLM-driven attempt carries a
  `provenance_id` (Φ8.1 substrate); deterministic attempts
  (pre-LLM Validate reject) leave it `None`.
- **Append-only** — re-running a session creates a new session
  id; re-trying within a session creates a new attempt with
  `attempt_index + 1`. `(session_id, attempt_index)` is
  `UNIQUE`. Concurrent writers race only via this constraint;
  the store retries once on conflict.

`query_ir_candidate` is `Option<serde_json::Value>` (not
typed `QueryIR`) so `ox-ontology` does not pull `ox-query-ir` —
the layering arrow stays `ox-core ← ox-ontology ← ox-query-ir`.
Consumers (Brain, Agent) deserialise via the typed shape.

## Schema

```sql
CREATE TABLE inference_sessions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    question TEXT NOT NULL,
    initiator JSONB NOT NULL,
    final_outcome JSONB,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    CONSTRAINT inference_sessions_outcome_aligns_ended_at
        CHECK ((final_outcome IS NULL) = (ended_at IS NULL))
);

CREATE TABLE inference_attempts (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES inference_sessions(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL,
    parent_attempt_id UUID REFERENCES inference_attempts(id) ON DELETE CASCADE,
    attempt_index INT NOT NULL,
    emitted_at_stage TEXT NOT NULL,
    query_ir_candidate JSONB,
    outcome JSONB NOT NULL,
    provenance_id UUID REFERENCES provenance_records(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (session_id, attempt_index)
);
```

4-clause RLS on both tables (ENABLE / FORCE / `ws_isolation` /
`system_bypass`) per `crates/ox-store/CLAUDE.md`. The
`emitted_at_stage TEXT` column carries a `CHECK` against the 9
known wire strings — forward-compat with new stages is a Rust
edit + a CHECK alter, no other DDL.

`InferenceSessionStore` trait
(`crates/ox-store/src/store/inference.rs`) + Postgres impl
(`crates/ox-store/src/postgres/inference.rs`) — five methods:
`create_inference_session`, `get_inference_session`,
`record_inference_attempt`, `list_inference_attempts`,
`complete_inference_session`. Stamps a `provenance_records`
row inline via `ProvenanceStore::record_activity` when the
attempt carries a capture.

## Consequences

- **Compile-time totality.** A new `PipelineStage` variant added
  to the enum without updating `TRANSITIONS` fails the
  `const _: () = { … assert!(found == 1) … }` check at build
  time. The runtime never sees a missing transition.
- **Typed attempt history.** Refine's ICL fold reads
  `list_inference_attempts(session_id)` and gets a structured
  `Vec<InferenceAttempt>` — `outcome.message` from each prior
  attempt becomes the next prompt's "previously tried, here's
  why it failed" block. No log archaeology.
- **Audit DAG completeness.** Provenance is FK-chained:
  session → attempt → `provenance_records` → `prompt_render_hash`.
  Any judged outcome traces to the exact bytes that produced
  it.
- **Observability shape.** Every attempt's
  `emitted_at_stage` is the canonical answer to "where did
  this run fail" — no per-tool-name reconstruction.
- **Substrate-only ship.** Φ9.1+9.2 lands the types + tables
  + store trait. Φ9.3 wires the agent's tool-loop to use the
  state machine; Φ9.4 emits OTel `gen_ai.*` spans per stage.
  Each follow-up phase is independent — the substrate is
  consumable as soon as it lands.

## Alternatives considered

- **Procedural state machine via `enum`-driven match in the
  agent loop.** Rejected — gives the same shape but without the
  compile-time totality guarantee. New stage forgotten in match
  is a `_ => panic!`, not a build error.
- **External `statig` / `state_machine_future` crate.**
  Rejected — adds a heavyweight dep for a pattern Rust enums +
  const tables already express. Internal types stay readable;
  the const-fn assertion is the totality guard the libraries
  would otherwise provide.
- **Persist only the final attempt.** Rejected — Refine's ICL
  fold needs the full chain. Storing only the last attempt
  loses the "why did try 1 fail" context that try 2's prompt
  consumes. Persistent multi-attempt history is the whole
  point.
- **Compute `attempt_index` client-side.** Rejected — race
  window between read and insert. The Postgres `INSERT … SELECT
  COALESCE(MAX(attempt_index) + 1, 0)` form pushes the index
  resolution into the same SQL statement; the `UNIQUE
  (session_id, attempt_index)` constraint catches the (rare)
  race between two writers, and the store retries once.

## Φ9.3 — Brain + Agent integration (landed 2026-05-08f)

The substrate above gained its first end-to-end producer in
Φ9.3:

- `InferenceContext` task-local + `scope_inference_context` +
  `current_inference_context` (`crates/ox-store/src/inference.rs`)
  mirror the `EvaluationContext` pattern. Outer pipeline drivers
  open a session, bind the context, run the Brain inside it.
- `run_in_inference_session(store, question, initiator, body)`
  helper opens / scopes / finalises a session in one call. On
  `Ok`, resolves the winning attempt id from
  `list_inference_attempts` and stamps
  `SessionOutcome::Success`. On `Err`, classifies the error +
  stamps `SessionOutcome::Rejected`.
- `DefaultBrain.inference_session_store: Option<Arc<dyn>>` —
  optional like `evaluation_capture`. Wired at startup from the
  same `PostgresStore` Arc the rest of the platform shares.
- `Brain::translate_query` records one `InferenceAttempt` per
  call. Tier1/2/3 fallback + label-correction retry are folded
  into one logical attempt at the InferenceSession layer; the
  Agent's outer loop is what produces multi-attempt chains via
  re-invocations across separate sessions.
- Three production callers wrapped:
  `crates/ox-agent/src/tools/query_graph.rs`,
  `crates/ox-api/src/mcp.rs`,
  `crates/ox-api/src/routes/evaluation.rs::execute_evaluation_case`.
  All three feed the `run_in_inference_session` helper with an
  appropriate `AgentRef` (User for chat, Service for MCP +
  evaluation runs).

`ox-api/src/main.rs` attaches the inference store via
`with_inference_session_store(...)` alongside the existing
`with_evaluation_capture` / `with_knowledge` chain.

## Φ9.4 — OTel GenAI span emission (landed 2026-05-08j)

Every LLM call funnels through
`DefaultBrain::call_structured_traced` (translate_query,
design_ontology, edit_ontology, judge_evaluation_case,
judge_safety_evaluation_case, repo_analyzer, …). Φ9.4 wraps
that single funnel in a `gen_ai.call` span carrying the OTel
GenAI semantic conventions:

- `gen_ai.operation.name` — the operation tag
  (`translate_match_query`, `evaluation_judge`, …).
- `gen_ai.system` — the resolved provider (`anthropic`,
  `openai`, `google`, `bedrock`, …).
- `gen_ai.request.model` — provider-prefixed model identifier.
- `gen_ai.request.max_tokens` — effective token cap after
  template / model-config / runtime resolution.
- `gen_ai.request.temperature` — when set; field is omitted
  when `None`.
- `gen_ai.usage.input_tokens` — provider-reported after
  completion.
- `gen_ai.usage.output_tokens` — provider-reported after
  completion.

Implemented via `tracing::info_span!` (the
`#[tracing::instrument]` proc-macro rejects quoted-string
field keys, but `info_span!` accepts them per the tracing
field-name syntax). Fields land empty at span entry and are
stamped via `span.record(...)` as the call progresses — an
OTLP collector (Phoenix Arize, Langfuse, Honeycomb, Helicone)
that recognises the GenAI conventions auto-categorises every
call as an LLM request without a downstream mapper.

Single funnel wins: instrumenting one method covers every
LLM call site in the codebase (~10 trait methods × N callers).
No per-site sweep needed.

## Outstanding (next phases)

- **Φ9.5 — Refine-stage ICL fold over `prior_attempts`.**
  `Brain::translate_query` currently records attempts but
  doesn't yet read them. The next iteration accepts
  `prior_attempts: &[InferenceAttempt]` and folds them into
  the prompt's `correction` block — failed attempts within
  the agent's outer loop become structured ICL the LLM uses
  to self-correct.

## References

- `crates/ox-ontology/src/inference_pipeline.rs`
- `crates/ox-store/src/store/inference.rs`
- `crates/ox-store/src/postgres/inference.rs`
- `crates/ox-store/migrations/0001_schema.sql` (inference
  tables section)
- ADR-0008 — PROV-O alignment (substrate)
- ADR-0030 — `ProvenanceCapture` as required argument (FK
  source for `inference_attempts.provenance_id`)
- ADR-0028 — `PromptBudget` (the OutOfBudget classification
  this state machine carries)
