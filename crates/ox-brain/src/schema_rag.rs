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
/// Increase for ontologies with deep multi-hop patterns.
const MAX_SCHEMA_NODES: usize = 40;

/// Minimum similarity score for schema node matches.
const MIN_SCHEMA_SCORE: f32 = 0.25;

/// Top-k results from vector search before graph expansion.
const VECTOR_TOP_K: usize = 12;

/// Ontologies at or below this node count get full progressive schema
/// without vector search. Modern LLMs handle ~12K tokens of schema easily,
/// and RAG on small ontologies risks omitting nodes the query needs.
pub const FULL_SCHEMA_NODE_THRESHOLD: usize = 50;

/// Maximum properties with descriptions per node in Tier 3.
/// Prevents token explosion on nodes with many described properties.
/// Properties are ranked by description length (longer = more informative).
const MAX_DESCRIBED_PROPS_PER_NODE: usize = 15;

/// Maximum properties with descriptions per edge in Tier 3.
/// Prevents token explosion on edges with many described properties.
const MAX_DESCRIBED_PROPS_PER_EDGE: usize = 10;

// ---------------------------------------------------------------------------
// Schema Indexing — runs once when ontology is saved
// ---------------------------------------------------------------------------

/// Index an ontology's schema into the vector store for RAG-based query translation.
/// Each node becomes a natural language embedding with its properties and connections.
///
/// Idempotent: existing entries for the same ontology_id are replaced via upsert.
pub async fn index_ontology_schema(memory: &MemoryStore, ontology: &OntologyIR, ontology_id: &str) {
    // Use ontology.id (internal IR ID) for consistency with discover_schema lookups.
    // The caller may pass saved_ontology_id, but discovery falls back to ontology.id
    // when Brain.ontology_id is None (the common case in Analyze mode).
    let effective_id = if ontology.id.is_empty() {
        ontology_id
    } else {
        &ontology.id
    };
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

    info!(
        ontology_id = effective_id,
        total, indexed, "Schema indexing complete"
    );
}

// ---------------------------------------------------------------------------
// Schema Discovery — runs per query translation
// ---------------------------------------------------------------------------

/// Discover relevant sub-schema via vector search + BFS graph expansion.
///
/// Returns `(progressive_schema_text, discovered_labels)` — a compact text
/// representation optimized for LLM query translation, plus the list of
/// labels included (for downstream Knowledge RAG filtering).
pub async fn discover_schema(
    memory: &MemoryStore,
    ontology: &OntologyIR,
    question: &str,
    ontology_id: &str,
) -> (String, Vec<String>) {
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
            return (fallback_compact_schema(ontology), all_labels(ontology));
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
        return (fallback_compact_schema(ontology), all_labels(ontology));
    }

    // Step 2: Map IDs to node labels
    let mut selected_labels: HashSet<&str> = HashSet::new();
    for node_id in &relevant_ids {
        if let Some(label) = ontology.node_label(node_id) {
            selected_labels.insert(label);
        }
    }

    // Step 3: Graph expansion — BFS from seed nodes until budget exhausted.
    // Unlike fixed 1-hop, this follows the graph structure outward from seeds,
    // ensuring multi-hop query chains (e.g., NodeA→NodeB→NodeC→NodeD)
    // are fully covered up to MAX_SCHEMA_NODES.
    let seed_labels: Vec<&str> = selected_labels.iter().copied().collect();

    let mut frontier: Vec<&str> = seed_labels.clone();
    while selected_labels.len() < MAX_SCHEMA_NODES && !frontier.is_empty() {
        let mut next_frontier = Vec::new();
        for label in &frontier {
            for neighbor in ontology.neighbor_labels(label) {
                if selected_labels.len() >= MAX_SCHEMA_NODES {
                    break;
                }
                if selected_labels.insert(neighbor) {
                    next_frontier.push(neighbor);
                }
            }
        }
        frontier = next_frontier;
    }

    let final_labels: Vec<&str> = selected_labels.into_iter().collect();

    let preview: String = question.chars().take(50).collect();
    info!(
        question_preview = %preview,
        direct_matches = seed_labels.len(),
        with_neighbors = final_labels.len(),
        "Schema discovery complete"
    );

    // Step 4: Build progressive disclosure schema
    // Tier 1: Graph topology (all expanded labels) — edges with source→target
    // Tier 2: Property names + types (all expanded labels) — compact, no descriptions
    // Tier 3: Property descriptions (seed labels only) — full detail for most relevant
    let labels_out: Vec<String> = final_labels.iter().map(|s| s.to_string()).collect();
    let schema = build_progressive_schema(ontology, &final_labels);
    (schema, labels_out)
}

/// Build a progressive disclosure schema with 3 tiers of detail.
///
/// This dramatically reduces token count (~70% reduction) while preserving
/// the most important information for query translation:
/// - Tier 1: Graph structure (edges) — enables multi-hop chain planning
/// - Tier 2: Property names + types — enables WHERE filters and projections
/// - Tier 3: Property descriptions — enables value matching (enums, ranges)
pub(crate) fn build_progressive_schema(ontology: &OntologyIR, expanded_labels: &[&str]) -> String {
    let expanded_set: HashSet<&str> = expanded_labels.iter().copied().collect();

    let mut output = String::with_capacity(2048);

    // Tier 1: Graph topology — edges between relevant nodes
    // Explicit labels help the LLM use EXACT edge names (critical in JSON mode)
    output.push_str("Graph edges (use EXACTLY these edge labels):\n");
    for edge in &ontology.edge_types {
        let src = ontology
            .node_label(edge.source_node_id.as_ref())
            .unwrap_or("?");
        let tgt = ontology
            .node_label(edge.target_node_id.as_ref())
            .unwrap_or("?");
        if expanded_set.contains(src) && expanded_set.contains(tgt) {
            let cardinality = format!("{:?}", edge.cardinality);
            output.push_str(&format!(
                "  ({src})-[:{}]->({tgt}) [{cardinality}]\n",
                edge.label
            ));
            // Include edge properties if they exist (e.g., quantity on CONTAINS)
            for p in &edge.properties {
                output.push_str(&format!(
                    "    edge.{}: {}\n",
                    p.name,
                    format_property_type(&p.property_type)
                ));
            }
        }
    }

    // Tier 2: Property names + types (all expanded labels, no descriptions)
    output.push_str("\nNode properties:\n");
    for label in expanded_labels {
        if let Some(node) = ontology.node_by_label(label) {
            if node.properties.is_empty() {
                continue; // Skip nodes with no properties — no useful info for query
            }
            let props: Vec<String> = node
                .properties
                .iter()
                .map(|p| {
                    let nullable = if p.nullable { "?" } else { "" };
                    format!(
                        "{}{}: {}",
                        p.name,
                        nullable,
                        format_property_type(&p.property_type)
                    )
                })
                .collect();
            output.push_str(&format!("  {}: {{{}}}\n", label, props.join(", ")));
        }
    }

    // Tier 3: Property descriptions + sample values (ALL expanded labels + edge properties)
    // Pruned to MAX_DESCRIBED_PROPS_PER_NODE per node to prevent token explosion.
    // Properties ranked by description length (longer descriptions contain more
    // informative data like sample values, enum lists, and ranges).
    // NOTE: Uses expanded_labels (not just seeds) so that BFS-discovered neighbor
    // nodes also get property descriptions — critical for LLM to distinguish
    // between similarly-named properties (e.g., name vs name_inci).
    let mut has_details = false;
    for label in expanded_labels {
        if let Some(node) = ontology.node_by_label(label) {
            let mut described_props: Vec<(&str, &str)> = node
                .properties
                .iter()
                .filter_map(|p| {
                    p.description
                        .as_ref()
                        .filter(|d| !d.is_empty())
                        .map(|d| (p.name.as_str(), d.as_str()))
                })
                .collect();
            // Rank by description length (descending) — longer = more informative
            described_props.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            let total = described_props.len();
            let pruned = &described_props[..total.min(MAX_DESCRIBED_PROPS_PER_NODE)];

            if !pruned.is_empty() {
                if !has_details {
                    output.push_str("\nProperty details:\n");
                    has_details = true;
                }
                for (prop_name, desc) in pruned {
                    output.push_str(&format!("  {label}.{prop_name}: {desc}\n"));
                }
                if total > MAX_DESCRIBED_PROPS_PER_NODE {
                    output.push_str(&format!(
                        "  ... and {} more properties (see Tier 2 for names)\n",
                        total - MAX_DESCRIBED_PROPS_PER_NODE,
                    ));
                }
            }
        }
    }

    // Edge property details (enriched sample values for edge properties)
    for edge in &ontology.edge_types {
        let src = ontology
            .node_label(edge.source_node_id.as_ref())
            .unwrap_or("?");
        let tgt = ontology
            .node_label(edge.target_node_id.as_ref())
            .unwrap_or("?");
        if expanded_set.contains(src) && expanded_set.contains(tgt) {
            let mut described: Vec<(&str, &str)> = edge
                .properties
                .iter()
                .filter_map(|p| {
                    p.description
                        .as_deref()
                        .filter(|d| !d.is_empty())
                        .map(|d| (p.name.as_str(), d))
                })
                .collect();
            described.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            let total = described.len();
            let pruned: Vec<_> = described
                .into_iter()
                .take(MAX_DESCRIBED_PROPS_PER_EDGE)
                .collect();
            if !pruned.is_empty() {
                if !has_details {
                    output.push_str("\nProperty details:\n");
                    has_details = true;
                }
                for (prop_name, desc) in pruned {
                    output.push_str(&format!("  {}.{prop_name}: {desc}\n", edge.label));
                }
                if total > MAX_DESCRIBED_PROPS_PER_EDGE {
                    output.push_str(&format!(
                        "  ... and {} more edge properties\n",
                        total - MAX_DESCRIBED_PROPS_PER_EDGE,
                    ));
                }
            }
        }
    }

    output
}

fn format_property_type(pt: &ox_core::types::PropertyType) -> String {
    match pt {
        ox_core::types::PropertyType::String => "string".into(),
        ox_core::types::PropertyType::Int => "int".into(),
        ox_core::types::PropertyType::Float => "float".into(),
        ox_core::types::PropertyType::Bool => "bool".into(),
        ox_core::types::PropertyType::Date => "date".into(),
        ox_core::types::PropertyType::DateTime => "datetime".into(),
        ox_core::types::PropertyType::Duration => "duration".into(),
        ox_core::types::PropertyType::Bytes => "bytes".into(),
        ox_core::types::PropertyType::Map => "map".into(),
        ox_core::types::PropertyType::List { element } => {
            format!("list<{}>", format_property_type(element))
        }
    }
}

fn all_labels(ontology: &OntologyIR) -> Vec<String> {
    ontology
        .node_types
        .iter()
        .map(|n| n.label.clone())
        .chain(ontology.edge_types.iter().map(|e| e.label.clone()))
        .collect()
}

/// Compact fallback: all nodes as label+properties summary (no full JSON).
/// For large ontologies (1000+ nodes), uses tiered compression:
/// - First MAX_SCHEMA_NODES nodes get full detail (properties + types)
/// - Remaining nodes get label-only summary with edge connectivity
fn fallback_compact_schema(ontology: &OntologyIR) -> String {
    if ontology.node_types.len() <= MAX_SCHEMA_NODES {
        let all_labels: Vec<&str> = ontology
            .node_types
            .iter()
            .map(|n| n.label.as_str())
            .collect();
        let compact = ontology.compact_schema(&all_labels);
        serde_json::to_string_pretty(&compact).unwrap_or_default()
    } else {
        // Tiered compression for large ontologies:
        // Tier 1: First 20 nodes with full properties (most connected or alphabetical)
        // Tier 2: Remaining nodes as label-only entries
        let mut summary =
            String::from("Schema (tiered — detailed nodes first, then labels-only):\n\n");
        summary.push_str("## Detailed Nodes\n");
        for node in ontology.node_types.iter().take(MAX_SCHEMA_NODES) {
            let props: Vec<String> = node
                .properties
                .iter()
                .map(|p| {
                    let ty = serde_json::to_value(&p.property_type)
                        .ok()
                        .and_then(|v| v.get("type").and_then(|t| t.as_str().map(String::from)))
                        .unwrap_or_else(|| "string".to_string());
                    let req = if p.nullable { "" } else { "*" };
                    format!("{}{}: {}", p.name, req, ty)
                })
                .collect();
            summary.push_str(&format!("- {} [{}]\n", node.label, props.join(", ")));
        }

        if ontology.node_types.len() > MAX_SCHEMA_NODES {
            summary.push_str(&format!(
                "\n## Additional Nodes ({} labels-only)\n",
                ontology.node_types.len() - MAX_SCHEMA_NODES
            ));
            for node in ontology.node_types.iter().skip(MAX_SCHEMA_NODES) {
                summary.push_str(&format!(
                    "- {} ({} props)\n",
                    node.label,
                    node.properties.len()
                ));
            }
        }

        summary.push_str("\n## Edges\n");
        for edge in &ontology.edge_types {
            let src = ontology
                .node_label(edge.source_node_id.as_ref())
                .unwrap_or("?");
            let tgt = ontology
                .node_label(edge.target_node_id.as_ref())
                .unwrap_or("?");
            summary.push_str(&format!("- ({src})-[:{}]->({tgt})\n", edge.label));
        }
        summary
    }
}
