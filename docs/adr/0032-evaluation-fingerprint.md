# ADR-0032 — `EvaluationFingerprint` typed reproducibility bundle

## Status

Accepted — Φ8.3 + Φ8.4, 2026-05-08.

## Context

Evaluation runs lost reproducibility silently. Three failure modes
were active in `evaluation_runs`:

1. **`ontology_version_id UUID NULLABLE`** with `ON DELETE SET NULL`
   (`crates/ox-store/migrations/0001_schema.sql`). A run created
   without an ontology pin, or whose snapshot was later
   garbage-collected, lost the schema context that made its scores
   interpretable. RAGAS faithfulness from one quarter could not be
   compared against the next quarter under schema drift.
2. **Reproducibility pins scattered across columns and JSONB.**
   `ontology_version_id` lived as a column, `model_id` lived inside
   `metadata.call_provenance`, `prompt_render_hash` lived per-case.
   No single equality token answered "are these two runs configured
   the same way?" — an operator running A/B regression ran a fanout
   JOIN across three tables.
3. **Three observation methods on `EvaluationCapture`**:
   `record_latency`, `record_tokens`, `record_cost_usd`. Cost was
   computed from a hardcoded tariff (`estimated_cost_micro_usd` in
   `ox-brain`); a pricing revision required code release. Three
   call sites, three independent metric rows per axis, no
   guarantee that all three landed (a partial failure left a
   call's latency without its tokens or its cost).

Together these meant: a regression in faithfulness six months
post-ship was not attributable. Was the model worse? Did the
ontology drift? Did the dataset change? Did decoding parameters
shift? The substrate did not pin enough to answer.

## Decision

Two coupled types in `ox-ontology`:

**`EvaluationFingerprint`** (`crates/ox-ontology/src/eval_fingerprint.rs`)
— typed bundle every run pins at construction. Fields:

```rust
pub struct EvaluationFingerprint {
    pub ontology_version_id: Uuid,
    pub dataset_id: Uuid,
    pub model_id: ModelId,
    pub prompt_template_id: Option<PromptTemplateId>,
    pub prompt_template_version: Option<String>,
    pub decoding_config_hash: Option<ConfigHash>,
    pub retrieval_profile_id: Option<RetrievalProfileId>,
}
```

`digest()` returns the SHA-256 of the canonical-JSON serialisation.
Two runs are configured identically iff their digests match — the
single equality token, persisted alongside the run.

`EvaluationFingerprintInput` is the wire DTO: free-form
`decoding_config: serde_json::Value` is hashed canonically into
`ConfigHash` on the way into `into_fingerprint()`.

**`ModelCall` + `ModelPrices`** (`crates/ox-ontology/src/model_pricing.rs`)
— one self-describing observation per LLM call (`model_id`,
`input_tokens`, `output_tokens`, `cached_input_tokens`,
`latency_ms`); pricing in a temporal `model_prices` catalogue
(half-open `[valid_from, valid_to)` validity windows; price
revisions land as new rows, never in-place edits). Cost is
*derived* at write time via `ModelCall::cost_micro_usd(prices)`
and persisted as the historical truth.

The `EvaluationCapture` trait shrinks to one method:

```rust
async fn record_call(
    &self,
    ctx: &EvaluationContext,
    operation: &str,
    call: ModelCall,
) -> OxResult<()>;
```

The `PostgresStore` impl fans this into five
`evaluation_metrics` rows (`latency_ms`, `tokens.input`,
`tokens.output`, `tokens.cached_input`, `cost_micro_usd`) sharing
the operation tag, with cost resolved from the active
`model_prices` row.

## Schema

```sql
CREATE TABLE evaluation_runs (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    ontology_version_id UUID NOT NULL
        REFERENCES ontology_version_snapshots(id) ON DELETE RESTRICT,
    dataset_id UUID NOT NULL
        REFERENCES evaluation_datasets(id) ON DELETE RESTRICT,
    model_id TEXT NOT NULL,
    fingerprint_digest VARCHAR(64) NOT NULL,
    fingerprint_components JSONB NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT evaluation_runs_fingerprint_digest_shape
        CHECK (length(fingerprint_digest) = 64),
    CONSTRAINT evaluation_runs_model_id_non_empty
        CHECK (length(model_id) > 0)
);

CREATE TABLE model_prices (
    model_id TEXT NOT NULL,
    input_price_usd_per_million DOUBLE PRECISION NOT NULL,
    cached_input_price_usd_per_million DOUBLE PRECISION NOT NULL,
    output_price_usd_per_million DOUBLE PRECISION NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_to TIMESTAMPTZ,
    PRIMARY KEY (model_id, valid_from)
);
```

Denormalised columns (`ontology_version_id`, `dataset_id`,
`model_id`) carry the FK constraint and the index; the JSONB
stays the extensible source of truth. Adding a new pin (e.g.
`retrieval_profile_id` when Φ10 lands) requires no schema
migration — the JSONB shape extends, new digests reflect the new
field, old digests stay equal to themselves so historical
runs survive untouched.

## Consequences

- **Reproducibility is enforced at the type level.** Construction
  of `EvaluationRun` requires a fully-pinned
  `EvaluationFingerprint`; a malformed caller cannot persist a run
  without an ontology version. The platform refuses to author an
  uninterpretable run.
- **Cost stays auditable across tariff changes.** A 2026-Q3 price
  revision lands as a new `model_prices` row; runs from Q2 retain
  the cost computed under the Q2 row. Re-running an old run does
  not silently rewrite history — historical metric rows persist.
- **Capture failure modes converge.** One `record_call` call site
  in `ox-brain::call_structured_traced` either succeeds or logs +
  drops as a unit; a partial-row state where latency landed but
  tokens did not is structurally impossible.
- **Live chat sampler requires canonical ontology.** Greenfield
  workspaces (no canonical version yet) skip sampling rather than
  authoring an unpinned run — `eval_sampler::ensure_live_samples_run`
  returns `None` until a commit lands. This is the right
  trade-off: the alternative (legal unpinned runs) reintroduces
  the silent-loss footgun the redesign exists to remove.
- **Per-model live runs.** `live_chat_samples` becomes
  `live_chat_samples:<model_id>` so the run-level fingerprint pins
  one model coherently. Multi-model traffic fans into distinct
  runs rather than mixing scores under an inconsistent pin.
- **No backwards compatibility.** `EvaluationCallProvenance` is
  removed; per-case Call metadata carries only `prompt_render_hash`
  (the only per-case-varying datum). The three-method capture
  trait is gone. The migration baseline is rewritten in place
  — the redesign assumes greenfield deployment.

## Alternatives considered

- **Keep separate columns for each pin.** Rejected — every new
  dimension forces a schema migration. The JSONB-backed digest
  pattern lets retrieval_profile_id (Φ10), prompt cache hit ratio
  (future), and any other reproducibility axis ride on the same
  shape.
- **Compute cost lazily from price catalogue at read time.**
  Rejected — historical cost would shift under tariff revisions.
  The metric loop's primary use case is "did Q3 cost more than
  Q2", which requires Q2 prices to stay fixed in Q2 rows.
- **Make `prompt_template_id` required.** Rejected — deterministic
  retrieval-only scoring (precision@k, recall@k, MRR, NDCG@k)
  invokes no LLM and has no template. The optional pair
  (`prompt_template_id` + `prompt_template_version`) names the
  applicable subset.

## References

- `crates/ox-ontology/src/eval_fingerprint.rs`
- `crates/ox-ontology/src/model_pricing.rs`
- `crates/ox-store/src/evaluation.rs` — trait + DTO surface
- `crates/ox-store/src/postgres/evaluation.rs` — capture impl
- `crates/ox-store/migrations/0001_schema.sql` — schema
- `crates/ox-api/src/routes/evaluation.rs` — HTTP handlers
- `crates/ox-api/src/eval_sampler.rs` — online sampler integration
- ADR-0018 — `EvaluationStore` three-table loop (substrate this
  builds on)
- ADR-0029 — `prompt_render_hash` (per-case fingerprint that
  pairs with this run-level fingerprint)
