//! Knowledge RAG — label-based lookup of learned corrections.
//!
//! Complements schema_rag (which discovers "what nodes/edges?") by providing
//! "what corrections/hints exist?" for the discovered labels.
//!
//! Strategy: Label-Based Only (no vector search, no BM25).
//! - Knowledge entries have `affected_labels` indexed via GIN.
//! - schema_rag already maps questions → labels.
//! - GIN `&&` lookup is O(1), no embedding cost, < 5ms.

use ox_store::KnowledgeStore;
use tracing::warn;

/// Discover knowledge corrections relevant to the given labels.
///
/// Returns a formatted string for injection into the translate_query prompt.
/// Empty string if no knowledge is found (renders as blank line in template).
pub async fn discover_knowledge(
    store: &dyn KnowledgeStore,
    discovered_labels: &[&str],
    ontology_name: &str,
    ontology_version: i32,
    top_k: usize,
) -> String {
    if discovered_labels.is_empty() {
        return String::new();
    }

    let entries = match store
        .search_knowledge_by_labels(
            ontology_name,
            ontology_version,
            discovered_labels,
            top_k as i64,
        )
        .await
    {
        Ok(entries) => entries,
        Err(e) => {
            warn!(error = %e, "Knowledge RAG lookup failed (non-critical)");
            return String::new();
        }
    };

    if entries.is_empty() {
        return String::new();
    }

    // Fire-and-forget: record usage for retrieved entries.
    // Telemetry — surface the failure so DB outages aren't silent.
    let ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.id).collect();
    if let Err(error) = store.record_knowledge_usage(&ids).await {
        tracing::warn!(?error, hits = ids.len(), "knowledge usage record failed");
    }

    // Only the kind tag and content cross into the prompt. Confidence
    // scores and ontology-version checks are retrieval-side bookkeeping —
    // surfacing them adds tokens without changing the model's decision.
    // Stale entries are deprioritized at retrieval, not by leaking version
    // metadata into the LLM context.
    let mut output = String::from("\n--- Learned corrections for this ontology ---\n");
    for entry in &entries {
        output.push_str(&format!("- [{}] {}\n", entry.kind, entry.content));
    }
    let _ = ontology_version; // currently used only for retrieval scoring

    output
}
