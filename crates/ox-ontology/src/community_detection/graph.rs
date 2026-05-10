//! Undirected weighted graph projection of an
//! [`crate::OntologyIR`] for community detection.
//!
//! The projection is intentionally lossy — community detection
//! cares about *thematic adjacency* of ontology entities, not
//! about every IR detail. The builder keeps the entity types
//! that carry retrieval-relevant identity (NodeType, EdgeType,
//! GlossaryTerm, ConceptDef, SegmentDef) and connects them via
//! the relationships the GraphRAG retrieval path traverses.
//!
//! ## Determinism
//!
//! Node ordering follows the IR's own iteration order
//! (insertion order on the underlying `Vec`s). Edge ordering
//! mirrors the order in which the builder walks the IR's
//! collections. Two runs over an unchanged IR yield byte-
//! identical [`CommunityGraph`] instances; the seeded
//! algorithms inherit that determinism.
//!
//! ## Extension points
//!
//! Adding a new entity kind to detection = one match arm in
//! [`add_concept_edges`] (or a sibling helper) and a new
//! `node_kind` push in [`build_ontology_graph`]. The trait /
//! algorithm layer doesn't change.

use crate::ir::OntologyIR;
use crate::storage::EntityKind;

/// One node in the [`CommunityGraph`]. Carries the entity
/// identity the cron needs to write
/// `community_summaries.member_*` rows; the algorithm itself
/// only consumes the `idx` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityGraphNode {
    pub kind: EntityKind,
    pub logical_id: String,
    /// Display name when the IR carries a human-friendly label.
    /// Surfaced to the LLM summariser; empty for entity kinds
    /// without a notion of display name.
    pub display_name: String,
}

/// Undirected weighted graph backed by parallel adjacency
/// lists. The representation is intentionally simple — for the
/// 10²-10³ node scale the platform sees, sparse adjacency
/// lists outperform any matrix-based encoding and stay
/// allocator-friendly.
///
/// `neighbours[i]` holds `(j, weight)` pairs for every edge
/// incident to node `i`. Edge symmetry is the builder's
/// responsibility — the builder pushes each `(i, j, w)` into
/// both `neighbours[i]` and `neighbours[j]`.
#[derive(Debug, Clone)]
pub struct CommunityGraph {
    pub nodes: Vec<CommunityGraphNode>,
    pub neighbours: Vec<Vec<(usize, f32)>>,
}

impl CommunityGraph {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Total weighted degree. Used by modularity calculations
    /// and by the label-propagation tie-break.
    pub fn total_edge_weight(&self) -> f32 {
        // Each edge contributes `2 * weight` (counted on both
        // endpoints). Standard graph-theory convention.
        self.neighbours
            .iter()
            .flat_map(|adj| adj.iter().map(|(_, w)| *w))
            .sum::<f32>()
            / 2.0
    }

    pub fn weighted_degree(&self, node: usize) -> f32 {
        self.neighbours[node].iter().map(|(_, w)| *w).sum()
    }
}

/// Edge weights — heuristic but stable. Mirrors the
/// retrieval-time intuition that a NodeType is more strongly
/// associated with the EdgeTypes it participates in than with
/// the abstract Concept it realises, and that a GlossaryTerm's
/// link to its Concept is the canonicalisation gateway.
///
/// Tunable later via a workspace-scoped policy if production
/// data shows uniform weights underfit the topology; today the
/// goal is reproducibility, not tuned recall.
mod weights {
    pub const NODE_TO_EDGE: f32 = 1.0;
    pub const NODE_TO_CONCEPT: f32 = 0.7;
    pub const EDGE_TO_CONCEPT: f32 = 0.7;
    pub const TERM_TO_CONCEPT: f32 = 0.6;
    pub const SEGMENT_TO_NODE: f32 = 0.8;
}

/// Project an [`OntologyIR`] into a [`CommunityGraph`].
///
/// Returns an empty graph for an IR with no node types — the
/// detector trait handles the empty case as a no-op. This is
/// the cold-start path: a workspace whose canonical ontology
/// is empty gets `community_summaries.len() = 0` for that
/// version, no error, no warning.
pub fn build_ontology_graph(ir: &OntologyIR) -> CommunityGraph {
    let mut nodes: Vec<CommunityGraphNode> = Vec::new();
    let mut index_by_key: std::collections::HashMap<(EntityKind, String), usize> =
        std::collections::HashMap::new();

    let push_node = |nodes: &mut Vec<CommunityGraphNode>,
                     index: &mut std::collections::HashMap<(EntityKind, String), usize>,
                     kind: EntityKind,
                     logical_id: String,
                     display_name: String|
     -> usize {
        let key = (kind, logical_id.clone());
        if let Some(idx) = index.get(&key) {
            return *idx;
        }
        let idx = nodes.len();
        nodes.push(CommunityGraphNode {
            kind,
            logical_id,
            display_name,
        });
        index.insert(key, idx);
        idx
    };

    // Topology nodes — the backbone of the graph.
    for nt in &ir.node_types {
        push_node(
            &mut nodes,
            &mut index_by_key,
            EntityKind::NodeType,
            nt.id.as_str().to_string(),
            nt.label.as_str().to_string(),
        );
    }
    for et in &ir.edge_types {
        push_node(
            &mut nodes,
            &mut index_by_key,
            EntityKind::EdgeType,
            et.id.as_str().to_string(),
            et.label.as_str().to_string(),
        );
    }

    // Concept layer — anchors the semantic identity the
    // GraphRAG retrieval path snaps onto.
    for c in ir.concepts() {
        push_node(
            &mut nodes,
            &mut index_by_key,
            EntityKind::Concept,
            c.id.as_str().to_string(),
            c.canonical_term_id.as_str().to_string(),
        );
    }
    for gt in ir.glossary() {
        push_node(
            &mut nodes,
            &mut index_by_key,
            EntityKind::GlossaryTerm,
            gt.id.as_str().to_string(),
            gt.term.as_str().to_string(),
        );
    }

    // Segments — Φ8.2 first-class IR collection. A NodeType's
    // segments share its community, so the segment edge
    // weight is the strongest of the non-topology edges.
    for seg in ir.segments() {
        push_node(
            &mut nodes,
            &mut index_by_key,
            EntityKind::Segment,
            seg.id.as_str().to_string(),
            seg.name.clone(),
        );
    }

    let mut neighbours: Vec<Vec<(usize, f32)>> = vec![Vec::new(); nodes.len()];

    let mut add_edge = |neighbours: &mut Vec<Vec<(usize, f32)>>, a: usize, b: usize, w: f32| {
        if a == b {
            return;
        }
        neighbours[a].push((b, w));
        neighbours[b].push((a, w));
    };

    // EdgeType ↔ source NodeType + ↔ target NodeType.
    for et in &ir.edge_types {
        let et_idx = match index_by_key.get(&(EntityKind::EdgeType, et.id.as_str().to_string())) {
            Some(i) => *i,
            None => continue,
        };
        link_node_to_edge_endpoint(
            &mut neighbours,
            &index_by_key,
            et_idx,
            &et.source_node_id,
            weights::NODE_TO_EDGE,
            &mut add_edge,
        );
        link_node_to_edge_endpoint(
            &mut neighbours,
            &index_by_key,
            et_idx,
            &et.target_node_id,
            weights::NODE_TO_EDGE,
            &mut add_edge,
        );
    }

    // NodeType ↔ Concept (when realised).
    for nt in &ir.node_types {
        let nt_idx = match index_by_key.get(&(EntityKind::NodeType, nt.id.as_str().to_string())) {
            Some(i) => *i,
            None => continue,
        };
        link_to_concept(
            &mut neighbours,
            &index_by_key,
            nt_idx,
            nt.concept_id.as_ref(),
            weights::NODE_TO_CONCEPT,
            &mut add_edge,
        );
    }
    for et in &ir.edge_types {
        let et_idx = match index_by_key.get(&(EntityKind::EdgeType, et.id.as_str().to_string())) {
            Some(i) => *i,
            None => continue,
        };
        link_to_concept(
            &mut neighbours,
            &index_by_key,
            et_idx,
            et.concept_id.as_ref(),
            weights::EDGE_TO_CONCEPT,
            &mut add_edge,
        );
    }

    // GlossaryTerm ↔ Concept (canonical / alias).
    for gt in ir.glossary() {
        let gt_idx = match index_by_key.get(&(EntityKind::GlossaryTerm, gt.id.as_str().to_string()))
        {
            Some(i) => *i,
            None => continue,
        };
        link_to_concept(
            &mut neighbours,
            &index_by_key,
            gt_idx,
            gt.concept_id.as_ref(),
            weights::TERM_TO_CONCEPT,
            &mut add_edge,
        );
    }

    // Segment ↔ owning NodeType.
    for seg in ir.segments() {
        let seg_idx = match index_by_key.get(&(EntityKind::Segment, seg.id.as_str().to_string())) {
            Some(i) => *i,
            None => continue,
        };
        if let Some(nt_idx) = index_by_key.get(&(
            EntityKind::NodeType,
            seg.target_node_type_id.as_str().to_string(),
        )) {
            add_edge(&mut neighbours, seg_idx, *nt_idx, weights::SEGMENT_TO_NODE);
        }
    }

    CommunityGraph { nodes, neighbours }
}

fn link_node_to_edge_endpoint<F>(
    neighbours: &mut Vec<Vec<(usize, f32)>>,
    index: &std::collections::HashMap<(EntityKind, String), usize>,
    et_idx: usize,
    node_id: &crate::ir::NodeTypeId,
    weight: f32,
    add_edge: &mut F,
) where
    F: FnMut(&mut Vec<Vec<(usize, f32)>>, usize, usize, f32),
{
    if let Some(nt_idx) = index.get(&(EntityKind::NodeType, node_id.as_str().to_string())) {
        add_edge(neighbours, et_idx, *nt_idx, weight);
    }
}

fn link_to_concept<F>(
    neighbours: &mut Vec<Vec<(usize, f32)>>,
    index: &std::collections::HashMap<(EntityKind, String), usize>,
    src_idx: usize,
    concept_id: Option<&crate::concept::ConceptId>,
    weight: f32,
    add_edge: &mut F,
) where
    F: FnMut(&mut Vec<Vec<(usize, f32)>>, usize, usize, f32),
{
    let Some(c) = concept_id else {
        return;
    };
    if let Some(c_idx) = index.get(&(EntityKind::Concept, c.as_str().to_string())) {
        add_edge(neighbours, src_idx, *c_idx, weight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{NodeTypeDef, NodeTypeId};
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;

    fn minimal_node(id: &str, label: &str) -> NodeTypeDef {
        NodeTypeDef {
            id: NodeTypeId::new(id),
            label: GraphLabel::new(label).expect("test label"),
            description: LocalizedText::default(),
            properties: vec![],
            constraints: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn empty_ir_yields_empty_graph() {
        let ir = OntologyIR::new(
            "ont".into(),
            "X".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        let g = build_ontology_graph(&ir);
        assert!(g.is_empty());
        assert_eq!(g.total_edge_weight(), 0.0);
    }

    #[test]
    fn edge_type_links_source_and_target_node_types() {
        let nt_a = minimal_node("nt-a", "A");
        let nt_b = minimal_node("nt-b", "B");
        let et = crate::ir::EdgeTypeDef {
            id: crate::ir::EdgeTypeId::new("et-1"),
            label: GraphLabel::new("Connects").expect("test label"),
            source_node_id: NodeTypeId::new("nt-a"),
            target_node_id: NodeTypeId::new("nt-b"),
            ..Default::default()
        };
        let ir = OntologyIR::new(
            "ont".into(),
            "X".into(),
            LocalizedText::default(),
            1,
            vec![nt_a, nt_b],
            vec![et],
            vec![],
        );
        let g = build_ontology_graph(&ir);
        // 2 NodeTypes + 1 EdgeType.
        assert_eq!(g.node_count(), 3);
        // EdgeType has 2 incident edges (one per endpoint).
        let et_idx = g
            .nodes
            .iter()
            .position(|n| n.kind == EntityKind::EdgeType)
            .unwrap();
        assert_eq!(g.neighbours[et_idx].len(), 2);
        assert!(
            (g.weighted_degree(et_idx) - 2.0 * weights::NODE_TO_EDGE).abs() < 1e-6,
            "EdgeType degree must equal 2 × NODE_TO_EDGE",
        );
    }

    #[test]
    fn self_referential_edge_does_not_create_loop() {
        // Source == target — the platform allows self edges
        // (e.g. `Manages: Employee → Employee`). The community
        // graph dedupes self-loops because they don't carry
        // community-detection signal.
        let nt = minimal_node("nt-emp", "Employee");
        let et = crate::ir::EdgeTypeDef {
            id: crate::ir::EdgeTypeId::new("et-manages"),
            label: GraphLabel::new("Manages").expect("test label"),
            source_node_id: NodeTypeId::new("nt-emp"),
            target_node_id: NodeTypeId::new("nt-emp"),
            ..Default::default()
        };
        let ir = OntologyIR::new(
            "ont".into(),
            "X".into(),
            LocalizedText::default(),
            1,
            vec![nt],
            vec![et],
            vec![],
        );
        let g = build_ontology_graph(&ir);
        let et_idx = g
            .nodes
            .iter()
            .position(|n| n.kind == EntityKind::EdgeType)
            .unwrap();
        // Both endpoints resolve to the same NodeType. Self-edge
        // is dropped by `add_edge` (the helper rejects `a == b`)
        // — but EdgeType ↔ NodeType is a different pair, so the
        // first call adds it, the second is a duplicate edge to
        // the same neighbour. We accept the duplicate here
        // because the algorithm correctly sums weights.
        assert_eq!(g.neighbours[et_idx].len(), 2);
    }
}
