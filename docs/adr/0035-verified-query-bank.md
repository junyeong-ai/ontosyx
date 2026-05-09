# ADR-0035 — `VerifiedQueryDef` typed verified Q→IR bank

## Status

Accepted — Φ11.1, 2026-05-08.

## Context

Vanna.AI's foundational pattern is `train(question, sql)` —
operator-validated `(natural-language question, structured
query)` pairs land in a vector store, RAG retrieves the top-k
most-similar priors at NL→SQL time, the LLM gets them as ICL
exemplars. The result: a dramatic accuracy lift + cost
reduction (the LLM anchors against a known-working pattern
instead of inferring from schema alone).

Pre-Φ11 Ontosyx had two adjacent surfaces but no positive-
example bank:

- **`KnowledgeStore`** — failure-driven corrections (the
  `RecoveryDetectionHook` auto-records "Q failed → Q corrected"
  pairs). Negative examples + corrections, not positive
  exemplars.
- **`EvaluationDataset`** — golden Q→IR pairs for evaluation
  (RAGAS / regression). Eval-side, not for runtime ICL
  injection.

The audit (ontive comparative deep-dive, 2026-05-07) flagged
this as a P0 NL2SQL gap: every competitor (Vanna, Snowflake
Cortex Analyst, MAC-SQL) exploits a verified-pair bank for
runtime ICL; Ontosyx's translate path went straight from raw
schema RAG to LLM with no exemplar layer.

## Decision

Promote operator-promoted Q→IR pairs into typed first-class
data: `VerifiedQueryDef` collection in `ox-ontology` +
`verified_queries` table in `ox-store` + `VerifiedQueryStore`
trait + Postgres impl.

**`VerifiedQueryDef`** (`crates/ox-ontology/src/verified_query.rs`):

```rust
pub struct VerifiedQueryDef {
    pub id: VerifiedQueryId,
    pub workspace_id: Uuid,
    pub question: String,
    pub question_hash: String,    // sha256 of canonicalised question
    pub query_ir: serde_json::Value,
    pub complexity_class: ComplexityClass,
    pub status: VerifiedQueryStatus,
    pub author: AgentRef,
    pub description: String,
    pub verified_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum ComplexityClass { Trivial, Simple, Composite, Complex }
pub enum VerifiedQueryStatus { Verified, UnderReview, Deprecated, Stale }
```

Two closed enums encode the gating policy:

- **`ComplexityClass::is_icl_eligible()`** returns `false` for
  `Trivial` only. The Brain's exemplar retriever (Φ11.2) will
  filter on this. Trivial queries (single label match, no
  joins) carry too little structural signal — ontive's
  experience: trivial few-shots produced over-literal LLM
  outputs that didn't generalise.
- **`VerifiedQueryStatus::is_retrievable()`** returns `true`
  only for `Verified`. The other states (`UnderReview` /
  `Deprecated` / `Stale`) park rows out of the retrieval pool
  but keep the audit lineage.

### Question canonicalisation + hash

```rust
pub fn canonicalize_question(q: &str) -> String {
    // trim, lowercase, collapse internal whitespace
}
pub fn question_hash(q: &str) -> String {
    // SHA-256 of canonicalize_question, lowercase hex
}
```

UPSERT key on `(workspace_id, question_hash)` — promoting the
same question twice (or with cosmetic variations) collapses to
one row. Operator-side `SaveAsVqr` is idempotent.

### Schema

```sql
CREATE TABLE verified_queries (
    id TEXT PRIMARY KEY,
    workspace_id UUID NOT NULL,
    question TEXT NOT NULL,
    question_hash VARCHAR(64) NOT NULL,
    query_ir JSONB NOT NULL,
    complexity_class TEXT NOT NULL,
    status TEXT NOT NULL,
    author JSONB NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, question_hash),
    CONSTRAINT verified_queries_question_hash_shape
        CHECK (length(question_hash) = 64),
    CONSTRAINT verified_queries_complexity_class_recognised
        CHECK (complexity_class IN ('trivial', 'simple', 'composite', 'complex')),
    CONSTRAINT verified_queries_status_recognised
        CHECK (status IN ('verified', 'under_review', 'deprecated', 'stale'))
);
```

Three indexes:

- `(workspace_id, updated_at DESC)` — admin surface "newest
  first" listing.
- `(workspace_id, status) WHERE status = 'verified'` — partial
  index, the Brain's hot retrieval path.
- `gin (question gin_trgm_ops)` — admin filter box trigram
  search. The Brain's *embedding* retrieval lives separately
  (Φ11.2).

4-clause RLS (ENABLE / FORCE / `ws_isolation` / `system_bypass`).

### `VerifiedQueryStore` trait

Seven methods spanning the canonical CRUD surface plus two
domain operations:

- `upsert_verified_query` — natural-key UPSERT.
- `get_verified_query(id)`, `find_verified_query_by_hash` —
  lookups.
- `list_verified_queries(status_filter, limit)` — admin paging.
- `transition_verified_query_status` — `Verified ↔ Deprecated`
  / `UnderReview → Verified` / `Verified → Stale` (cron).
- `delete_verified_query` — hard delete.
- `search_verified_queries_by_text(query_text, complexity, limit)`
  — trigram-similarity ranked browse for the admin filter
  box. Embedding-similarity retrieval is the Brain's path
  (Φ11.2).

## Consequences

- **Audit-flagged NL2SQL gap closed at the substrate level.**
  Operators can now persist verified Q→IR pairs; the
  freshness cron tracks schema drift via the `Stale` state.
- **Cross-version durable.** Verified queries persist across
  ontology version commits — a schema edit doesn't drop the
  bank. The freshness cron (Φ11.3) flips `Stale` when the IR
  references entities that no longer exist.
- **Trivial-class gate exists at type level.** The Brain's
  exemplar retriever cannot accidentally inject a degenerate
  pattern as ICL — `ComplexityClass::is_icl_eligible()` is the
  single decision point.
- **Vanna-style fast path.** A future translate-query
  short-circuit hits the persisted exact match by
  `find_verified_query_by_hash` and returns the IR without an
  LLM call. ontive's "RETRIEVE high-tier τ skip-stages" pattern
  becomes a 1-line check.
- **Substrate-only ship.** Φ11.1 lands the types + table +
  trait + impl. Φ11.2 wires Brain `translate_query_inner` to
  retrieve top-k similar verified queries + inject as ICL.
  Φ11.3 ships the freshness cron. Φ11.4 ships the operator-
  facing FE surface (`SaveAsVqr` + admin review queue).

## Why no `IrCollection` impl

Verified queries are *not* IR collection members — they live
alongside the ontology, not inside any committed ontology
version snapshot. Ontology versions snapshot the schema; the
verified-query bank is operational data that survives across
schema commits. Treating them as IR-extracted entities would
duplicate every committed version with the entire bank, which
is the wrong semantic.

The `Def` suffix follows naming conventions for typed
identity ergonomics (`ConceptDef`, `RuleDef`, …), but the type
is intentionally workspace-level not version-level.

## Alternatives considered

- **Vector embedding column on the same row.** Rejected for
  Φ11.1 — embedding is a separate concern (Brain's vector
  retrieval lives in `ox-memory`'s vector store). Adding the
  column at substrate time bakes the embedding model + dim
  choice into the schema; landing it in Φ11.2 lets the Brain
  pick the embedding pipeline alongside the retrieval logic.
- **Single `query_ir TEXT` column** (raw JSON string).
  Rejected — the freshness cron needs to walk the IR's
  referenced labels / properties without re-parsing. JSONB
  gives indexed-path access (`query_ir #> '{labels}'` etc.)
  for free.
- **Reuse `KnowledgeStore` for verified queries.** Rejected —
  KnowledgeStore is failure-driven (label corrections from
  the recovery hook). Mixing positive exemplars with
  failure entries forces every consumer to filter by entry
  kind; separation matches the read patterns.
- **Tie verified queries to `ontology_version_id`** (per-
  version bank). Rejected — operators promote queries against
  business semantics that survive schema cosmetics
  (rename / property reorganisation). Stale tracking + the
  freshness cron is the right granularity, not full version
  pinning.

## Φ11.2 — Brain exact-hash short-circuit (landed 2026-05-08l)

Half of Φ11.2 lands in this same ADR — the exact-hash
short-circuit. Top-k similar-search ICL injection (Φ11.2b)
defers because it requires a prompt-template revision; this
half is purely substrate-consumer wiring with zero prompt
change.

`DefaultBrain.verified_query_store: Option<Arc<dyn
VerifiedQueryStore>>` field + `with_verified_query_store(...)`
builder. `translate_query_inner` opens with a
`try_verified_query_cache(question, ctx)` probe:

1. `question_hash(question)` → canonical SHA-256.
2. `find_verified_query_by_hash(hash)` against the active
   workspace.
3. Hit + `status.is_retrievable()` → deserialise IR, return
   with synthetic `CallProvenance` (`prompt_id =
   "verified_query_cache"`, `provider = "ontosyx-cache"`,
   `prompt_render_hash = "vq:{hash}"`).
4. Miss / non-retrievable status / IR deserialise failure /
   store error → fall through to the full LLM translate
   path. Every branch logs the outcome via
   `ctx.progress("verified_query_lookup")` so the agent UI
   surfaces cache hits + misses.

Wire-up: `ox-api/src/main.rs` attaches the same
`Arc<dyn VerifiedQueryStore>` (the canonical `PostgresStore`)
on brain construction.

Synthetic `CallProvenance` is honest about its origin — the
`vq:` prefix on the render hash + the literal
`"verified_query_cache"` prompt id ensure downstream consumers
(eval-case provenance, audit DAG, dashboard) can distinguish
cache-hit attempts from LLM-driven ones in a one-line filter.

## Φ11.2b — Top-k ICL exemplar injection (landed 2026-05-08o)

Cache-miss path now retrieves up to 3 verified-query rows per
translate call and renders them as a workspace-validated
in-context-learning block injected via the
`{{verified_examples}}` placeholder of `translate_match_query`.
Pure trigram similarity (no embedding dependency) — Φ11.5 is the
semantic-similarity upgrade.

The retrieval gates live inline at the SQL layer in the new
`VerifiedQueryStore::search_verified_queries_for_icl` method:

```sql
WHERE question % $1
  AND status = 'verified'
  AND complexity_class <> 'trivial'
ORDER BY similarity(question, $1) DESC, updated_at DESC
LIMIT $2
```

Both gates are mandatory and must be enforced server-side —
letting a caller forget the status filter would surface
UnderReview rows as exemplars (regressing the bank's curation
guarantee), and forgetting the complexity filter would inject
Trivial 1-line patterns that carry no structural signal worth
the prompt budget. The SHA-1 mirrors `ComplexityClass::is_icl_eligible`
and `VerifiedQueryStatus::is_retrievable` in code.

### Brain wiring

`DefaultBrain::retrieve_verified_examples(question, ctx)` is the
helper. It runs strictly *after* `try_verified_query_cache`
returns `None` — an exact-hash hit short-circuits before any
top-k retrieval, so the bank-row that matches the question
verbatim never duplicates itself into its own ICL block.

The retriever returns `""` on three paths, all silently
absorbed by the placeholder:

- no `verified_query_store` attached (greenfield deployments),
- empty result set (cold-start bank, or every match got filtered),
- store error (logged at `warn`, falls through to the LLM
  *without* exemplars — observability, not load-bearing).

A `verified_query_icl` progress event with
`outcome ∈ {hit, empty, failed}` lands on the execution context
so the agent's progress stream surfaces ICL hit-rate without a
separate metric.

### Block shape

When the retrieval finds k ≥ 1 eligible rows the placeholder
substitutes:

```markdown
## Verified examples (workspace-validated patterns)

These QueryIR shapes were promoted by an operator after a
successful run. Treat them as authoritative templates — match
the structural pattern when the question is analogous.

### Q: <verified question>
```json
{ ... pretty-printed query_ir ... }
```

### Q: <next match>
...
```

The header copy explicitly anchors the LLM on "treat these as
authoritative templates" because raw exemplars (without the
framing) frequently get treated as *suggestions* the model
freely deviates from. Operator-curated patterns deserve more
prompt-engineering weight than that.

### Top-k = 3 rationale

Beyond k=3 the prompt-budget regression dominates the retrieval
lift on a trigram ranker. Φ11.5 (embedding swap to pgvector)
raises the ceiling because semantic similarity returns higher-
quality top-k tail rows, but until then 3 is the sweet spot
matching Vanna.AI's published default.

### Forward-compat notes

- The `{{verified_examples}}` placeholder substitution is
  no-op-safe — DB-stored prompts still on the pre-Φ11.2b body
  silently drop the variable insertion (no rendered literal
  leaks). Greenfield deploys seed the new template body.
- `VerifiedQueryStore::search_verified_queries_for_icl` is a
  separate trait method from `search_verified_queries_by_text`
  precisely because ICL retrieval needs the gates baked in;
  the admin browse path takes operator-supplied filters.

## Φ11.5 — Embedding column + semantic NN (landed 2026-05-08s)

`verified_queries.embedding vector(1024)` carries the dense
representation of the canonical question. The dimension matches
the workspace's default multilingual embedding model (the same
`Arc<dyn EmbeddingProvider>` `MemoryStore` shares); a deploy
that swaps the model also swaps the schema's vector dimension,
so the contract stays consistent.

A partial HNSW index over `vector_cosine_ops` accelerates the
top-K cosine lookup; `WHERE embedding IS NOT NULL` keeps the
index small while the bank bootstraps. Cold rows (no embedding
yet) are silently absent from the semantic ranker — the trigram
retriever still surfaces them upstream.

### Capture path

`POST /api/verified-queries` calls
`embed_question_for_verified_query(state, &question)` which
reads `state.memory.embedder()` and embeds the canonical
question with `EmbeddingRole::Document`. Cold-start deployments
(no `MemoryStore` attached) and embed-call failures fall through
to `embedding = None`; the row promotes successfully and rejoins
the trigram retriever upstream.

### Retrieval path

`DefaultBrain::retrieve_verified_examples` now prefers the
semantic NN when an embedder is attached:

1. Embed the user's question with `EmbeddingRole::Query`.
2. Call `search_verified_queries_by_embedding(&query_vec, 3)` —
   filters `embedding IS NOT NULL AND status = Verified AND
   complexity_class != Trivial`, orders by cosine distance.
3. If the result set is non-empty, render those rows and tag
   the `verified_query_icl` progress event with
   `retrieval_mode = "embedding"`.
4. Otherwise fall through to the trigram retriever
   (`retrieval_mode = "trigram"`) — the cold-start path stays
   warm.

Embed failures and store-side errors log at `warn` and fall
through; ICL retrieval is observability-grade, not load-
bearing.

### Why share the embedder Arc

Both `MemoryStore` and `DefaultBrain` hold the same
`Arc<dyn EmbeddingProvider>`. Sharing keeps the dimension
contract trivially aligned: the schema column type
`vector(1024)` is set by the workspace's chosen model, the
embedder's `dimensions()` matches by construction, and a
mismatched-batch insert is rejected by Postgres rather than
silently truncated.

The promotion route reaches the embedder via
`state.memory.as_ref()?.embedder()` rather than a dedicated
`AppState.embedder` slot — the memory facade owns embedder
lifetime, and verified queries are *another consumer* of it,
not a parallel provider.

## Outstanding (next phases)

- **Φ11.4b — `SaveAsVqr` operator UX + review queue.** Chat-side
  button + master-detail workbench surface; backend complete.
- **Φ10.4 — CommunityDetectionCron.** External-dep decision
  pending.

## Φ11.4 — Operator API routes (landed 2026-05-08n)

`crates/ox-api/src/routes/verified_queries.rs` ships five
endpoints for the operator surface:

| Method   | Path                                      | Purpose                |
|----------|-------------------------------------------|------------------------|
| POST     | `/api/verified-queries`                   | Promote                |
| GET      | `/api/verified-queries`                   | List w/ status filter  |
| GET      | `/api/verified-queries/{id}`              | Detail                 |
| POST     | `/api/verified-queries/{id}/transition-status` | Lifecycle transition |
| DELETE   | `/api/verified-queries/{id}`              | Hard delete            |

All admin-gated by `principal.require_designer()`. Promotion
defaults `status = UnderReview` so chat-side `SaveAsVqr` always
lands in the review queue; designers that want immediate
retrievability pass `status = "verified"` explicitly. Server
generates the row id as `vq-{question_hash}` when caller omits
it, matching the canonicalised question.

`PromoteVerifiedQueryRequest` validates server-side: empty
question rejected, `query_ir` must be a JSON object (the
DB-level CHECK constraint redundantly enforces this — server
returns the typed validation error before the round-trip). The
canonical `question_hash` is computed server-side from the raw
question; client-supplied hashes are ignored.

3 DTOs registered to the OpenAPI schema:
`PromoteVerifiedQueryRequest`,
`TransitionVerifiedQueryStatusRequest`,
`VerifiedQueryListResponse`. The pinned-schema test pins all 3
plus the underlying types (`VerifiedQueryDef`, `VerifiedQueryId`,
`ComplexityClass`, `VerifiedQueryStatus`).

The FE workflow (chat-side `SaveAsVqr` button + admin review
queue + bulk transitions) is Φ11.4b; the backend surface is
complete here.

## Φ11.3 — Verified-query freshness cron (landed 2026-05-08m)

`spawn_verified_query_freshness_sweep` runs hourly (singleton-
locked via `ADVISORY_LOCK_CRON_VERIFIED_QUERY_FRESHNESS`) and
walks each workspace's verified bank against the active
canonical ontology version:

1. `list_workspace_ids()` (under SYSTEM_BYPASS) → cross-tenant
   workspace iterator.
2. Per workspace, inside `WORKSPACE_ID.scope(ws_id, ...)`:
   `get_workspace_ontology` + `find_current_version` +
   `get_ontology_ir` rehydrates the IR; greenfield workspaces
   skip silently (`WorkspaceSweepReport::NoCanonical`).
3. `list_verified_queries(Some(Verified), 1000)` — only
   currently-verified rows are scanned (other states are
   already non-retrievable).
4. For each row: `serde_json::from_value::<QueryIR>` then
   `unknown_labels_in_query(ontology_ir, &query_ir)`.
5. Non-empty unknown set →
   `transition_verified_query_status(id, Stale)`.

The cron does **not** auto-stale on deserialise failure — that
class of error indicates corruption, not schema drift, and
silently burying it would hide the root cause. Per-row
diagnostics surface via `tracing::warn!` (operator surface
flags rows for re-promotion).

Interval defaults to 1 hour: schema-drift detection latency is
bounded; per-tick cost (workspace IR rehydrate + bank scan)
stays low because every operation is structural / SQL — no LLM
calls. Verified queries are an ICL input, not a load-bearing
query path, so "occasionally" using a stale exemplar is
graceful (the LLM still produces a working IR), "permanently"
isn't.
- **Φ11.3 — Verified-query freshness cron.** Walks committed
  ontology versions; verified queries whose IR references
  unknown labels / properties flip to `Stale`.
  Singleton-locked, per-workspace fan.
- **Φ11.4 — `SaveAsVqr` operator UX + review queue.** Chat-
  side button promoting a successful execution to
  `UnderReview`; admin queue moves rows
  `UnderReview → Verified`.
- **Φ11.5 — Embedding column + nearest-neighbour retrieval.**
  Wire the workspace's chosen embedding model + a separate
  table for vectors (or pgvector inline) so the Brain's
  exemplar fetch uses semantic similarity rather than just
  exact hash + trigram.

## References

- `crates/ox-ontology/src/verified_query.rs` — type substrate
- `crates/ox-store/src/store/verified_query.rs` — trait
- `crates/ox-store/src/postgres/verified_query.rs` — impl
- `crates/ox-store/migrations/0001_schema.sql` —
  `verified_queries` section
- `crates/ox-store/src/store/knowledge.rs` — adjacent
  failure-driven correction surface (peer)
- `crates/ox-store/src/evaluation.rs` —
  `EvaluationDataset` (eval-side golden bank)
- ontive comparative deep-dive (2026-05-07) — the audit that
  identified this as a P0 NL2SQL gap
