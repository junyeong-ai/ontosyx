//! Knowledge RAG — hybrid retrieval over learned corrections.
//!
//! Complements `schema_rag` (which discovers "what nodes/edges?")
//! by surfacing "what corrections / hints exist?" for the
//! discovered question.
//!
//! ## Retrieval strategy
//!
//! Reciprocal Rank Fusion over four / five rankers (whichever
//! the caller can supply):
//!
//! 1. Trigram on `title` — typo / cosmetic recall on the
//!    correction's headline.
//! 2. Trigram on `content` — same on the longer prose.
//! 3. Lexical FTS on `searchable_tsv` — workspace lindera +
//!    glossary user-dict canonicalisation. Recall consistency
//!    by construction: index-time and query-time tokens come
//!    from the same `Arc<dyn Tokenizer>`.
//! 4. Vector NN on `embedding` — paraphrase recall. Optional
//!    (cold-start / no embedder → skipped).
//! 5. Label boost — when the caller threads
//!    `discovered_labels` from `schema_rag`, every entry whose
//!    `affected_labels && labels` is non-empty enters at rank
//!    1 in this synthetic ranker, lifting it past unrelated
//!    matches. Soft boost, not a filter — knowledge entries
//!    without labels (text-driven hints) still surface via the
//!    other rankers.
//!
//! Eligibility is hard-gated: `status = 'approved'` and
//! `ontology_version_min ≤ v ≤ ontology_version_max`.
//! Confidence multiplies into the final fusion score so
//! operator-set trust carries through.

use std::sync::Arc;

use ox_memory::{EmbeddingProvider, EmbeddingRole};
use ox_store::KnowledgeStore;
use ox_text::WorkspaceTokenizerRegistry;
use tracing::warn;
use uuid::Uuid;

/// Inputs the caller threads through. The struct keeps the
/// signature stable as more rankers come online and avoids the
/// boolean-positional argument anti-pattern.
pub struct KnowledgeRetrievalContext<'a> {
    pub question: &'a str,
    pub discovered_labels: &'a [&'a str],
    pub ontology_name: &'a str,
    pub ontology_version: i32,
    pub top_k: usize,
    pub workspace_id: Option<Uuid>,
    pub tokenizer_registry: Option<&'a Arc<WorkspaceTokenizerRegistry>>,
    pub embedder: Option<&'a Arc<dyn EmbeddingProvider>>,
}

/// Discover knowledge corrections relevant to the question +
/// labels.
///
/// Returns a formatted string for injection into the
/// translate_query prompt. Empty string if no knowledge is
/// found (renders as a blank line in the template).
pub async fn discover_knowledge(
    store: &dyn KnowledgeStore,
    ctx: &KnowledgeRetrievalContext<'_>,
) -> String {
    // The hybrid retriever's text rankers (trigram, FTS, vector)
    // all key off `question`; an empty question collapses every
    // arm to zero matches and the label boost alone — when
    // present — would be a degenerate single-ranker fusion.
    // Guard at the entry so we never burn a SQL roundtrip on
    // empty input.
    if ctx.question.trim().is_empty() {
        return String::new();
    }

    let tokenized = match (ctx.tokenizer_registry, ctx.workspace_id) {
        (Some(reg), Some(ws)) => {
            let tok = reg.for_workspace(ws);
            tok.tokenize(ctx.question)
                .unwrap_or_else(|_| ctx.question.to_string())
        }
        _ => ctx.question.to_string(),
    };

    let query_embedding = if let Some(embedder) = ctx.embedder {
        match embedder
            .embed(
                ctx.question,
                "Represent the analytical question for knowledge retrieval",
                EmbeddingRole::Query,
            )
            .await
        {
            Ok(v) => Some(v),
            Err(error) => {
                warn!(
                    ?error,
                    "knowledge question embed failed; degrading to lexical-only hybrid"
                );
                None
            }
        }
    } else {
        None
    };

    let entries = match store
        .hybrid_search_knowledge_entries(
            ctx.question,
            &tokenized,
            query_embedding.as_deref(),
            ctx.ontology_name,
            ctx.ontology_version,
            ctx.discovered_labels,
            ctx.top_k as i64,
        )
        .await
    {
        Ok(entries) => entries,
        Err(error) => {
            warn!(%error, "Knowledge RAG hybrid retrieval failed (non-critical)");
            return String::new();
        }
    };

    if entries.is_empty() {
        return String::new();
    }

    let ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.id).collect();
    if let Err(error) = store.record_knowledge_usage(&ids).await {
        warn!(?error, hits = ids.len(), "knowledge usage record failed");
    }

    let mut output = String::from("\n--- Learned corrections for this ontology ---\n");
    for entry in &entries {
        output.push_str(&format!("- [{}] {}\n", entry.kind, entry.content));
    }
    output
}
