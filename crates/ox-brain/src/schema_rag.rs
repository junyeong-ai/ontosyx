//! Schema-Guided RAG for query translation on large ontologies.
//!
//! Instead of injecting the entire OntologyIR JSON into LLM prompts (~120K tokens
//! for 138 nodes), this module discovers the relevant sub-schema via:
//!
//! 1. **Vector search**: Embed the user's question → find semantically related schema nodes
//! 2. **Graph expansion**: Add 1-hop neighbors of discovered nodes (edge connectivity)
//! 3. **Compact schema**: Build minimal JSON with full property descriptions
//!
//! Result: ~5-15 nodes × ~300 bytes = ~2-5KB instead of ~474KB (99% reduction).

use std::collections::HashSet;

use ox_core::ontology_ir::OntologyIR;
use ox_memory::store::{MemoryEntry, MemoryMetadata, MemorySource, MemoryStore};
use ox_memory::vector::MemoryFilter;
use tracing::{info, warn};

/// Maximum schema nodes to include in compact schema for query translation.
const MAX_SCHEMA_NODES: usize = 20;

/// Minimum similarity score for schema node matches.
const MIN_SCHEMA_SCORE: f32 = 0.3;

/// Top-k results from vector search before graph expansion.
const VECTOR_TOP_K: usize = 8;

// ---------------------------------------------------------------------------
// Schema Indexing — runs once when ontology is saved
// ---------------------------------------------------------------------------

/// Index an ontology's schema into the vector store for RAG-based query translation.
/// Each node becomes a natural language embedding with its properties and connections.
///
/// Idempotent: existing entries for the same ontology_id are replaced via upsert.
pub async fn index_ontology_schema(
    memory: &MemoryStore,
    ontology: &OntologyIR,
    ontology_id: &str,
) {
    // Use ontology.id (internal IR ID) for consistency with discover_schema lookups.
    // The caller may pass saved_ontology_id, but discovery falls back to ontology.id
    // when Brain.ontology_id is None (the common case in Analyze mode).
    let effective_id = if ontology.id.is_empty() { ontology_id } else { &ontology.id };
    let entries = ontology.to_schema_entries();
    let total = entries.len();
    let mut indexed = 0;

    for (node_id, description) in entries {
        let entry = MemoryEntry {
            id: format!("schema_{effective_id}_{node_id}"),
            content: description,
            metadata: MemoryMetadata {
                source: MemorySource::Schema,
                ontology_id: Some(effective_id.to_string()),
                session_id: None,
                created_at: chrono::Utc::now(),
            },
        };

        if let Err(e) = memory.store(entry).await {
            warn!(ontology_id, node_id, error = %e, "Failed to index schema node");
            continue;
        }
        indexed += 1;
    }

    info!(ontology_id = effective_id, total, indexed, "Schema indexing complete");
}

// ---------------------------------------------------------------------------
// Schema Discovery — runs per query translation
// ---------------------------------------------------------------------------

/// Discover the relevant sub-schema for a user's question.
///
/// Returns compact schema JSON string optimized for LLM query translation.
/// Falls back to full ontology serialization if memory store unavailable.
pub async fn discover_schema(
    memory: &MemoryStore,
    ontology: &OntologyIR,
    question: &str,
    ontology_id: &str,
) -> String {
    // Step 1: Vector search for semantically related schema nodes
    let filter = MemoryFilter {
        ontology_id: Some(ontology_id.to_string()),
        source: Some("schema".to_string()),
        ..Default::default()
    };

    let hits = match memory
        .search_filtered(question, Some(&MemorySource::Schema), VECTOR_TOP_K, &filter)
        .await
    {
        Ok(hits) => hits,
        Err(e) => {
            warn!(error = %e, "Schema RAG search failed — falling back");
            return fallback_compact_schema(ontology);
        }
    };

    // Filter by minimum score
    let prefix = format!("schema_{ontology_id}_");
    let relevant_ids: Vec<&str> = hits
        .iter()
        .filter(|h| h.score >= MIN_SCHEMA_SCORE)
        .filter_map(|h| h.id.strip_prefix(&prefix))
        .collect();

    if relevant_ids.is_empty() {
        let top_scores: Vec<f32> = hits.iter().take(3).map(|h| h.score).collect();
        info!(
            hit_count = hits.len(),
            ?top_scores,
            min_threshold = MIN_SCHEMA_SCORE,
            "No schema matches above threshold — falling back to compact summary"
        );
        return fallback_compact_schema(ontology);
    }

    // Step 2: Map IDs to node labels
    let mut selected_labels: HashSet<&str> = HashSet::new();
    for node_id in &relevant_ids {
        if let Some(label) = ontology.node_label(node_id) {
            selected_labels.insert(label);
        }
    }

    // Step 3: Graph expansion — add 1-hop neighbors
    let seed_labels: Vec<&str> = selected_labels.iter().copied().collect();
    for label in &seed_labels {
        for neighbor in ontology.neighbor_labels(label) {
            selected_labels.insert(neighbor);
        }
    }

    // Cap at MAX_SCHEMA_NODES (prioritize direct matches)
    let final_labels: Vec<&str> = if selected_labels.len() <= MAX_SCHEMA_NODES {
        selected_labels.into_iter().collect()
    } else {
        let mut result: Vec<&str> = seed_labels.clone();
        for label in &selected_labels {
            if result.len() >= MAX_SCHEMA_NODES {
                break;
            }
            if !seed_labels.contains(label) {
                result.push(label);
            }
        }
        result
    };

    let preview: String = question.chars().take(50).collect();
    info!(
        question_preview = %preview,
        direct_matches = seed_labels.len(),
        with_neighbors = final_labels.len(),
        "Schema discovery complete"
    );

    // Step 4: Build compact schema JSON
    let compact = ontology.compact_schema(&final_labels);
    serde_json::to_string_pretty(&compact).unwrap_or_else(|_| fallback_compact_schema(ontology))
}

/// Compact fallback: all nodes as label+properties summary (no full JSON).
/// For large ontologies (1000+ nodes), uses tiered compression:
/// - First MAX_SCHEMA_NODES nodes get full detail (properties + types)
/// - Remaining nodes get label-only summary with edge connectivity
fn fallback_compact_schema(ontology: &OntologyIR) -> String {
    if ontology.node_types.len() <= MAX_SCHEMA_NODES {
        let all_labels: Vec<&str> = ontology.node_types.iter().map(|n| n.label.as_str()).collect();
        let compact = ontology.compact_schema(&all_labels);
        serde_json::to_string_pretty(&compact).unwrap_or_default()
    } else {
        // Tiered compression for large ontologies:
        // Tier 1: First 20 nodes with full properties (most connected or alphabetical)
        // Tier 2: Remaining nodes as label-only entries
        let mut summary = String::from("Schema (tiered — detailed nodes first, then labels-only):\n\n");
        summary.push_str("## Detailed Nodes\n");
        for node in ontology.node_types.iter().take(MAX_SCHEMA_NODES) {
            let props: Vec<String> = node.properties.iter().map(|p| {
                let ty = serde_json::to_value(&p.property_type)
                    .ok()
                    .and_then(|v| v.get("type").and_then(|t| t.as_str().map(String::from)))
                    .unwrap_or_else(|| "string".to_string());
                let req = if p.nullable { "" } else { "*" };
                format!("{}{}: {}", p.name, req, ty)
            }).collect();
            summary.push_str(&format!("- {} [{}]\n", node.label, props.join(", ")));
        }

        if ontology.node_types.len() > MAX_SCHEMA_NODES {
            summary.push_str(&format!(
                "\n## Additional Nodes ({} labels-only)\n",
                ontology.node_types.len() - MAX_SCHEMA_NODES
            ));
            for node in ontology.node_types.iter().skip(MAX_SCHEMA_NODES) {
                summary.push_str(&format!("- {} ({} props)\n", node.label, node.properties.len()));
            }
        }

        summary.push_str("\n## Edges\n");
        for edge in &ontology.edge_types {
            let src = ontology.node_label(edge.source_node_id.as_ref()).unwrap_or("?");
            let tgt = ontology.node_label(edge.target_node_id.as_ref()).unwrap_or("?");
            summary.push_str(&format!("- ({src})-[:{}]->({tgt})\n", edge.label));
        }
        summary
    }
}
