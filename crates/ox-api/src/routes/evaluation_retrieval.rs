//! Retrieval comparison case-execute helper.
//!
//! Drives the [`EvaluationCaseInput::RetrievalComparison`] flow:
//! the same question runs through both the hybrid retrieval path
//! and the trigram-only baseline against the chosen surface, and
//! the two ranked lists land on the case's `actual` payload with
//! per-leg precision@k / recall@k / MRR / NDCG@k metrics already
//! computed.
//!
//! The case-execute layer dispatches here through `match`-based
//! kind routing — there's no separate HTTP endpoint. Same audit
//! trail (`evaluation_runs` + `evaluation_cases` + per-axis
//! `evaluation_metrics`) applies; the FE dashboard's lift chart
//! is derived from the actual payload + metrics rows.
//!
//! ## Why hybrid + baseline in one case
//!
//! The lift signal is *per-question*, not aggregate. Running each
//! leg as a separate case would pair the wrong rows on
//! cross-tabulation; the hybrid leg of question A would land
//! against the trigram leg of question A only by case-key
//! equality, which forces brittle naming conventions. Co-locating
//! the legs lets the dashboard answer "for THIS question, did
//! hybrid help?" deterministically.

use std::sync::Arc;

use ox_brain::Brain;
use ox_memory::EmbeddingProvider;
use ox_ontology::OntologyIR;
use ox_store::Store;
use ox_store::evaluation::{
    EvaluationActual, EvaluationRetrievedAnchor, RetrievalLeg, RetrievalSurface,
};
use ox_store::evaluation::score_retrieval_metrics;
use ox_text::WorkspaceTokenizerRegistry;
use uuid::Uuid;

/// Inputs the case-execute layer passes through to drive the
/// comparison. Carrying these in a struct keeps the call-site
/// signature narrow + readable when more rankers / surfaces
/// land on the contract.
pub struct ComparisonContext<'a> {
    pub store: &'a dyn Store,
    pub ir: &'a OntologyIR,
    pub version_id: Uuid,
    pub workspace_id: Uuid,
    pub tokenizer_registry: &'a Arc<WorkspaceTokenizerRegistry>,
    pub embedder: Option<&'a Arc<dyn EmbeddingProvider>>,
    pub question: &'a str,
    pub surface: RetrievalSurface,
    pub top_k: u32,
    pub expected_ids: &'a [String],
}

/// Run both retrieval legs (hybrid + trigram baseline) against
/// the chosen surface, score each against the gold-standard ids,
/// and return the typed actual envelope. The case-execute layer
/// then UPSERTs `actual` + `latency_ms` + `metadata` on the
/// evaluation_cases row in stage 3; the per-axis
/// evaluation_metrics rows land afterwards via the standard
/// case-judge / capture path so dashboard pivots work.
pub async fn execute_retrieval_comparison(
    ctx: ComparisonContext<'_>,
) -> Result<(EvaluationActual, Option<ox_brain::CallProvenance>), String> {
    let top_k = ctx.top_k.clamp(1, 100);
    let (hybrid, trigram) = match ctx.surface {
        RetrievalSurface::VerifiedQuery => verified_query_legs(&ctx, top_k).await?,
        RetrievalSurface::CommunitySummary => community_summary_legs(&ctx, top_k).await?,
        RetrievalSurface::KnowledgeEntry => knowledge_entry_legs(&ctx, top_k).await?,
    };

    let hybrid_metrics = score_retrieval_metrics(
        &hybrid.iter().map(|h| h.logical_id.clone()).collect::<Vec<_>>(),
        ctx.expected_ids,
        top_k as usize,
    );
    let trigram_metrics = score_retrieval_metrics(
        &trigram.iter().map(|h| h.logical_id.clone()).collect::<Vec<_>>(),
        ctx.expected_ids,
        top_k as usize,
    );

    let payload = EvaluationActual::RetrievalComparison {
        surface: ctx.surface,
        hybrid: RetrievalLeg {
            anchor_ids: hybrid.iter().map(|h| h.logical_id.clone()).collect(),
            hits: hybrid,
            metrics: hybrid_metrics,
        },
        trigram: RetrievalLeg {
            anchor_ids: trigram.iter().map(|h| h.logical_id.clone()).collect(),
            hits: trigram,
            metrics: trigram_metrics,
        },
    };
    Ok((payload, None))
}

/// Run both legs against the verified-query bank.
///
/// Hybrid leg = `hybrid_search_verified_queries_for_icl` (RRF
/// over trigram + tokenized FTS + optional pgvector). Trigram
/// baseline = `search_verified_queries_for_icl` (raw `question
/// %` similarity). Both return `VerifiedQueryDef`; `id` becomes
/// the anchor `logical_id`, `question` the doc text. Score is
/// monotonic per-leg (rank index inverted to similarity for
/// dashboard inspection); precision/recall/MRR/NDCG come from
/// the IR scorer downstream.
async fn verified_query_legs(
    ctx: &ComparisonContext<'_>,
    top_k: u32,
) -> Result<(Vec<EvaluationRetrievedAnchor>, Vec<EvaluationRetrievedAnchor>), String> {
    let tokenizer = ctx.tokenizer_registry.for_workspace(ctx.workspace_id);
    let tokenized = tokenizer
        .tokenize(ctx.question)
        .unwrap_or_else(|_| ctx.question.to_string());
    let embedding = embed_question_or_none(
        ctx.embedder,
        ctx.question,
        "Represent the analytical question for retrieval",
    )
    .await;
    let hybrid_rows = ctx
        .store
        .hybrid_search_verified_queries_for_icl(
            ctx.question,
            &tokenized,
            embedding.as_deref(),
            top_k,
        )
        .await
        .map_err(|e| e.to_string())?;
    let trigram_rows = ctx
        .store
        .search_verified_queries_for_icl(ctx.question, top_k)
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        hybrid_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_verified(i, top_k, r))
            .collect(),
        trigram_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_verified(i, top_k, r))
            .collect(),
    ))
}

async fn community_summary_legs(
    ctx: &ComparisonContext<'_>,
    top_k: u32,
) -> Result<(Vec<EvaluationRetrievedAnchor>, Vec<EvaluationRetrievedAnchor>), String> {
    let tokenizer = ctx.tokenizer_registry.for_workspace(ctx.workspace_id);
    let tokenized = tokenizer
        .tokenize(ctx.question)
        .unwrap_or_else(|_| ctx.question.to_string());
    let embedding = embed_question_or_none(
        ctx.embedder,
        ctx.question,
        "Represent the analytical question for community retrieval",
    )
    .await;
    let hybrid_rows = ctx
        .store
        .search_community_summaries(
            ctx.version_id,
            ctx.question,
            &tokenized,
            embedding.as_deref(),
            top_k,
        )
        .await
        .map_err(|e| e.to_string())?;
    let trigram_rows = ctx
        .store
        .search_community_summaries_trigram_only(ctx.version_id, ctx.question, top_k)
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        hybrid_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_community(i, top_k, r))
            .collect(),
        trigram_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_community(i, top_k, r))
            .collect(),
    ))
}

async fn knowledge_entry_legs(
    ctx: &ComparisonContext<'_>,
    top_k: u32,
) -> Result<(Vec<EvaluationRetrievedAnchor>, Vec<EvaluationRetrievedAnchor>), String> {
    let tokenizer = ctx.tokenizer_registry.for_workspace(ctx.workspace_id);
    let tokenized = tokenizer
        .tokenize(ctx.question)
        .unwrap_or_else(|_| ctx.question.to_string());
    let embedding = embed_question_or_none(
        ctx.embedder,
        ctx.question,
        "Represent the analytical question for knowledge retrieval",
    )
    .await;
    let ontology_name = &ctx.ir.name;
    let ontology_version = ctx.ir.version.number as i32;
    let hybrid_rows = ctx
        .store
        .hybrid_search_knowledge_entries(
            ctx.question,
            &tokenized,
            embedding.as_deref(),
            ontology_name,
            ontology_version,
            &[],
            top_k as i64,
        )
        .await
        .map_err(|e| e.to_string())?;
    let trigram_rows = ctx
        .store
        .search_knowledge_entries_trigram_only(
            ctx.question,
            ontology_name,
            ontology_version,
            top_k as i64,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        hybrid_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_knowledge(i, top_k, r))
            .collect(),
        trigram_rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| anchor_from_knowledge(i, top_k, r))
            .collect(),
    ))
}

/// Embed the question through the workspace's shared embedder
/// or return `None` when no embedder is wired in. Embed
/// failures degrade silently — the hybrid path still runs the
/// trigram + FTS rankers, and the comparison stays meaningful
/// even when the vector arm is unavailable.
async fn embed_question_or_none(
    embedder: Option<&Arc<dyn EmbeddingProvider>>,
    question: &str,
    instruction: &str,
) -> Option<Vec<f32>> {
    let provider = embedder?;
    provider
        .embed(question, instruction, ox_memory::EmbeddingRole::Query)
        .await
        .ok()
}

/// Convert a per-rank position (0-indexed) + bank size to a
/// monotonic [0,1] score the dashboard renders alongside the
/// IR metrics. Rank-derived rather than raw similarity because
/// the underlying SQL surfaces don't return a unified score
/// shape; rank is the contract every leg agrees on.
fn rank_to_score(rank_index: usize, top_k: u32) -> f64 {
    let denom = top_k.max(1) as f64;
    (denom - rank_index as f64).max(0.0) / denom
}

fn anchor_from_verified(
    rank_index: usize,
    top_k: u32,
    row: ox_ontology::VerifiedQueryDef,
) -> EvaluationRetrievedAnchor {
    EvaluationRetrievedAnchor {
        entity_kind: "VerifiedQuery".into(),
        logical_id: row.id.as_str().to_string(),
        doc: row.question,
        score: rank_to_score(rank_index, top_k),
    }
}

fn anchor_from_community(
    rank_index: usize,
    top_k: u32,
    row: ox_store::community::CommunitySummary,
) -> EvaluationRetrievedAnchor {
    EvaluationRetrievedAnchor {
        entity_kind: "CommunitySummary".into(),
        logical_id: row.community_id.clone(),
        doc: format!("{} — {}", row.title, row.summary),
        score: rank_to_score(rank_index, top_k),
    }
}

fn anchor_from_knowledge(
    rank_index: usize,
    top_k: u32,
    row: ox_store::KnowledgeEntry,
) -> EvaluationRetrievedAnchor {
    EvaluationRetrievedAnchor {
        entity_kind: "KnowledgeEntry".into(),
        logical_id: row.id.to_string(),
        doc: format!("{}\n{}", row.title, row.content),
        score: rank_to_score(rank_index, top_k),
    }
}

#[allow(dead_code)]
fn _brain_signature_pin(_: &dyn Brain) {}
