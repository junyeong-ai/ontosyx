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

use ox_ontology::ir::OntologyIR;
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
/// Idempotent: existing entries for the same ontology_lineage_id are replaced via upsert.
pub async fn index_ontology_schema(
    memory: &MemoryStore,
    ontology: &OntologyIR,
    ontology_lineage_id: &str,
) {
    // Use ontology.id (internal IR ID) for consistency with discover_schema lookups.
    // The caller may pass an externally-scoped id (e.g., the `ontologies.id`
    // identity uuid), but discovery falls back to ontology.id when
    // Brain.ontology_lineage_id is None (the common case in Analyze mode).
    let effective_id = if ontology.id.is_empty() {
        ontology_lineage_id
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
                ontology_lineage_id: Some(effective_id.to_string()),
                session_id: None,
                created_at: chrono::Utc::now(),
            },
        };

        if let Err(e) = memory.store(entry).await {
            warn!(ontology_lineage_id, node_id, error = %e, "Failed to index schema node");
            continue;
        }
        indexed += 1;
    }

    info!(
        ontology_lineage_id = effective_id,
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
    ontology_lineage_id: &str,
) -> (String, Vec<String>) {
    // Step 1: Vector search for semantically related schema nodes
    let filter = MemoryFilter {
        ontology_lineage_id: Some(ontology_lineage_id.to_string()),
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
    let prefix = format!("schema_{ontology_lineage_id}_");
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

    // Tier 1: Graph topology — edges between relevant nodes.
    // The "use exact labels" rule lives in the translate prompts, not
    // here — repeating it on every payload spends prefix-cache budget
    // on a sentence the LLM has already read in the system prompt.
    output.push_str("Graph edges:\n");
    for edge in ontology.edge_types() {
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
    // Tier 3 carries description + Ω-9 terminology enrichment. A property
    // with a bound value_set / notation / range / unit contributes even
    // without a description, so the enrichment line stands in when
    // `description.present()` is empty.
    let mut has_details = false;
    for label in expanded_labels {
        if let Some(node) = ontology.node_by_label(label) {
            let mut described_props: Vec<(&ox_ontology::ir::PropertyDef, &str, String)> = node
                .properties
                .iter()
                .filter_map(|p| {
                    let desc = p.description.present().unwrap_or("");
                    let enrichment = format_property_enrichment(ontology, p);
                    if desc.is_empty() && enrichment.is_empty() {
                        None
                    } else {
                        Some((p, desc, enrichment))
                    }
                })
                .collect();
            // Rank by total informative payload (description + enrichment)
            // so enrichment-rich terminology props aren't starved out by
            // long-described free-text ones.
            described_props.sort_by(|a, b| (b.1.len() + b.2.len()).cmp(&(a.1.len() + a.2.len())));
            let total = described_props.len();
            let pruned = &described_props[..total.min(MAX_DESCRIBED_PROPS_PER_NODE)];

            if !pruned.is_empty() {
                if !has_details {
                    output.push_str("\nProperty details:\n");
                    has_details = true;
                }
                for (prop, desc, enrichment) in pruned {
                    if desc.is_empty() {
                        output.push_str(&format!("  {label}.{}:{enrichment}\n", prop.name));
                    } else {
                        output.push_str(&format!(
                            "  {label}.{}: {desc}{enrichment}\n",
                            prop.name
                        ));
                    }
                }
                if total > MAX_DESCRIBED_PROPS_PER_NODE {
                    // The pruned property names still appear above under
                    // "Node properties:" — no internal "Tier" reference
                    // since the LLM never sees that label.
                    output.push_str(&format!(
                        "  ... and {} more properties\n",
                        total - MAX_DESCRIBED_PROPS_PER_NODE,
                    ));
                }
            }
        }
    }

    // Edge property details (enriched sample values for edge properties)
    for edge in ontology.edge_types() {
        let src = ontology
            .node_label(edge.source_node_id.as_ref())
            .unwrap_or("?");
        let tgt = ontology
            .node_label(edge.target_node_id.as_ref())
            .unwrap_or("?");
        if expanded_set.contains(src) && expanded_set.contains(tgt) {
            let mut described: Vec<(&ox_ontology::ir::PropertyDef, &str, String)> = edge
                .properties
                .iter()
                .filter_map(|p| {
                    let desc = p.description.present().unwrap_or("");
                    let enrichment = format_property_enrichment(ontology, p);
                    if desc.is_empty() && enrichment.is_empty() {
                        None
                    } else {
                        Some((p, desc, enrichment))
                    }
                })
                .collect();
            described.sort_by(|a, b| (b.1.len() + b.2.len()).cmp(&(a.1.len() + a.2.len())));
            let total = described.len();
            let pruned: Vec<_> = described.into_iter().take(MAX_DESCRIBED_PROPS_PER_EDGE).collect();
            if !pruned.is_empty() {
                if !has_details {
                    output.push_str("\nProperty details:\n");
                    has_details = true;
                }
                for (prop, desc, enrichment) in pruned {
                    if desc.is_empty() {
                        output.push_str(&format!("  {}.{}:{enrichment}\n", edge.label, prop.name));
                    } else {
                        output.push_str(&format!(
                            "  {}.{}: {desc}{enrichment}\n",
                            edge.label, prop.name
                        ));
                    }
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

// ---------------------------------------------------------------------------
// Ω-9 — property terminology enrichment
//
// Each `PropertyDef` can reference a value_set / notation_pattern /
// value_range_set / unit. Bare name+type+description is not enough
// for the LLM to author a correct query filter ("status = 'A' " when
// the actual code is "ACTIVE"). We append a 1-line summary per binding
// to the Tier 3 block so the LLM sees the valid code list, the
// notation layout, the numeric band boundaries, and the unit symbol
// inline with the property it describes.
//
// Budgets are deliberately tight — every extra character competes with
// other schema context for the cache-warm prefix budget.
// ---------------------------------------------------------------------------

/// Max concrete codes listed per bound value_set. Ten covers the
/// overwhelming majority of enum-style terminologies; over that, we
/// flatten into a "+N more" tail.
const MAX_VS_CODES_INLINED: usize = 10;

/// Max value-range bands listed per property. Range sets with dozens
/// of fine-grained bands (e.g., clinical lab ranges) are unusual;
/// when they occur, we show the first N and truncate.
const MAX_RS_BANDS_INLINED: usize = 8;

/// Produce the terminology-enrichment suffix for a single property.
/// Empty string iff no bindings — caller appends verbatim so the
/// common "no bindings" path costs zero bytes.
pub(crate) fn format_property_enrichment(
    ontology: &OntologyIR,
    prop: &ox_ontology::ir::PropertyDef,
) -> String {
    let mut out = String::new();

    if let Some(vs_id) = prop.value_set_id()
        && let Some(vs) = ontology.value_set_by_id(vs_id)
    {
        out.push_str(&format!(" [values: {}]", format_value_set_summary(ontology, vs)));
    }

    if let Some(np_id) = prop.notation_pattern_id()
        && let Some(np) = ontology.notation_pattern_by_id(np_id)
    {
        out.push_str(&format!(" [format: {}]", format_notation_summary(np)));
    }

    if let Some(rs_id) = prop.value_range_set_id()
        && let Some(rs) = ontology.value_range_set_by_id(rs_id)
    {
        out.push_str(&format!(" [bands: {}]", format_range_summary(rs)));
    }

    if let Some(unit_id) = &prop.unit_id
        && let Some((_, cv)) = ontology.coded_value_by_id(unit_id)
    {
        out.push_str(&format!(" [unit: {}]", cv.code));
    }

    out
}

/// Resolve a value set to its concrete codes (via `expand_value_set`) and
/// produce a comma-separated list capped at `MAX_VS_CODES_INLINED`.
/// Falls back to the value-set *name* when expansion fails (e.g.
/// malformed selector) — the LLM still gets a handle it can ask about.
fn format_value_set_summary(
    ontology: &OntologyIR,
    vs: &ox_ontology::value_set::ValueSetDef,
) -> String {
    // `expand_value_set` reports ambiguity / missing refs through a
    // `warnings` side-channel rather than `Result` — an empty codes
    // vector is the signal that the value set couldn't be resolved
    // to any concrete codes.
    let expansion = ox_ontology::value_set::expand_value_set(vs, ontology.code_systems());
    if expansion.codes.is_empty() {
        return vs.name.clone();
    }
    let total = expansion.codes.len();
    let head: Vec<String> = expansion
        .codes
        .iter()
        .take(MAX_VS_CODES_INLINED)
        .map(|cv| cv.code.clone())
        .collect();
    let joined = head.join(", ");
    if total > MAX_VS_CODES_INLINED {
        format!("{joined}, +{} more", total - MAX_VS_CODES_INLINED)
    } else {
        joined
    }
}

/// One-line notation pattern summary: template (if authored) + any
/// authored examples. `AAA_NNN` + first example gives the LLM concrete
/// ground to match user strings against.
fn format_notation_summary(np: &ox_ontology::notation_pattern::NotationPatternDef) -> String {
    let mut parts = Vec::new();
    if !np.template.is_empty() {
        parts.push(np.template.clone());
    }
    if let Some(example) = np.examples.first() {
        parts.push(format!("e.g. {example}"));
    }
    if parts.is_empty() {
        np.name.clone()
    } else {
        parts.join(" ")
    }
}

/// One-line value-range summary: `min..max=label` tuples separated by
/// `|`. Unbounded ends render as `-∞` / `+∞`. Bands without a
/// `default`-locale label fall back to `(band N)`.
fn format_range_summary(rs: &ox_ontology::value_range::ValueRangeSetDef) -> String {
    let bands: Vec<String> = rs
        .bands
        .iter()
        .enumerate()
        .take(MAX_RS_BANDS_INLINED)
        .map(|(i, b)| {
            let lo = b
                .min
                .map(|v| format_range_bound(v, b.inclusive_min, true))
                .unwrap_or_else(|| "-∞".to_string());
            let hi = b
                .max
                .map(|v| format_range_bound(v, b.inclusive_max, false))
                .unwrap_or_else(|| "+∞".to_string());
            let label = b
                .label
                .present()
                .map(str::to_string)
                .unwrap_or_else(|| format!("(band {})", i + 1));
            format!("{lo}..{hi}={label}")
        })
        .collect();

    let total = rs.bands.len();
    let joined = bands.join(" | ");
    if total > MAX_RS_BANDS_INLINED {
        format!("{joined} | +{} more", total - MAX_RS_BANDS_INLINED)
    } else if joined.is_empty() {
        rs.name.clone()
    } else {
        joined
    }
}

/// `[v` or `(v` / `v]` or `v)` — bracket-style inclusivity notation
/// is standard math; the LLM training corpus parses it cleanly.
fn format_range_bound(value: f64, inclusive: bool, is_low: bool) -> String {
    match (is_low, inclusive) {
        (true, true) => format!("[{value}"),
        (true, false) => format!("({value}"),
        (false, true) => format!("{value}]"),
        (false, false) => format!("{value})"),
    }
}

fn all_labels(ontology: &OntologyIR) -> Vec<String> {
    ontology
        .node_types()
        .iter()
        .map(|n| n.label.to_string())
        .chain(ontology.edge_types().iter().map(|e| e.label.to_string()))
        .collect()
}

/// Compact fallback: all nodes as label+properties summary (no full JSON).
/// For large ontologies (1000+ nodes), uses tiered compression:
/// - First MAX_SCHEMA_NODES nodes get full detail (properties + types)
/// - Remaining nodes get label-only summary with edge connectivity
fn fallback_compact_schema(ontology: &OntologyIR) -> String {
    if ontology.node_types().len() <= MAX_SCHEMA_NODES {
        let all_labels: Vec<&str> = ontology
            .node_types()
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
        for node in ontology.node_types().iter().take(MAX_SCHEMA_NODES) {
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

        if ontology.node_types().len() > MAX_SCHEMA_NODES {
            summary.push_str(&format!(
                "\n## Additional Nodes ({} labels-only)\n",
                ontology.node_types().len() - MAX_SCHEMA_NODES
            ));
            for node in ontology.node_types().iter().skip(MAX_SCHEMA_NODES) {
                summary.push_str(&format!(
                    "- {} ({} props)\n",
                    node.label,
                    node.properties.len()
                ));
            }
        }

        summary.push_str("\n## Edges\n");
        for edge in ontology.edge_types() {
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

// ---------------------------------------------------------------------------
// Ω-9 unit tests — property terminology enrichment formatters.
//
// The full `format_property_enrichment` path requires an `OntologyIR`
// lookup index (value_set_by_id / coded_value_by_id), so we build a
// real minimal IR and exercise each binding shape through it rather
// than mocking the accessors. Covers the empty-bindings short-circuit,
// value-set expansion + truncation, notation template + example,
// range band rendering across inclusive / exclusive / unbounded, and
// unit code pickup.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;
    use ox_ontology::code_system::{
        CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId,
    };
    use ox_ontology::ir::{NodeTypeDef, OntologyIR, OntologyVersion, PropertyDef};
    use ox_ontology::notation_pattern::{
        NotationComponent, NotationComponentKind, NotationPatternDef, NotationPatternId,
    };
    use ox_ontology::value_range::{ValueBand, ValueRangeSetDef, ValueRangeSetId};
    use ox_ontology::value_set::{
        IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
    };

    fn label(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("graph label")
    }

    fn coded(id: &str, code: &str) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.to_string(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec![],
            broader_id: None,
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn system_with_codes(id: &str, codes: Vec<CodedValue>) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: id.to_string(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    /// Empty-bindings short-circuit: a plain String property returns an
    /// empty string so callers can concatenate without a trailing marker.
    #[test]
    fn enrichment_empty_for_unbound_property() {
        let ontology = minimal_ontology(vec![], vec![], vec![], vec![]);
        let prop = PropertyDef {
            id: "nt_a.prop_a".into(),
            name: PropertyKey::new("x").unwrap(),
            property_type: PropertyType::String,
            description: LocalizedText::default(),
            nullable: true,
            ..Default::default()
        };
        assert_eq!(format_property_enrichment(&ontology, &prop), "");
    }

    /// Value-set enrichment inlines codes up to the cap, then emits a
    /// `+N more` tail. Order mirrors the CodeSystemDef declaration —
    /// `expand_value_set` is stable when the selector is `All`.
    #[test]
    fn enrichment_value_set_inlines_codes_with_tail() {
        let codes: Vec<CodedValue> = (0..12).map(|i| coded(&format!("cv_{i}"), &format!("C{i}"))).collect();
        let system = system_with_codes("sys", codes);

        let vs = ValueSetDef {
            id: ValueSetId::new("vs_status"),
            name: "Status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("sys"),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        };

        let ontology = minimal_ontology(vec![system], vec![vs], vec![], vec![]);

        let prop = PropertyDef {
            id: "nt_a.status".into(),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::String,
            description: LocalizedText::default(),
            nullable: false,
            bindings: vec![ox_ontology::PropertyBinding::value_set(ValueSetId::new("vs_status"),)],
            ..Default::default()
        };

        let enriched = format_property_enrichment(&ontology, &prop);
        assert!(enriched.contains("[values:"), "enrichment present: {enriched}");
        assert!(enriched.contains("C0"), "first code visible");
        assert!(
            enriched.contains("+2 more"),
            "12 codes → 10 inlined + 2 tail: {enriched}",
        );
    }

    /// Notation pattern enrichment shows template + first example.
    #[test]
    fn enrichment_notation_pattern_shows_template_and_example() {
        let pattern = NotationPatternDef {
            id: NotationPatternId::new("np_spring"),
            name: "SpringCode".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: "SPRING_{NNN}".into(),
            separator: "_".into(),
            components: vec![NotationComponent {
                name: "n".into(),
                display: LocalizedText::default(),
                kind: NotationComponentKind::IntegerRange {
                    min: 0,
                    max: 999,
                    width: 3,
                },
            }],
            examples: vec!["SPRING_001".into(), "SPRING_042".into()],
        };

        let ontology = minimal_ontology(vec![], vec![], vec![pattern], vec![]);

        let prop = PropertyDef {
            id: "nt_a.ticket".into(),
            name: PropertyKey::new("ticket").unwrap(),
            property_type: PropertyType::String,
            description: LocalizedText::default(),
            nullable: false,
            bindings: vec![ox_ontology::PropertyBinding::notation_pattern(NotationPatternId::new("np_spring"),)],
            ..Default::default()
        };

        let enriched = format_property_enrichment(&ontology, &prop);
        assert!(enriched.contains("[format:"), "notation tag: {enriched}");
        assert!(enriched.contains("SPRING_{NNN}"), "template visible");
        assert!(enriched.contains("e.g. SPRING_001"), "first example visible");
    }

    /// Value-range enrichment renders each band as `[lo..hi]=Label`.
    /// Unbounded ends come out as `-∞` / `+∞`. Brackets match inclusivity.
    #[test]
    fn enrichment_value_range_renders_bands_with_inclusivity() {
        let range = ValueRangeSetDef {
            id: ValueRangeSetId::new("rs_bp"),
            name: "BP".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                ValueBand {
                    min: None,
                    max: Some(90.0),
                    inclusive_min: false,
                    inclusive_max: false,
                    label: LocalizedText::new("Low"),
                    severity: None,
                },
                ValueBand {
                    min: Some(90.0),
                    max: Some(120.0),
                    inclusive_min: true,
                    inclusive_max: false,
                    label: LocalizedText::new("Normal"),
                    severity: None,
                },
                ValueBand {
                    min: Some(120.0),
                    max: None,
                    inclusive_min: true,
                    inclusive_max: false,
                    label: LocalizedText::new("High"),
                    severity: None,
                },
            ],
        };

        let ontology = minimal_ontology(vec![], vec![], vec![], vec![range]);

        let prop = PropertyDef {
            id: "nt_a.bp".into(),
            name: PropertyKey::new("bp").unwrap(),
            property_type: PropertyType::Int,
            description: LocalizedText::default(),
            nullable: false,
            bindings: vec![ox_ontology::PropertyBinding::value_range(ValueRangeSetId::new("rs_bp"),)],
            ..Default::default()
        };

        let enriched = format_property_enrichment(&ontology, &prop);
        assert!(enriched.contains("-∞..90)=Low"), "low band: {enriched}");
        assert!(enriched.contains("[90..120)=Normal"), "normal band: {enriched}");
        assert!(enriched.contains("[120..+∞=High"), "high band: {enriched}");
    }

    /// Unit enrichment surfaces the coded-value `code` (e.g. `kg`).
    #[test]
    fn enrichment_unit_shows_coded_value_code() {
        let system = system_with_codes(
            "ucum",
            vec![coded("ucum.kg", "kg"), coded("ucum.m", "m")],
        );
        let ontology = minimal_ontology(vec![system], vec![], vec![], vec![]);

        let prop = PropertyDef {
            id: "nt_a.mass".into(),
            name: PropertyKey::new("mass").unwrap(),
            property_type: PropertyType::Float,
            description: LocalizedText::default(),
            nullable: false,
            unit_id: Some(CodedValueId::new("ucum.kg")),
            ..Default::default()
        };

        let enriched = format_property_enrichment(&ontology, &prop);
        assert_eq!(enriched, " [unit: kg]");
    }

    /// Build a fresh OntologyIR carrying the supplied terminology
    /// registry items. A single node type is attached so the IR passes
    /// its own invariant checks.
    fn minimal_ontology(
        code_systems: Vec<CodeSystemDef>,
        value_sets: Vec<ValueSetDef>,
        notation_patterns: Vec<NotationPatternDef>,
        value_range_sets: Vec<ValueRangeSetDef>,
    ) -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont-test".to_string(),
            "Enrichment Test".to_string(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                valid_from: None,
                valid_to: None,
                committed_by: None,
                commit_message: None,
            },
            vec![NodeTypeDef {
                id: "nt_a".into(),
                label: label("A"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        for cs in code_systems {
            ir.add_code_system(cs).expect("add code system");
        }
        for vs in value_sets {
            ir.add_value_set(vs).expect("add value set");
        }
        for np in notation_patterns {
            ir.add_notation_pattern(np).expect("add notation pattern");
        }
        for rs in value_range_sets {
            ir.add_value_range_set(rs).expect("add value range set");
        }
        ir
    }
}
