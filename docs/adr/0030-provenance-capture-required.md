# ADR-0030 — `ProvenanceCapture` as required mutation argument

## Status

Accepted — Φ8.1, 2026-05-08.

## Context

PROV-O `ProvenanceDef` (`crates/ox-ontology/src/provenance.rs`) is
the platform's typed audit shape — Activity, Agent, Entity, plan
reference (template id + version + `prompt_render_hash` from
ADR-0029), bitemporal axes. The struct landed early and was wired
into `OntologyIR` as a collection.

The producer side never followed. A repository-wide grep found
**zero** production write sites: every mutation that produced a
fact (committing an ontology version, judging an evaluation case,
executing an action) ran without stamping a row. The audit DAG
PROV-O was meant to provide existed in type theory only.

A typical post-incident question — "what activity produced this
ontology version, with what prompt, and which earlier activity
informed it?" — had no answer because no one had been writing the
rows. Fixing this with "remember to call `record_activity` after
every mutation" is the wrong shape: 95% of the time the call gets
omitted, the audit becomes scattered, and a new mutation is one
forgotten line away from regressing the invariant.

## Decision

Provenance stamping is enforced at the **function-signature**
level. Every mutation that produces a fact takes a typed
[`ProvenanceCapture`] as a *required* argument. The Rust compiler
refuses to call the function without it. New mutations inherit
the invariant by construction — they simply cannot ship without
the bundle.

**`ProvenanceCapture`** (`crates/ox-ontology/src/provenance.rs`)
— pre-validation input shape carrying every PROV-O field except
the three the producer knows for itself:

```rust
pub struct ProvenanceCapture {
    pub activity: ProvenanceActivityKind,
    pub agent: AgentRef,
    pub plan: Option<ProvenancePlan>,
    pub used: Vec<EntityRef>,
    pub derived_from: Vec<EntityRef>,
    pub was_informed_by: Vec<ProvenanceId>,
    pub ontology_valid_at: Option<DateTime<Utc>>,
    pub data_valid_at: Option<DateTime<Utc>>,
}

impl ProvenanceCapture {
    pub fn ontology_edit(agent: AgentRef, command_summary: impl Into<String>) -> Self;
    pub fn draft_proposal(plan: ProvenancePlan, model_id: impl Into<String>) -> Self;
    pub fn with_used(self, used: impl IntoIterator<Item = EntityRef>) -> Self;
    pub fn with_derived_from(self, derived_from: impl IntoIterator<Item = EntityRef>) -> Self;
    pub fn informed_by(self, prior: ProvenanceId) -> Self;
    pub fn into_def(self, id: ProvenanceId, subject: EntityRef) -> ProvenanceDef;
}
```

The store fills in `id` (fresh UUID v7), `at_time` (insert wall
clock), and `subject` (the entity the producer just produced).

**`ProvenanceStore`** (`crates/ox-store/src/store/provenance.rs`)
— `record_activity(capture, subject) -> ProvenanceId`. Workspace-
scoped via the bound task-local; rejects without `WORKSPACE_ID`
set. Backed by the new `provenance_records` table.

## Wiring

Three producer sites land in Φ8.1, each with the required
argument folded into its trait/handler signature:

1. **`OntologyVersionStore::commit_version`**
   (`crates/ox-store/src/store/ontology_version.rs`) takes
   `capture: ProvenanceCapture` after the existing
   `commit_message`. The impl
   (`crates/ox-store/src/postgres/ontology_version.rs`) calls
   `record_activity` first, then writes the snapshot row with
   `provenance_id NOT NULL` resolved from the recorded id. ON
   DELETE RESTRICT — the audit row outlives the snapshot.

2. **RAGAS judge handlers**
   (`crates/ox-api/src/routes/evaluation.rs::judge_evaluation_case`
   + `crates/ox-api/src/background/eval_judge.rs::judge_one_ragas`)
   build a capture from the LLM call's `CallProvenance`
   (prompt id + version + render hash + model id),
   `record_activity` against the case as subject, then attach
   `provenance_id` to every emitted metric row. The
   `EvaluationJudge` trait now returns
   `(EvaluationJudgement, CallProvenance)` so callers can
   stamp the activity without duplicating the brain's resolver
   round-trip.

3. **Safety judge handlers**
   (`crates/ox-api/src/routes/evaluation.rs::judge_safety_evaluation_case`
   + `crates/ox-api/src/background/eval_judge.rs::judge_one_safety`)
   — same shape, distinct prompt template, distinct rubric.
   `EvaluationSafetyJudgeApi` returns
   `(EvaluationSafetyJudgement, CallProvenance)`.

`evaluation_metrics.provenance_id UUID` (NULLABLE, ON DELETE
RESTRICT) carries the FK denormalised for fast filtering. Capture-
axis observations (latency / tokens / cost) leave it `None` —
their provenance is the underlying LLM call, attached to the case
via `EvaluationCaseMetadata::Call.prompt_render_hash`. Judge-
produced rows always carry `Some(provenance_id)`.

## Schema

```sql
CREATE TABLE provenance_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    subject JSONB NOT NULL,
    activity JSONB NOT NULL,
    agent JSONB NOT NULL,
    plan JSONB,
    used JSONB NOT NULL DEFAULT '[]'::jsonb,
    derived_from JSONB NOT NULL DEFAULT '[]'::jsonb,
    was_informed_by UUID[] NOT NULL DEFAULT '{}',
    at_time TIMESTAMPTZ NOT NULL DEFAULT now(),
    ontology_valid_at TIMESTAMPTZ,
    data_valid_at TIMESTAMPTZ
);

ALTER TABLE ontology_version_snapshots
    ADD COLUMN provenance_id UUID NOT NULL
        REFERENCES provenance_records(id) ON DELETE RESTRICT;

ALTER TABLE evaluation_metrics
    ADD COLUMN provenance_id UUID
        REFERENCES provenance_records(id) ON DELETE RESTRICT;
```

JSONB shape mirrors `ox_ontology::ProvenanceDef` directly. RLS
4-clause (ENABLE / FORCE / `ws_isolation` / `system_bypass`) per
`crates/ox-store/CLAUDE.md`.

## Consequences

- **Audit DAG is structurally complete.** Every committed
  ontology version + every judge-produced metric resolves to a
  queryable provenance row. New mutations cannot be added without
  the bundle — the compiler refuses.
- **`prompt_render_hash` is reachable.** ADR-0029 promised
  deterministic LLM replay; until Φ8.1, the hash had no producer.
  The judge wiring above is the first end-to-end use: the row
  carries the exact bytes that fed the judge model.
- **`derived_from` chains compile.** A version-2 commit derives
  from version-1; the FE version-history panel can walk the chain
  by id rather than reconstructing from `parent_version_id`. A
  judge-row's `used` field points at the `evaluation_run` it
  scored against — the audit trail spans run → case → judgement
  in one walk.
- **No backwards compatibility shim.** The trait signature change
  ripples through 6 production call sites + 3 integration test
  fixtures; all are migrated in the same PR. There is no
  `commit_version_without_capture` escape hatch. New mutations
  that need provenance simply take the bundle; ones that don't
  produce facts (read-only queries, idempotent caches) are
  outside the scope.
- **Future producers wire identically.** Φ8.1 covers ontology
  commit + RAGAS / safety judging. `ActionStore::execute_action`
  + `query_graph` tool + source-scan paths follow in subsequent
  phases under the same trait pattern. The substrate is shared.

## Alternatives considered

- **Optional capture argument with `Option<ProvenanceCapture>`.**
  Rejected — defeats the entire purpose. 95% of call sites would
  pass `None` and the audit would stay scattered.
- **Macro-based interception (`#[record_provenance]` attribute).**
  Rejected — hides the contract. The function signature is the
  single most-readable place to express "this mutation requires
  provenance"; a derive macro buries it under indirection.
- **Separate `ProvenanceWriter` trait that callers must hold a
  reference to.** Rejected — same shape as the trait/method
  argument, but with extra wiring cost on every caller. The
  argument-on-the-mutation form is simpler and equally enforced.

## References

- `crates/ox-ontology/src/provenance.rs` — `ProvenanceCapture`
- `crates/ox-store/src/store/provenance.rs` — trait
- `crates/ox-store/src/postgres/provenance.rs` — impl
- `crates/ox-store/migrations/0001_schema.sql` — schema
- `crates/ox-store/src/store/ontology_version.rs` — commit
  signature
- `crates/ox-api/src/routes/evaluation.rs` — judge handlers
- `crates/ox-api/src/background/eval_judge.rs` — async cron
- ADR-0008 — W3C PROV-O aligned provenance (substrate)
- ADR-0029 — `prompt_render_hash` (the LLM-call fingerprint this
  capture wires through)
