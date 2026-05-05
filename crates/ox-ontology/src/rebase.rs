//! Conflict-aware rebase analysis.
//!
//! When a draft's `parent_version_id` is stale (a sibling draft
//! committed onto the canonical lineage between the fork and now),
//! a structural rebase pins the draft's parent pointer forward —
//! but the draft and the canonical edits may have *touched the
//! same entities*. Without surfacing that overlap an operator who
//! rebases blindly might land a commit that silently regresses
//! a sibling's work, or vice-versa.
//!
//! This module computes a [`RebaseAnalysis`]: the canonical's own
//! evolution since the draft's parent (`base_to_head`), the
//! draft's evolution against its parent (`base_to_draft`), and
//! the conflict surface between them — entities that both sides
//! mutated in incompatible ways.
//!
//! The conflict model is intentionally narrow at this layer:
//!
//! - **Add/Add same id** — both sides added a node/edge with the
//!   same id (different content). Operator must pick one.
//! - **Modify/Remove** — one side modified, the other removed.
//!   The modify is "phantom" if removal lands.
//! - **Modify/Modify on the same atomic field** — both sides
//!   touched the same node label, the same property's nullability,
//!   the same edge's source endpoint, etc. Reconciliation is
//!   field-by-field.
//!
//! Disjoint changes (one side adds a brand-new node, the other
//! tweaks a different node's description) merge cleanly and do
//! NOT appear in the conflict list. Operators who see no
//! conflicts can rebase with confidence; operators who see them
//! have a typed list to triage before pinning forward.
//!
//! The analyser does NOT mutate the draft's IR. The HTTP layer
//! returns the analysis as a preview; the FE renders it as a
//! per-entity inspection panel; the actual rebase still pins
//! `parent_version_id` to the canonical head and leaves the
//! draft's content under operator control. This keeps the rebase
//! latest-wins-on-the-pointer (no auto-merge regressions) while
//! giving the operator the data they need to merge by hand.

use serde::Serialize;

use crate::diff::{compute_diff, EdgeChange, NodeChange, OntologyDiff, PropertyChange};
use crate::ir::OntologyIR;

#[derive(Debug, Clone, Serialize)]
pub struct RebaseAnalysis {
    /// Diff from the draft's parent version to the canonical head.
    /// Captures what the canonical has done since the draft was
    /// forked.
    pub base_to_head: OntologyDiff,
    /// Diff from the draft's parent version to the draft's
    /// in-flight ontology. Captures what the draft has done.
    pub base_to_draft: OntologyDiff,
    /// Per-entity conflicts — a non-empty list means the operator
    /// must reconcile before rebasing. Empty list = clean rebase.
    pub conflicts: Vec<RebaseConflict>,
}

impl RebaseAnalysis {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RebaseConflict {
    /// Both sides added a node/edge with the same id but
    /// different content. Operator picks one or merges by hand.
    AddAdd {
        entity_kind: ConflictEntityKind,
        entity_id: String,
        label: String,
    },
    /// Draft modified an entity that canonical removed (or
    /// vice-versa). The modify will land against a vanished
    /// entity — the operator must either re-add or drop the
    /// modify.
    ModifyRemove {
        entity_kind: ConflictEntityKind,
        entity_id: String,
        label: String,
        /// `"draft"` when the draft holds the modify and the
        /// canonical removed; `"head"` when the canonical
        /// modified and the draft removed.
        modifier: ConflictSide,
    },
    /// Both sides modified the same entity in overlapping ways.
    /// The conflict carries the per-axis intersection so the
    /// operator can see exactly which atomic field clashes.
    ModifyModify {
        entity_kind: ConflictEntityKind,
        entity_id: String,
        label: String,
        /// One entry per overlapping atomic axis. Order matches
        /// the canonical change variant order so the FE renders
        /// stably.
        axes: Vec<ConflictAxis>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictEntityKind {
    Node,
    Edge,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSide {
    Draft,
    Head,
}

/// One overlapping atomic axis on a Modify/Modify conflict.
/// Carries the canonical and draft values so the operator picks
/// without an extra fetch.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "axis", rename_all = "snake_case")]
pub enum ConflictAxis {
    /// Both sides changed the entity's label.
    Label { head: String, draft: String },
    /// Both sides changed the entity's description (default
    /// localized text rendered for the diff).
    Description { head: String, draft: String },
    /// Both sides changed the same edge's source endpoint.
    Source { head: String, draft: String },
    /// Both sides changed the same edge's target endpoint.
    Target { head: String, draft: String },
    /// Both sides changed the same edge's cardinality.
    Cardinality { head: String, draft: String },
    /// Both sides modified the same property in overlapping ways.
    PropertyOverlap {
        property_name: String,
        /// One entry per atomic property axis (type / nullability
        /// / default / description) that both sides touched.
        atoms: Vec<PropertyConflictAxis>,
    },
    /// Draft modified a property that canonical removed (or
    /// vice-versa) on the same entity.
    PropertyModifyRemove {
        property_name: String,
        modifier: ConflictSide,
    },
    /// Both sides added a property with the same name but
    /// different content.
    PropertyAddAdd { property_name: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "axis", rename_all = "snake_case")]
pub enum PropertyConflictAxis {
    Type { head: String, draft: String },
    Nullability { head: bool, draft: bool },
    Description { head: String, draft: String },
    DefaultValue { head: Option<String>, draft: Option<String> },
}

/// Compute the rebase analysis. The caller supplies three IRs:
/// the draft's parent version (the fork point), the canonical
/// head (where the canonical lineage has advanced to), and the
/// draft's in-flight ontology. All three diffs are taken from
/// the parent baseline so add/remove/modify lattices align.
///
/// When `base` and `head` are identical (fast-forward case — the
/// canonical did not advance), `base_to_head` is empty and
/// `conflicts` is necessarily empty. The FE treats that as
/// "already pinned to head" and skips the rebase call.
pub fn analyze_rebase(
    base: &OntologyIR,
    head: &OntologyIR,
    draft: &OntologyIR,
) -> RebaseAnalysis {
    let base_to_head = compute_diff(base, head);
    let base_to_draft = compute_diff(base, draft);
    let conflicts = detect_conflicts(&base_to_head, &base_to_draft);
    RebaseAnalysis {
        base_to_head,
        base_to_draft,
        conflicts,
    }
}

fn detect_conflicts(head: &OntologyDiff, draft: &OntologyDiff) -> Vec<RebaseConflict> {
    let mut out = Vec::new();

    // Add/Add — same id added on both sides.
    let head_added: std::collections::HashMap<&str, &str> = head
        .added_nodes
        .iter()
        .map(|n| (n.id.as_str(), n.label.as_str()))
        .collect();
    for n in &draft.added_nodes {
        if head_added.contains_key(n.id.as_str()) {
            out.push(RebaseConflict::AddAdd {
                entity_kind: ConflictEntityKind::Node,
                entity_id: n.id.to_string(),
                label: n.label.to_string(),
            });
        }
    }
    let head_added_edges: std::collections::HashMap<&str, &str> = head
        .added_edges
        .iter()
        .map(|e| (e.id.as_str(), e.label.as_str()))
        .collect();
    for e in &draft.added_edges {
        if head_added_edges.contains_key(e.id.as_str()) {
            out.push(RebaseConflict::AddAdd {
                entity_kind: ConflictEntityKind::Edge,
                entity_id: e.id.to_string(),
                label: e.label.to_string(),
            });
        }
    }

    // Modify/Remove — node modified on one side, removed on the
    // other.
    let head_removed_nodes: std::collections::HashSet<&str> =
        head.removed_nodes.iter().map(|n| n.id.as_str()).collect();
    for nd in &draft.modified_nodes {
        if head_removed_nodes.contains(nd.node_id.as_str()) {
            out.push(RebaseConflict::ModifyRemove {
                entity_kind: ConflictEntityKind::Node,
                entity_id: nd.node_id.to_string(),
                label: nd.label.to_string(),
                modifier: ConflictSide::Draft,
            });
        }
    }
    let draft_removed_nodes: std::collections::HashSet<&str> =
        draft.removed_nodes.iter().map(|n| n.id.as_str()).collect();
    for nd in &head.modified_nodes {
        if draft_removed_nodes.contains(nd.node_id.as_str()) {
            out.push(RebaseConflict::ModifyRemove {
                entity_kind: ConflictEntityKind::Node,
                entity_id: nd.node_id.to_string(),
                label: nd.label.to_string(),
                modifier: ConflictSide::Head,
            });
        }
    }
    let head_removed_edges: std::collections::HashSet<&str> =
        head.removed_edges.iter().map(|e| e.id.as_str()).collect();
    for ed in &draft.modified_edges {
        if head_removed_edges.contains(ed.edge_id.as_str()) {
            out.push(RebaseConflict::ModifyRemove {
                entity_kind: ConflictEntityKind::Edge,
                entity_id: ed.edge_id.to_string(),
                label: ed.label.to_string(),
                modifier: ConflictSide::Draft,
            });
        }
    }
    let draft_removed_edges: std::collections::HashSet<&str> =
        draft.removed_edges.iter().map(|e| e.id.as_str()).collect();
    for ed in &head.modified_edges {
        if draft_removed_edges.contains(ed.edge_id.as_str()) {
            out.push(RebaseConflict::ModifyRemove {
                entity_kind: ConflictEntityKind::Edge,
                entity_id: ed.edge_id.to_string(),
                label: ed.label.to_string(),
                modifier: ConflictSide::Head,
            });
        }
    }

    // Modify/Modify — both sides modified the same entity. Walk
    // each side's NodeChange list and intersect by axis.
    let head_node_map: std::collections::HashMap<&str, &crate::diff::NodeDiff> =
        head.modified_nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();
    for draft_node in &draft.modified_nodes {
        if let Some(head_node) = head_node_map.get(draft_node.node_id.as_str()) {
            let axes = node_conflict_axes(&head_node.changes, &draft_node.changes);
            if !axes.is_empty() {
                out.push(RebaseConflict::ModifyModify {
                    entity_kind: ConflictEntityKind::Node,
                    entity_id: draft_node.node_id.to_string(),
                    label: draft_node.label.to_string(),
                    axes,
                });
            }
        }
    }
    let head_edge_map: std::collections::HashMap<&str, &crate::diff::EdgeDiff> =
        head.modified_edges.iter().map(|e| (e.edge_id.as_str(), e)).collect();
    for draft_edge in &draft.modified_edges {
        if let Some(head_edge) = head_edge_map.get(draft_edge.edge_id.as_str()) {
            let axes = edge_conflict_axes(&head_edge.changes, &draft_edge.changes);
            if !axes.is_empty() {
                out.push(RebaseConflict::ModifyModify {
                    entity_kind: ConflictEntityKind::Edge,
                    entity_id: draft_edge.edge_id.to_string(),
                    label: draft_edge.label.to_string(),
                    axes,
                });
            }
        }
    }

    out
}

fn node_conflict_axes(head: &[NodeChange], draft: &[NodeChange]) -> Vec<ConflictAxis> {
    let mut axes = Vec::new();

    // Label.
    if let (Some(h), Some(d)) = (
        head.iter().find_map(node_label_changed),
        draft.iter().find_map(node_label_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Label {
                head: h.to_string(),
                draft: d.to_string(),
            });
        }
    }

    // Description.
    if let (Some(h), Some(d)) = (
        head.iter().find_map(node_description_changed),
        draft.iter().find_map(node_description_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Description {
                head: h,
                draft: d,
            });
        }
    }

    // Properties — collect each side's property activity by name.
    let head_props = collect_node_property_activity(head);
    let draft_props = collect_node_property_activity(draft);
    for (name, draft_act) in &draft_props {
        if let Some(head_act) = head_props.get(name) {
            if let Some(axis) = property_conflict_axis(name, head_act, draft_act) {
                axes.push(axis);
            }
        }
    }

    axes
}

fn edge_conflict_axes(head: &[EdgeChange], draft: &[EdgeChange]) -> Vec<ConflictAxis> {
    let mut axes = Vec::new();

    if let (Some(h), Some(d)) = (
        head.iter().find_map(edge_label_changed),
        draft.iter().find_map(edge_label_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Label {
                head: h.to_string(),
                draft: d.to_string(),
            });
        }
    }

    if let (Some(h), Some(d)) = (
        head.iter().find_map(edge_description_changed),
        draft.iter().find_map(edge_description_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Description {
                head: h,
                draft: d,
            });
        }
    }

    if let (Some(h), Some(d)) = (
        head.iter().find_map(edge_source_changed),
        draft.iter().find_map(edge_source_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Source {
                head: h.to_string(),
                draft: d.to_string(),
            });
        }
    }
    if let (Some(h), Some(d)) = (
        head.iter().find_map(edge_target_changed),
        draft.iter().find_map(edge_target_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Target {
                head: h.to_string(),
                draft: d.to_string(),
            });
        }
    }
    if let (Some(h), Some(d)) = (
        head.iter().find_map(edge_cardinality_changed),
        draft.iter().find_map(edge_cardinality_changed),
    ) {
        if h != d {
            axes.push(ConflictAxis::Cardinality {
                head: h,
                draft: d,
            });
        }
    }

    let head_props = collect_edge_property_activity(head);
    let draft_props = collect_edge_property_activity(draft);
    for (name, draft_act) in &draft_props {
        if let Some(head_act) = head_props.get(name) {
            if let Some(axis) = property_conflict_axis(name, head_act, draft_act) {
                axes.push(axis);
            }
        }
    }

    axes
}

// ---------------------------------------------------------------------------
// Internal helpers — "what did this side do to property X?"
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum PropertyActivity {
    Added,
    Removed,
    Modified(Vec<PropertyChange>),
}

fn collect_node_property_activity(
    changes: &[NodeChange],
) -> std::collections::HashMap<String, PropertyActivity> {
    let mut out = std::collections::HashMap::new();
    for c in changes {
        match c {
            NodeChange::PropertyAdded { property } => {
                out.insert(property.name.to_string(), PropertyActivity::Added);
            }
            NodeChange::PropertyRemoved { property } => {
                out.insert(property.name.to_string(), PropertyActivity::Removed);
            }
            NodeChange::PropertyModified {
                property_name,
                changes,
            } => {
                out.insert(
                    property_name.to_string(),
                    PropertyActivity::Modified(changes.clone()),
                );
            }
            _ => {}
        }
    }
    out
}

fn collect_edge_property_activity(
    changes: &[EdgeChange],
) -> std::collections::HashMap<String, PropertyActivity> {
    let mut out = std::collections::HashMap::new();
    for c in changes {
        match c {
            EdgeChange::PropertyAdded { property } => {
                out.insert(property.name.to_string(), PropertyActivity::Added);
            }
            EdgeChange::PropertyRemoved { property } => {
                out.insert(property.name.to_string(), PropertyActivity::Removed);
            }
            EdgeChange::PropertyModified {
                property_name,
                changes,
            } => {
                out.insert(
                    property_name.to_string(),
                    PropertyActivity::Modified(changes.clone()),
                );
            }
            _ => {}
        }
    }
    out
}

fn property_conflict_axis(
    name: &str,
    head: &PropertyActivity,
    draft: &PropertyActivity,
) -> Option<ConflictAxis> {
    match (head, draft) {
        (PropertyActivity::Added, PropertyActivity::Added) => {
            Some(ConflictAxis::PropertyAddAdd {
                property_name: name.to_string(),
            })
        }
        (PropertyActivity::Modified(_), PropertyActivity::Removed) => {
            Some(ConflictAxis::PropertyModifyRemove {
                property_name: name.to_string(),
                modifier: ConflictSide::Head,
            })
        }
        (PropertyActivity::Removed, PropertyActivity::Modified(_)) => {
            Some(ConflictAxis::PropertyModifyRemove {
                property_name: name.to_string(),
                modifier: ConflictSide::Draft,
            })
        }
        (PropertyActivity::Modified(h), PropertyActivity::Modified(d)) => {
            let atoms = property_modify_atoms(h, d);
            if atoms.is_empty() {
                None
            } else {
                Some(ConflictAxis::PropertyOverlap {
                    property_name: name.to_string(),
                    atoms,
                })
            }
        }
        _ => None,
    }
}

fn property_modify_atoms(
    head: &[PropertyChange],
    draft: &[PropertyChange],
) -> Vec<PropertyConflictAxis> {
    let mut out = Vec::new();
    if let (Some((ho, hn)), Some((do_, dn))) = (
        head.iter().find_map(property_type_changed),
        draft.iter().find_map(property_type_changed),
    ) {
        if hn != dn {
            out.push(PropertyConflictAxis::Type {
                head: format!("{ho} → {hn}"),
                draft: format!("{do_} → {dn}"),
            });
        }
    }
    if let (Some((ho, hn)), Some((do_, dn))) = (
        head.iter().find_map(property_nullability_changed),
        draft.iter().find_map(property_nullability_changed),
    ) {
        if hn != dn {
            let _ = (ho, do_);
            out.push(PropertyConflictAxis::Nullability {
                head: hn,
                draft: dn,
            });
        }
    }
    if let (Some(h), Some(d)) = (
        head.iter().find_map(property_description_changed),
        draft.iter().find_map(property_description_changed),
    ) {
        if h != d {
            out.push(PropertyConflictAxis::Description {
                head: h,
                draft: d,
            });
        }
    }
    if let (Some(h), Some(d)) = (
        head.iter().find_map(property_default_changed),
        draft.iter().find_map(property_default_changed),
    ) {
        if h != d {
            out.push(PropertyConflictAxis::DefaultValue {
                head: h,
                draft: d,
            });
        }
    }
    out
}

fn node_label_changed(c: &NodeChange) -> Option<String> {
    match c {
        NodeChange::LabelChanged { new, .. } => Some(new.to_string()),
        _ => None,
    }
}

fn node_description_changed(c: &NodeChange) -> Option<String> {
    match c {
        NodeChange::DescriptionChanged { new, .. } => Some(new.default.clone()),
        _ => None,
    }
}

fn edge_label_changed(c: &EdgeChange) -> Option<String> {
    match c {
        EdgeChange::LabelChanged { new, .. } => Some(new.to_string()),
        _ => None,
    }
}

fn edge_description_changed(c: &EdgeChange) -> Option<String> {
    match c {
        EdgeChange::DescriptionChanged { new, .. } => Some(new.default.clone()),
        _ => None,
    }
}

fn edge_source_changed(c: &EdgeChange) -> Option<String> {
    match c {
        EdgeChange::SourceChanged { new, .. } => Some(new.to_string()),
        _ => None,
    }
}

fn edge_target_changed(c: &EdgeChange) -> Option<String> {
    match c {
        EdgeChange::TargetChanged { new, .. } => Some(new.to_string()),
        _ => None,
    }
}

fn edge_cardinality_changed(c: &EdgeChange) -> Option<String> {
    match c {
        EdgeChange::CardinalityChanged { new, .. } => Some(format!("{new:?}")),
        _ => None,
    }
}

fn property_type_changed(c: &PropertyChange) -> Option<(String, String)> {
    match c {
        PropertyChange::TypeChanged { old, new } => Some((old.clone(), new.clone())),
        _ => None,
    }
}

fn property_nullability_changed(c: &PropertyChange) -> Option<(bool, bool)> {
    match c {
        PropertyChange::NullabilityChanged { old, new } => Some((*old, *new)),
        _ => None,
    }
}

fn property_description_changed(c: &PropertyChange) -> Option<String> {
    match c {
        PropertyChange::DescriptionChanged { new, .. } => Some(new.default.clone()),
        _ => None,
    }
}

fn property_default_changed(c: &PropertyChange) -> Option<Option<String>> {
    match c {
        PropertyChange::DefaultValueChanged { new, .. } => Some(new.clone()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;
    use crate::ir::{NodeTypeDef, PropertyDef};

    fn node(id: &str, label: &str, _description: &str) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: GraphLabel::new(label).unwrap(),
            description: LocalizedText::default(),
            properties: vec![],
            constraints: vec![],
            ..Default::default()
        }
    }

    fn ir_with(nodes: Vec<NodeTypeDef>) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "T".into(),
            LocalizedText::default(),
            1u32,
            nodes,
            vec![],
            vec![],
        )
        .expect("seed ir")
    }

    #[test]
    fn clean_rebase_when_changes_disjoint() {
        let base = ir_with(vec![node("n1", "Alpha", "")]);
        let head = ir_with(vec![node("n1", "Alpha", ""), node("n2", "Beta", "")]);
        let draft = ir_with(vec![node("n1", "Alpha", ""), node("n3", "Gamma", "")]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert!(analysis.is_clean(), "{:?}", analysis.conflicts);
    }

    #[test]
    fn add_add_same_id_conflict() {
        let base = ir_with(vec![node("n1", "Alpha", "")]);
        let head = ir_with(vec![node("n1", "Alpha", ""), node("n2", "Beta", "")]);
        let draft = ir_with(vec![
            node("n1", "Alpha", ""),
            node("n2", "DraftBeta", ""),
        ]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert_eq!(analysis.conflicts.len(), 1);
        assert!(matches!(
            analysis.conflicts[0],
            RebaseConflict::AddAdd {
                entity_kind: ConflictEntityKind::Node,
                ..
            }
        ));
    }

    #[test]
    fn modify_remove_conflict_fires() {
        // Base has n1 (Alpha). Head removes n1; draft modifies n1's
        // label to Renamed.
        let base = ir_with(vec![node("n1", "Alpha", "")]);
        let head = ir_with(vec![]);
        let draft = ir_with(vec![node("n1", "Renamed", "")]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert_eq!(analysis.conflicts.len(), 1);
        assert!(matches!(
            analysis.conflicts[0],
            RebaseConflict::ModifyRemove {
                modifier: ConflictSide::Draft,
                ..
            }
        ));
    }

    #[test]
    fn label_modify_conflict_when_both_sides_rename_differently() {
        let base = ir_with(vec![node("n1", "Alpha", "")]);
        let head = ir_with(vec![node("n1", "AlphaPrime", "")]);
        let draft = ir_with(vec![node("n1", "AlphaDraft", "")]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert_eq!(analysis.conflicts.len(), 1);
        let RebaseConflict::ModifyModify { axes, .. } = &analysis.conflicts[0]
        else {
            panic!("expected modify/modify, got {:?}", analysis.conflicts[0]);
        };
        assert!(axes
            .iter()
            .any(|a| matches!(a, ConflictAxis::Label { .. })));
    }

    #[test]
    fn label_modify_no_conflict_when_both_sides_pick_same_new_label() {
        let base = ir_with(vec![node("n1", "Alpha", "")]);
        let head = ir_with(vec![node("n1", "AlphaPrime", "")]);
        let draft = ir_with(vec![node("n1", "AlphaPrime", "")]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert!(analysis.is_clean(), "{:?}", analysis.conflicts);
    }

    fn node_with_prop(
        id: &str,
        label: &str,
        prop_id: &str,
        prop_name: &str,
        prop_type: PropertyType,
    ) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: GraphLabel::new(label).unwrap(),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: prop_id.into(),
                name: PropertyKey::new(prop_name).unwrap(),
                property_type: prop_type,
                nullable: true,
                default_value: None,
                description: LocalizedText::default(),
                classification: None,
                ..Default::default()
            }],
            constraints: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn property_type_modify_conflict_atomic() {
        let base = ir_with(vec![node_with_prop(
            "n1",
            "X",
            "p1",
            "p",
            PropertyType::String,
        )]);
        let head = ir_with(vec![node_with_prop(
            "n1",
            "X",
            "p1",
            "p",
            PropertyType::Int,
        )]);
        let draft = ir_with(vec![node_with_prop(
            "n1",
            "X",
            "p1",
            "p",
            PropertyType::Float,
        )]);
        let analysis = analyze_rebase(&base, &head, &draft);
        assert_eq!(analysis.conflicts.len(), 1);
        let RebaseConflict::ModifyModify { axes, .. } = &analysis.conflicts[0]
        else {
            panic!("expected modify/modify");
        };
        let prop_axis = axes
            .iter()
            .find_map(|a| match a {
                ConflictAxis::PropertyOverlap { atoms, .. } => Some(atoms),
                _ => None,
            })
            .expect("property overlap present");
        assert!(prop_axis
            .iter()
            .any(|a| matches!(a, PropertyConflictAxis::Type { .. })));
    }
}
