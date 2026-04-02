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
        .search_knowledge_by_labels(ontology_name, ontology_version, discovered_labels, top_k as i64)
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

    // Fire-and-forget: record usage for retrieved entries
    let ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.id).collect();
    let _ = store.record_knowledge_usage(&ids).await;

    // Format entries with version warnings
    let mut output = String::from("\n--- Learned corrections for this ontology ---\n");
    for entry in &entries {
        let prefix = if entry.version_checked < ontology_version {
            format!("[Unverified since v{}] ", entry.version_checked)
        } else {
            String::new()
        };
        output.push_str(&format!(
            "- {}[{}, {:.1}] {}\n",
            prefix, entry.kind, entry.confidence, entry.content,
        ));
    }

    output
}
