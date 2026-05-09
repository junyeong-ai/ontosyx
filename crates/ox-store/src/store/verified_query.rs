//! Verified query bank persistence — operator-validated `(NL,
//! QueryIR)` pairs the Brain retrieves as ICL exemplars at
//! translate time.
//!
//! Workspace-scoped. UPSERT on the `(workspace_id,
//! question_hash)` natural key — re-promoting the same question
//! collapses to one row. Cross-ontology-version durable; the
//! freshness cron is a separate concern (Φ11.3).

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::{ComplexityClass, VerifiedQueryDef, VerifiedQueryId, VerifiedQueryStatus};

#[async_trait]
pub trait VerifiedQueryStore: Send + Sync {
    /// Insert-or-update on `(workspace_id, question_hash)`.
    /// Returns the persisted row so the caller picks up the
    /// server-stamped `updated_at` without re-fetching.
    async fn upsert_verified_query(
        &self,
        query: &VerifiedQueryDef,
    ) -> OxResult<VerifiedQueryDef>;

    /// Lookup by id. RLS-scoped — cross-tenant ids resolve to
    /// `None`.
    async fn get_verified_query(
        &self,
        id: &VerifiedQueryId,
    ) -> OxResult<Option<VerifiedQueryDef>>;

    /// Lookup by canonical question hash. The fast path the Brain's
    /// translate-query funnel uses to short-circuit on an exact
    /// prior verification — same question canonically equal to a
    /// verified row → return the IR without an LLM call.
    async fn find_verified_query_by_hash(
        &self,
        question_hash: &str,
    ) -> OxResult<Option<VerifiedQueryDef>>;

    /// List rows in the active workspace, newest-updated first.
    /// `status_filter` narrows to a single state when supplied
    /// (typically `Some(Verified)` for retrieval-eligible only,
    /// `None` for the admin surface that shows every state).
    /// `limit` caps the result; the caller's pagination cursor
    /// is a follow-up phase.
    async fn list_verified_queries(
        &self,
        status_filter: Option<VerifiedQueryStatus>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>>;

    /// Update only the lifecycle state. Used by:
    /// - admin surface (Verified ↔ Deprecated)
    /// - operator review queue (UnderReview → Verified)
    /// - freshness cron (Verified → Stale)
    /// Domain verb because the audit trail attached to status
    /// changes (who flipped it, when) lives on the row's
    /// `updated_at` + the activity log emitted alongside.
    async fn transition_verified_query_status(
        &self,
        id: &VerifiedQueryId,
        new_status: VerifiedQueryStatus,
    ) -> OxResult<VerifiedQueryDef>;

    /// Hard delete. The admin path that purges verified queries
    /// without preserving them — for accidentally-promoted rows
    /// that should never have entered the bank. Soft-delete via
    /// `transition_verified_query_status(.., Deprecated)` is the
    /// preferred path for retiring a row that was ever valid.
    async fn delete_verified_query(&self, id: &VerifiedQueryId) -> OxResult<bool>;

    /// Trigram text search on the `question` column.
    /// Approximate-match on natural-language questions for the
    /// admin surface's filter box. The Brain's
    /// embedding-retrieval path (Φ11.2) lives separately —
    /// that's vector / semantic similarity; this is operator
    /// browse / lookup.
    ///
    /// `complexity_filter` narrows to the Brain's ICL-eligible
    /// classes when supplied; `None` returns every class.
    async fn search_verified_queries_by_text(
        &self,
        query_text: &str,
        complexity_filter: Option<ComplexityClass>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>>;

    /// Trigram-ranked retrieval shaped for the Brain's ICL
    /// exemplar injection (Φ11.2b). Returns rows that satisfy
    /// **both** ICL eligibility gates inline at the SQL layer:
    ///
    /// - `status = 'verified'` — only the canonical retrievable
    ///   bank (UnderReview / Deprecated / Stale all excluded).
    /// - `complexity_class != 'trivial'` — Trivial rows carry
    ///   too little structural signal to anchor an LLM exemplar
    ///   ([`ComplexityClass::is_icl_eligible`] in code; this SQL
    ///   filter mirrors it).
    ///
    /// Ordering is `similarity(question, $1) DESC, updated_at
    /// DESC` so the closest match wins, with recency as the
    /// tiebreak. `limit` is the per-call top-k; clamped at
    /// 50 server-side because larger ICL blocks burn prompt
    /// budget without proportional accuracy lift.
    ///
    /// Distinct from
    /// [`Self::search_verified_queries_by_text`] because the ICL
    /// path needs the gates baked in — letting the caller
    /// forget either one regresses retrieval quality silently.
    async fn search_verified_queries_for_icl(
        &self,
        question: &str,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>>;

    /// Φ11.5 — semantic nearest-neighbour retrieval shaped for
    /// the Brain's ICL exemplar injection. Same eligibility
    /// gates as
    /// [`Self::search_verified_queries_for_icl`] (status =
    /// Verified, complexity != Trivial) **plus** a fresh
    /// `embedding IS NOT NULL` filter — rows that haven't been
    /// embedded yet (cold-start, schema-drift) silently drop out
    /// of the semantic ranker and fall back to the trigram
    /// retriever upstream.
    ///
    /// Ordering is `embedding <=> $1::vector` (cosine distance,
    /// closest first). `query_embedding.len()` must match the
    /// column dimension declared in the schema (`vector(1024)`);
    /// a mismatch is rejected by Postgres.
    async fn search_verified_queries_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>>;

    /// Hybrid 3-ranker retrieval — Reciprocal Rank Fusion of:
    ///
    /// 1. **Trigram** — `pg_trgm.similarity(question, $1)` over
    ///    the raw question column. Catches typos / cosmetic
    ///    variation that morphology + embedding miss.
    /// 2. **Lexical (FTS)** — `searchable_tsv @@
    ///    plainto_tsquery('simple', $tokenized_question)` ranked
    ///    by `ts_rank_cd`. Workspace tokenizer canonicalises both
    ///    write-time `tokenized_text` and the runtime
    ///    `tokenized_question` so morphological variants collapse
    ///    onto the same lemmas (Korean compounds + glossary
    ///    canonical concept lemmas).
    /// 3. **Vector (semantic)** — `embedding <=> $vec` cosine
    ///    distance. Optional — `None` skips the vector ranker
    ///    so cold-start workspaces (no embedder) degrade to a
    ///    2-ranker hybrid.
    ///
    /// Fusion: each row's score = `Σ 1/(k + rank_i)` over rankers
    /// where `k = 60` (the published RRF canonical constant from
    /// Cormack/Clarke/Buettcher 2009; balances early-rank weight
    /// without overfitting to a single ranker).
    ///
    /// Eligibility gates baked in: `status = 'verified'` AND
    /// `complexity_class != 'trivial'` (mirrors
    /// [`Self::search_verified_queries_for_icl`]).
    ///
    /// Each ranker pulls `limit * 3` candidates (the standard
    /// RRF candidate breadth — wide enough that the fusion
    /// surfaces rows ranked low by one ranker but high by
    /// another; narrow enough that the SQL stays bounded). The
    /// final SELECT returns `limit` rows.
    async fn hybrid_search_verified_queries_for_icl(
        &self,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>>;
}
