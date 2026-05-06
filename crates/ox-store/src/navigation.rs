//! Navigation options + subgraph types for [`crate::OntologyNavigationStore`].
//!
//! Keeps the trait's method signatures readable (one `options` struct
//! per call) and centralises the Subgraph shape that the patent's
//! Progressive Disclosure 4-step API threads end-to-end:
//!
//! 1. `search_entry_points(opts)` — anchor hits (tsvector + trigram + embedding blend)
//! 2. `expand_neighbors(opts)` — BFS from anchors, depth-limited, batch
//! 3. `apply_hierarchy_and_facet(subgraph, opts)` — closure + facet filter, mutating
//! 4. `render_subgraph_for_llm(subgraph, opts)` — markdown fit for the LLM prompt tail
//!
//! All three mutators operate on the shared [`Subgraph`] so a caller
//! can chain calls without re-allocating rows between layers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One logical identity in a versioned ontology. `kind` matches the
/// `entity_kind` column on the Level-3 flat tables
/// (`NodeType`, `PropertyDef`, `EdgeType`, `GlossaryTerm`,
/// `CodeSystem`, `CodedValue`, `ValueSet`, `NotationPattern`, ...).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: String,
    pub logical_id: String,
}

impl EntityRef {
    pub fn new(kind: impl Into<String>, logical_id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            logical_id: logical_id.into(),
        }
    }
}

/// One hit from the entry-point search. `score` is normalised 0..1;
/// the caller sorts by it to pick the top-K anchors.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntitySearchHit {
    pub entity_kind: String,
    pub logical_id: String,
    /// Concatenated searchable document snippet (label + aliases + description).
    pub doc: String,
    pub score: f32,
}

impl EntitySearchHit {
    pub fn as_entity_ref(&self) -> EntityRef {
        EntityRef::new(&self.entity_kind, &self.logical_id)
    }
}

/// Options for [`crate::OntologyNavigationStore::search_entry_points`].
#[derive(Debug, Clone)]
pub struct EntryPointSearchOptions {
    pub version_id: Uuid,
    pub query: String,
    pub limit: u32,
    /// When `Some`, restricts the hit stream to these entity kinds.
    /// `None` returns any kind (the usual starting point for a
    /// natural-language search).
    pub kinds: Option<Vec<String>>,
    pub blend: BlendWeights,
}

impl EntryPointSearchOptions {
    pub fn new(version_id: Uuid, query: impl Into<String>, limit: u32) -> Self {
        Self {
            version_id,
            query: query.into(),
            limit,
            kinds: None,
            blend: BlendWeights::default(),
        }
    }

    pub fn with_kinds(mut self, kinds: Vec<String>) -> Self {
        self.kinds = Some(kinds);
        self
    }

    pub fn with_blend(mut self, blend: BlendWeights) -> Self {
        self.blend = blend;
        self
    }
}

/// Per-index weight for the blended anchor score. Weights don't need
/// to sum to 1 — the blend takes the max of weighted sub-scores so an
/// exact trigram hit can dominate even when the full-text rank is low.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlendWeights {
    pub trigram: f32,
    pub full_text: f32,
    pub embedding: f32,
}

impl Default for BlendWeights {
    fn default() -> Self {
        // Empirical baseline: trigram dominates short prefix /
        // alias / typo queries; full-text helps phrase search;
        // embedding catches semantic rewrites. Tune per workspace
        // once real usage lands.
        Self {
            trigram: 0.5,
            full_text: 0.3,
            embedding: 0.2,
        }
    }
}

/// Options for [`crate::OntologyNavigationStore::expand_neighbors`].
/// Batch-capable on the anchor axis — a single call expands N anchors
/// concurrently into one subgraph, which is the shape
/// Progressive Disclosure step 2 consumes.
#[derive(Debug, Clone)]
pub struct NeighborExpandOptions {
    pub version_id: Uuid,
    pub anchors: Vec<EntityRef>,
    /// BFS depth. `1` = only direct neighbors; `2` = 2-hop; capped
    /// at 5 for cost.
    pub depth: u8,
    pub direction: NeighborDirection,
    /// Kinds to include. `None` returns every neighbor kind.
    pub include_kinds: Option<Vec<String>>,
    /// Hard cap on total nodes. The BFS stops once the frontier
    /// would exceed this; `Subgraph.truncated` signals to the
    /// caller. 0 means unlimited (dangerous on wide graphs — the
    /// default is the sane value).
    pub max_nodes: u32,
}

impl NeighborExpandOptions {
    pub fn new(version_id: Uuid, anchors: Vec<EntityRef>) -> Self {
        Self {
            version_id,
            anchors,
            depth: 2,
            direction: NeighborDirection::Both,
            include_kinds: None,
            max_nodes: 256,
        }
    }

    pub fn with_depth(mut self, depth: u8) -> Self {
        self.depth = depth.clamp(0, 5);
        self
    }

    pub fn with_direction(mut self, direction: NeighborDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn with_include_kinds(mut self, kinds: Vec<String>) -> Self {
        self.include_kinds = Some(kinds);
        self
    }

    pub fn with_max_nodes(mut self, max_nodes: u32) -> Self {
        self.max_nodes = max_nodes;
        self
    }
}

/// Direction selector for neighbor expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborDirection {
    /// Edges FROM the anchor TO others.
    Outgoing,
    /// Edges FROM others TO the anchor.
    Incoming,
    /// Both.
    Both,
}

/// Options for [`crate::OntologyNavigationStore::apply_hierarchy_and_facet`].
/// Step 3 of Progressive Disclosure — enriches an existing subgraph
/// with hierarchy closure (code_system_broader, glossary_term_parent,
/// interface_implements) and optionally filters by facet.
#[derive(Debug, Clone)]
pub struct HierarchyFacetOptions {
    pub version_id: Uuid,
    pub hierarchy_expand: Option<HierarchyExpand>,
    pub facet_filter: Option<FacetFilter>,
    /// Cap on codes emitted per CodeSystem after hierarchy expansion.
    /// Default keeps the subgraph LLM-fit without truncating value
    /// semantics beyond what Progressive Disclosure needs.
    pub max_codes_per_code_system: u32,
}

impl Default for HierarchyFacetOptions {
    fn default() -> Self {
        Self {
            version_id: Uuid::nil(),
            hierarchy_expand: None,
            facet_filter: None,
            max_codes_per_code_system: 20,
        }
    }
}

/// Which direction to walk the hierarchy closure table.
#[derive(Debug, Clone)]
pub enum HierarchyExpand {
    /// Expand descendants of `anchor` in `relation_kind` up to
    /// `max_depth` (inclusive). `max_depth: u32::MAX` = everything.
    Descendants {
        relation_kind: String,
        anchor: EntityRef,
        max_depth: u32,
    },
    /// Expand ancestors (inclusive) of `anchor`. No depth limit —
    /// the ancestor chain is always short by design.
    Ancestors {
        relation_kind: String,
        anchor: EntityRef,
    },
}

/// Structural filter applied after hierarchy expansion.
#[derive(Debug, Clone, Default)]
pub struct FacetFilter {
    /// Keep only nodes whose kind is in this set.
    pub kinds: Option<Vec<String>>,
}

/// Options for [`crate::OntologyNavigationStore::render_subgraph_for_llm`].
#[derive(Debug, Clone)]
pub struct LlmRenderOptions {
    /// Cap on nodes emitted into the markdown. Over-cap subgraphs
    /// render a trailing "... and N more" line so the LLM knows the
    /// context is truncated.
    pub max_nodes: usize,
    /// Optional hard cap on the rendered output's estimated token
    /// count. `None` = no token cap (only `max_nodes` applies).
    /// When set, the renderer stops emitting nodes once the
    /// running estimate would exceed the budget — caller-driven
    /// when the prompt has a tight context-window slice
    /// (`MAX_GRAPHRAG_BUDGET = ctx_window - prompt_overhead -
    /// answer_reservation`). Tokens are estimated via
    /// [`estimate_tokens_chars`] (chars / 3, conservative); the
    /// platform doesn't carry a tokenizer dep so this stays
    /// model-agnostic and safe to over-trim by 5-10%.
    pub max_tokens: Option<u32>,
    /// Include the `doc` text per node. Adds tokens but lets the LLM
    /// disambiguate similarly-named concepts.
    pub include_doc_snippets: bool,
}

impl Default for LlmRenderOptions {
    fn default() -> Self {
        Self {
            max_nodes: 80,
            max_tokens: None,
            include_doc_snippets: true,
        }
    }
}

/// Conservative token-count estimate. Uses 3 chars per token —
/// the lower-bound estimate that holds across English (4 cpt
/// average), Korean / Japanese (1.5-2 cpt), and mixed scripts.
/// Picking the conservative end of the range means the renderer
/// over-trims by 5-10% rather than overflowing the context
/// window. The platform deliberately doesn't pull in `tiktoken`
/// or a per-model tokenizer here — embedding-budget decisions
/// shouldn't fluctuate on a model swap.
pub fn estimate_tokens_chars(text: &str) -> u32 {
    // `chars().count()` over `len()` so multi-byte UTF-8 doesn't
    // inflate the estimate (e.g. Korean characters are 3 bytes
    // each in UTF-8 but should count as ~0.7 tokens, not ~2).
    let chars = text.chars().count();
    ((chars + 2) / 3) as u32
}

// ---------------------------------------------------------------------------
// Subgraph — shared return / mutation vehicle across steps 2–3
// ---------------------------------------------------------------------------

/// Aggregated subgraph produced by neighbor expansion, optionally
/// mutated by hierarchy / facet steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Subgraph {
    pub nodes: Vec<SubgraphNode>,
    pub edges: Vec<SubgraphEdge>,
    /// Set to `true` when the BFS hit `max_nodes` and stopped
    /// expanding the frontier. Callers render a "truncated" note in
    /// the LLM context so the model doesn't assume completeness.
    pub truncated: bool,
}

impl Subgraph {
    pub fn len_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn len_edges(&self) -> usize {
        self.edges.len()
    }

    pub fn find_node(&self, key: &EntityRef) -> Option<&SubgraphNode> {
        self.nodes
            .iter()
            .find(|n| n.kind == key.kind && n.logical_id == key.logical_id)
    }
}

/// One node inside a [`Subgraph`]. `doc` mirrors the searchable
/// document the entry-point index carries so the LLM-render step has
/// everything it needs without another round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphNode {
    pub kind: String,
    pub logical_id: String,
    pub label: Option<String>,
    pub doc: Option<String>,
    /// Shortest BFS distance from any anchor. `0` = anchor itself.
    pub depth: u8,
}

/// One edge inside a [`Subgraph`]. `relation_kind` matches the value
/// stored in `ontology_entity_neighbors.relation_kind`
/// (e.g. `property_of`, `value_set`, `has_edge`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphEdge {
    pub from: EntityRef,
    pub to: EntityRef,
    pub relation_kind: String,
}

/// Pure subgraph → markdown renderer. Extracted so unit tests can
/// pin the LLM-context shape without a Postgres pool; the
/// `OntologyNavigationStore::render_subgraph_for_llm` trait method
/// is a thin forwarder to this function.
//
// `writeln!`/`write!` to a `String` buffer is infallible — the
// `fmt::Write` impl for `String` never returns Err, so the
// `let_underscore_must_use` gate is satisfied by the function-level
// allow rather than wrapping each call.
#[allow(clippy::let_underscore_must_use)]
pub fn render_subgraph_as_llm_markdown(
    subgraph: &Subgraph,
    options: &LlmRenderOptions,
) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    let mut out = String::with_capacity(2048);

    // Group nodes by kind — the LLM reads kind-sectioned markdown
    // better than a flat list. Within a section, closer anchors
    // (lower depth) sort first.
    let mut by_kind: BTreeMap<&str, Vec<&SubgraphNode>> = BTreeMap::new();
    for n in &subgraph.nodes {
        by_kind.entry(n.kind.as_str()).or_default().push(n);
    }
    for bucket in by_kind.values_mut() {
        bucket.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| a.logical_id.cmp(&b.logical_id))
        });
    }

    let mut rendered: usize = 0;
    let mut hit_token_cap = false;
    'outer: for (kind, nodes) in &by_kind {
        let _ = writeln!(out, "## {kind}");
        for n in nodes {
            if rendered >= options.max_nodes {
                break 'outer;
            }
            // Token-budget pre-check — estimate the running
            // size BEFORE we commit the node so the cap is a
            // ceiling, not a soft target. If even one node
            // would push us over, we stop emitting and let the
            // trailing "... and N more" footer render.
            if let Some(cap) = options.max_tokens
                && estimate_tokens_chars(&out) >= cap
            {
                hit_token_cap = true;
                break 'outer;
            }
            let label = n.label.as_deref().unwrap_or(n.logical_id.as_str());
            let _ = writeln!(out, "- **{}** · `{}`", label, n.logical_id);
            if options.include_doc_snippets
                && let Some(doc) = &n.doc
                && !doc.is_empty()
            {
                let _ = writeln!(out, "  {doc}");
            }
            rendered += 1;
        }
        let _ = writeln!(out);
    }

    if rendered < subgraph.nodes.len() {
        let remaining = subgraph.nodes.len() - rendered;
        let reason = if hit_token_cap {
            "token budget"
        } else {
            "prompt"
        };
        let _ = writeln!(
            out,
            "_... and {remaining} more entities (subgraph truncated for {reason})_"
        );
    }

    if !subgraph.edges.is_empty() {
        let mut by_relation: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &subgraph.edges {
            *by_relation.entry(e.relation_kind.as_str()).or_insert(0) += 1;
        }
        let _ = writeln!(out, "## Relations");
        for (rel, count) in by_relation {
            let _ = writeln!(out, "- `{rel}` × {count}");
        }
    }

    if subgraph.truncated {
        out.push_str("\n_Note: subgraph was truncated by `max_nodes` or `max_codes_per_code_system` — expand with more specific anchors if you need the rest._\n");
    }

    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn node(kind: &str, id: &str, depth: u8) -> SubgraphNode {
        SubgraphNode {
            kind: kind.into(),
            logical_id: id.into(),
            label: None,
            doc: None,
            depth,
        }
    }

    fn edge(from_k: &str, from_id: &str, rel: &str, to_k: &str, to_id: &str) -> SubgraphEdge {
        SubgraphEdge {
            from: EntityRef::new(from_k, from_id),
            to: EntityRef::new(to_k, to_id),
            relation_kind: rel.into(),
        }
    }

    #[test]
    fn render_groups_nodes_by_kind_section_headers() {
        let g = Subgraph {
            nodes: vec![
                node("NodeType", "nt_customer", 0),
                node("PropertyDef", "p_grade", 1),
                node("NodeType", "nt_order", 1),
            ],
            edges: vec![],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(&g, &LlmRenderOptions::default());
        assert!(md.contains("## NodeType"));
        assert!(md.contains("## PropertyDef"));
        // Depth-sort: nt_customer (0) before nt_order (1)
        let c = md.find("nt_customer").unwrap();
        let o = md.find("nt_order").unwrap();
        assert!(c < o, "{md}");
    }

    #[test]
    fn render_truncates_when_max_nodes_hit() {
        let many: Vec<SubgraphNode> = (0..100)
            .map(|i| node("NodeType", &format!("nt_{i:03}"), 0))
            .collect();
        let g = Subgraph {
            nodes: many,
            edges: vec![],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(
            &g,
            &LlmRenderOptions {
                max_nodes: 10,
                max_tokens: None,
                include_doc_snippets: false,
            },
        );
        assert!(md.contains("and 90 more entities"));
    }

    #[test]
    fn render_emits_relation_summary_counts() {
        let g = Subgraph {
            nodes: vec![node("NodeType", "a", 0), node("NodeType", "b", 0)],
            edges: vec![
                edge("NodeType", "a", "property_of", "NodeType", "b"),
                edge("NodeType", "a", "property_of", "NodeType", "b"),
                edge("NodeType", "a", "value_set", "NodeType", "b"),
            ],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(&g, &LlmRenderOptions::default());
        assert!(md.contains("## Relations"));
        assert!(md.contains("`property_of` × 2"));
        assert!(md.contains("`value_set` × 1"));
    }

    #[test]
    fn render_surfaces_truncated_marker_when_set() {
        let g = Subgraph {
            nodes: vec![node("NodeType", "a", 0)],
            edges: vec![],
            truncated: true,
        };
        let md = render_subgraph_as_llm_markdown(&g, &LlmRenderOptions::default());
        assert!(md.contains("subgraph was truncated"));
    }

    #[test]
    fn render_omits_doc_block_when_disabled() {
        let n = SubgraphNode {
            kind: "NodeType".into(),
            logical_id: "nt_customer".into(),
            label: Some("Customer".into()),
            doc: Some("Buyers of things".into()),
            depth: 0,
        };
        let g = Subgraph {
            nodes: vec![n],
            edges: vec![],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(
            &g,
            &LlmRenderOptions {
                max_nodes: 10,
                max_tokens: None,
                include_doc_snippets: false,
            },
        );
        assert!(md.contains("**Customer**"));
        assert!(!md.contains("Buyers of things"), "{md}");
    }

    #[test]
    fn estimate_tokens_chars_uses_unicode_char_count() {
        // 6 ASCII chars = 6 chars / 3 = 2 tokens
        assert_eq!(estimate_tokens_chars("hello!"), 2);
        // 3 Korean chars = 3 chars / 3 = 1 token (each Korean
        // codepoint is 3 bytes in UTF-8, but 1 char by Rust's
        // char count). Bytes-based estimation would over-count
        // by 3x.
        assert_eq!(estimate_tokens_chars("안녕요"), 1);
        // Empty
        assert_eq!(estimate_tokens_chars(""), 0);
    }

    #[test]
    fn render_respects_max_tokens_budget() {
        // 50 nodes, each carrying a 100-char doc — without a
        // token cap they'd all render. With a tight cap, the
        // renderer stops mid-stream and emits the "token
        // budget" footer.
        let many: Vec<SubgraphNode> = (0..50)
            .map(|i| SubgraphNode {
                kind: "NodeType".into(),
                logical_id: format!("nt_{i:03}"),
                label: Some(format!("Node{i:03}")),
                doc: Some(
                    "This is a longer description that drives the token \
                     estimate up so the budget cap fires."
                        .into(),
                ),
                depth: 0,
            })
            .collect();
        let g = Subgraph {
            nodes: many,
            edges: vec![],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(
            &g,
            &LlmRenderOptions {
                max_nodes: 1000,
                // ~150 tokens budget. Each node ≈ 40+ tokens
                // (label + doc combined), so we expect ~3-4
                // nodes rendered before the cap fires.
                max_tokens: Some(150),
                include_doc_snippets: true,
            },
        );
        // Footer must call out the *token* budget, not the
        // generic prompt-truncation copy, so the operator
        // knows the cap they hit.
        assert!(md.contains("truncated for token budget"), "{md}");
        // And not every node landed.
        let rendered_count = md.matches("- **Node").count();
        assert!(
            rendered_count > 0 && rendered_count < 50,
            "rendered={rendered_count}",
        );
    }

    #[test]
    fn render_no_token_cap_renders_all_within_max_nodes() {
        // No `max_tokens` — falls back to count cap only. The
        // footer should NOT mention "token budget" since the
        // cap path didn't fire.
        let many: Vec<SubgraphNode> = (0..3)
            .map(|i| SubgraphNode {
                kind: "NodeType".into(),
                logical_id: format!("nt_{i}"),
                label: None,
                doc: None,
                depth: 0,
            })
            .collect();
        let g = Subgraph {
            nodes: many,
            edges: vec![],
            truncated: false,
        };
        let md = render_subgraph_as_llm_markdown(
            &g,
            &LlmRenderOptions {
                max_nodes: 100,
                max_tokens: None,
                include_doc_snippets: false,
            },
        );
        assert!(!md.contains("token budget"), "{md}");
        assert!(md.contains("nt_0"));
        assert!(md.contains("nt_2"));
    }

    #[test]
    fn subgraph_find_node_returns_by_entity_ref() {
        let g = Subgraph {
            nodes: vec![
                node("NodeType", "a", 0),
                node("NodeType", "b", 1),
                node("PropertyDef", "p_grade", 1),
            ],
            edges: vec![],
            truncated: false,
        };
        let hit = g.find_node(&EntityRef::new("PropertyDef", "p_grade"));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().depth, 1);
        let miss = g.find_node(&EntityRef::new("NodeType", "nope"));
        assert!(miss.is_none());
    }
}
