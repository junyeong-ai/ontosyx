use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::ontology_ir::*;
use crate::types::PropertyType;

// ---------------------------------------------------------------------------
// OntologyDiff — structural diff between two OntologyIR versions
//
// Matches entities by their stable UUIDs (NodeTypeId, EdgeTypeId, PropertyId),
// not by labels or names. This means renames are detected as modifications,
// not as delete+add pairs.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct OntologyDiff {
    pub added_nodes: Vec<NodeTypeDef>,
    pub removed_nodes: Vec<NodeTypeDef>,
    pub modified_nodes: Vec<NodeDiff>,
    pub added_edges: Vec<EdgeTypeDef>,
    pub removed_edges: Vec<EdgeTypeDef>,
    pub modified_edges: Vec<EdgeDiff>,
    pub summary: DiffSummary,
}

impl OntologyDiff {
    /// Returns true if the diff contains no changes.
    pub fn is_empty(&self) -> bool {
        self.summary.total_changes == 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDiff {
    pub node_id: NodeTypeId,
    pub label: String,
    pub changes: Vec<NodeChange>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeChange {
    LabelChanged {
        old: String,
        new: String,
    },
    DescriptionChanged {
        old: Option<String>,
        new: Option<String>,
    },
    PropertyAdded {
        property: PropertyDef,
    },
    PropertyRemoved {
        property: PropertyDef,
    },
    PropertyModified {
        property_name: String,
        changes: Vec<PropertyChange>,
    },
    ConstraintAdded {
        constraint: String,
    },
    ConstraintRemoved {
        constraint: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PropertyChange {
    TypeChanged {
        old: String,
        new: String,
    },
    NullabilityChanged {
        old: bool,
        new: bool,
    },
    DescriptionChanged {
        old: Option<String>,
        new: Option<String>,
    },
    DefaultValueChanged {
        old: Option<String>,
        new: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeDiff {
    pub edge_id: EdgeTypeId,
    pub label: String,
    pub changes: Vec<EdgeChange>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EdgeChange {
    LabelChanged {
        old: String,
        new: String,
    },
    DescriptionChanged {
        old: Option<String>,
        new: Option<String>,
    },
    SourceChanged {
        old: String,
        new: String,
    },
    TargetChanged {
        old: String,
        new: String,
    },
    CardinalityChanged {
        old: Cardinality,
        new: Cardinality,
    },
    PropertyAdded {
        property: PropertyDef,
    },
    PropertyRemoved {
        property: PropertyDef,
    },
    PropertyModified {
        property_name: String,
        changes: Vec<PropertyChange>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary {
    pub total_changes: usize,
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub nodes_modified: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
    pub edges_modified: usize,
    pub properties_added: usize,
    pub properties_removed: usize,
}

// ---------------------------------------------------------------------------
// Knowledge lifecycle — label impact classification
// ---------------------------------------------------------------------------

/// Extract labels affected by breaking changes (Cypher will definitely fail).
///
/// Breaking: removed nodes/edges, label renames, edge source/target changes.
/// These invalidate any knowledge referencing the old labels.
pub fn breaking_labels(diff: &OntologyDiff) -> Vec<String> {
    let mut labels = Vec::new();
    for n in &diff.removed_nodes {
        labels.push(n.label.clone());
    }
    for e in &diff.removed_edges {
        labels.push(e.label.clone());
    }
    for nd in &diff.modified_nodes {
        for c in &nd.changes {
            if let NodeChange::LabelChanged { old, .. } = c {
                labels.push(old.clone());
            }
        }
    }
    for ed in &diff.modified_edges {
        for c in &ed.changes {
            match c {
                EdgeChange::LabelChanged { old, .. } => labels.push(old.clone()),
                EdgeChange::SourceChanged { .. } | EdgeChange::TargetChanged { .. } => {
                    labels.push(ed.label.clone());
                }
                _ => {}
            }
        }
    }
    labels
}

/// Extract labels affected by structural changes (queries may break).
///
/// Structural: property removed, property type changed, cardinality changed.
/// Knowledge referencing these labels gets a version warning in RAG results.
pub fn structural_labels(diff: &OntologyDiff) -> Vec<String> {
    let mut labels = Vec::new();
    for nd in &diff.modified_nodes {
        let has_structural = nd.changes.iter().any(|c| match c {
            NodeChange::PropertyRemoved { .. } => true,
            NodeChange::PropertyModified { changes, .. } => {
                changes.iter().any(|pc| matches!(pc, PropertyChange::TypeChanged { .. }))
            }
            _ => false,
        });
        if has_structural {
            labels.push(nd.label.clone());
        }
    }
    for ed in &diff.modified_edges {
        let has_structural = ed.changes.iter().any(|c| {
            matches!(c, EdgeChange::CardinalityChanged { .. } | EdgeChange::PropertyRemoved { .. })
        });
        if has_structural {
            labels.push(ed.label.clone());
        }
    }
    labels
}

// ---------------------------------------------------------------------------
// compute_diff — the main entry point
// ---------------------------------------------------------------------------

/// Compute a structural diff between two ontology versions.
///
/// Entities are matched by their stable UUIDs:
/// - Nodes by `NodeTypeId`
/// - Edges by `EdgeTypeId`
/// - Properties by `PropertyId`
///
/// The diff is deterministic: same inputs always produce the same output.
pub fn compute_diff(old: &OntologyIR, new: &OntologyIR) -> OntologyDiff {
    let old_node_ids: HashSet<&str> = old.node_types.iter().map(|n| &*n.id).collect();
    let new_node_ids: HashSet<&str> = new.node_types.iter().map(|n| &*n.id).collect();
    let old_node_map: HashMap<&str, &NodeTypeDef> =
        old.node_types.iter().map(|n| (&*n.id, n)).collect();
    // --- Nodes ---
    let mut added_nodes = Vec::new();
    let mut removed_nodes = Vec::new();
    let mut modified_nodes = Vec::new();
    let mut total_properties_added: usize = 0;
    let mut total_properties_removed: usize = 0;

    // Added nodes
    for node in &new.node_types {
        if !old_node_ids.contains(&*node.id) {
            total_properties_added += node.properties.len();
            added_nodes.push(node.clone());
        }
    }

    // Removed nodes
    for node in &old.node_types {
        if !new_node_ids.contains(&*node.id) {
            total_properties_removed += node.properties.len();
            removed_nodes.push(node.clone());
        }
    }

    // Modified nodes (present in both)
    for new_node in &new.node_types {
        if let Some(old_node) = old_node_map.get(&*new_node.id) {
            let (changes, props_added, props_removed) = diff_node(old_node, new_node);
            total_properties_added += props_added;
            total_properties_removed += props_removed;
            if !changes.is_empty() {
                modified_nodes.push(NodeDiff {
                    node_id: new_node.id.clone(),
                    label: new_node.label.clone(),
                    changes,
                });
            }
        }
    }

    // --- Edges ---
    let old_edge_ids: HashSet<&str> = old.edge_types.iter().map(|e| &*e.id).collect();
    let new_edge_ids: HashSet<&str> = new.edge_types.iter().map(|e| &*e.id).collect();
    let old_edge_map: HashMap<&str, &EdgeTypeDef> =
        old.edge_types.iter().map(|e| (&*e.id, e)).collect();

    let mut added_edges = Vec::new();
    let mut removed_edges = Vec::new();
    let mut modified_edges = Vec::new();

    // Added edges
    for edge in &new.edge_types {
        if !old_edge_ids.contains(&*edge.id) {
            total_properties_added += edge.properties.len();
            added_edges.push(edge.clone());
        }
    }

    // Removed edges
    for edge in &old.edge_types {
        if !new_edge_ids.contains(&*edge.id) {
            total_properties_removed += edge.properties.len();
            removed_edges.push(edge.clone());
        }
    }

    // Modified edges (present in both)
    for new_edge in &new.edge_types {
        if let Some(old_edge) = old_edge_map.get(&*new_edge.id) {
            let (changes, props_added, props_removed) = diff_edge(old_edge, new_edge, old, new);
            total_properties_added += props_added;
            total_properties_removed += props_removed;
            if !changes.is_empty() {
                modified_edges.push(EdgeDiff {
                    edge_id: new_edge.id.clone(),
                    label: new_edge.label.clone(),
                    changes,
                });
            }
        }
    }

    let summary = DiffSummary {
        nodes_added: added_nodes.len(),
        nodes_removed: removed_nodes.len(),
        nodes_modified: modified_nodes.len(),
        edges_added: added_edges.len(),
        edges_removed: removed_edges.len(),
        edges_modified: modified_edges.len(),
        properties_added: total_properties_added,
        properties_removed: total_properties_removed,
        total_changes: added_nodes.len()
            + removed_nodes.len()
            + modified_nodes.len()
            + added_edges.len()
            + removed_edges.len()
            + modified_edges.len(),
    };

    OntologyDiff {
        added_nodes,
        removed_nodes,
        modified_nodes,
        added_edges,
        removed_edges,
        modified_edges,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Node diffing
// ---------------------------------------------------------------------------

/// Returns (changes, properties_added, properties_removed).
fn diff_node(old: &NodeTypeDef, new: &NodeTypeDef) -> (Vec<NodeChange>, usize, usize) {
    let mut changes = Vec::new();
    let mut props_added = 0;
    let mut props_removed = 0;

    if old.label != new.label {
        changes.push(NodeChange::LabelChanged {
            old: old.label.clone(),
            new: new.label.clone(),
        });
    }

    if old.description != new.description {
        changes.push(NodeChange::DescriptionChanged {
            old: old.description.clone(),
            new: new.description.clone(),
        });
    }

    // Property diffs
    let (prop_changes, pa, pr) = diff_properties(&old.properties, &new.properties);
    props_added += pa;
    props_removed += pr;
    changes.extend(prop_changes.into_iter().map(|c| match c {
        PropDiffResult::Added(p) => NodeChange::PropertyAdded { property: p },
        PropDiffResult::Removed(p) => NodeChange::PropertyRemoved { property: p },
        PropDiffResult::Modified(name, ch) => NodeChange::PropertyModified {
            property_name: name,
            changes: ch,
        },
    }));

    // Constraint diffs
    let old_constraint_ids: HashSet<&str> = old.constraints.iter().map(|c| &*c.id).collect();
    let new_constraint_ids: HashSet<&str> = new.constraints.iter().map(|c| &*c.id).collect();

    for c in &new.constraints {
        if !old_constraint_ids.contains(&*c.id) {
            changes.push(NodeChange::ConstraintAdded {
                constraint: format_constraint(c),
            });
        }
    }

    for c in &old.constraints {
        if !new_constraint_ids.contains(&*c.id) {
            changes.push(NodeChange::ConstraintRemoved {
                constraint: format_constraint(c),
            });
        }
    }

    // Check for modified constraints (same id, different content)
    for new_c in &new.constraints {
        if let Some(old_c) = old.constraints.iter().find(|c| c.id == new_c.id) {
            let old_json = serde_json::to_value(&old_c.constraint).ok();
            let new_json = serde_json::to_value(&new_c.constraint).ok();
            if old_json != new_json {
                changes.push(NodeChange::ConstraintRemoved {
                    constraint: format_constraint(old_c),
                });
                changes.push(NodeChange::ConstraintAdded {
                    constraint: format_constraint(new_c),
                });
            }
        }
    }

    (changes, props_added, props_removed)
}

// ---------------------------------------------------------------------------
// Edge diffing
// ---------------------------------------------------------------------------

/// Returns (changes, properties_added, properties_removed).
fn diff_edge(
    old: &EdgeTypeDef,
    new: &EdgeTypeDef,
    old_ont: &OntologyIR,
    new_ont: &OntologyIR,
) -> (Vec<EdgeChange>, usize, usize) {
    let mut changes = Vec::new();
    let mut props_added = 0;
    let mut props_removed = 0;

    if old.label != new.label {
        changes.push(EdgeChange::LabelChanged {
            old: old.label.clone(),
            new: new.label.clone(),
        });
    }

    if old.description != new.description {
        changes.push(EdgeChange::DescriptionChanged {
            old: old.description.clone(),
            new: new.description.clone(),
        });
    }

    if old.source_node_id != new.source_node_id {
        changes.push(EdgeChange::SourceChanged {
            old: resolve_node_label(old_ont, &old.source_node_id),
            new: resolve_node_label(new_ont, &new.source_node_id),
        });
    }

    if old.target_node_id != new.target_node_id {
        changes.push(EdgeChange::TargetChanged {
            old: resolve_node_label(old_ont, &old.target_node_id),
            new: resolve_node_label(new_ont, &new.target_node_id),
        });
    }

    if old.cardinality != new.cardinality {
        changes.push(EdgeChange::CardinalityChanged {
            old: old.cardinality,
            new: new.cardinality,
        });
    }

    // Property diffs
    let (prop_changes, pa, pr) = diff_properties(&old.properties, &new.properties);
    props_added += pa;
    props_removed += pr;
    changes.extend(prop_changes.into_iter().map(|c| match c {
        PropDiffResult::Added(p) => EdgeChange::PropertyAdded { property: p },
        PropDiffResult::Removed(p) => EdgeChange::PropertyRemoved { property: p },
        PropDiffResult::Modified(name, ch) => EdgeChange::PropertyModified {
            property_name: name,
            changes: ch,
        },
    }));

    (changes, props_added, props_removed)
}

// ---------------------------------------------------------------------------
// Property diffing (shared between nodes and edges)
// ---------------------------------------------------------------------------

enum PropDiffResult {
    Added(PropertyDef),
    Removed(PropertyDef),
    Modified(String, Vec<PropertyChange>),
}

/// Returns (changes, added_count, removed_count).
fn diff_properties(
    old_props: &[PropertyDef],
    new_props: &[PropertyDef],
) -> (Vec<PropDiffResult>, usize, usize) {
    let old_ids: HashSet<&str> = old_props.iter().map(|p| &*p.id).collect();
    let new_ids: HashSet<&str> = new_props.iter().map(|p| &*p.id).collect();
    let old_map: HashMap<&str, &PropertyDef> = old_props.iter().map(|p| (&*p.id, p)).collect();

    let mut results = Vec::new();
    let mut added = 0;
    let mut removed = 0;

    // Added properties
    for prop in new_props {
        if !old_ids.contains(&*prop.id) {
            added += 1;
            results.push(PropDiffResult::Added(prop.clone()));
        }
    }

    // Removed properties
    for prop in old_props {
        if !new_ids.contains(&*prop.id) {
            removed += 1;
            results.push(PropDiffResult::Removed(prop.clone()));
        }
    }

    // Modified properties
    for new_prop in new_props {
        if let Some(old_prop) = old_map.get(&*new_prop.id) {
            let changes = diff_single_property(old_prop, new_prop);
            if !changes.is_empty() {
                results.push(PropDiffResult::Modified(new_prop.name.clone(), changes));
            }
        }
    }

    (results, added, removed)
}

fn diff_single_property(old: &PropertyDef, new: &PropertyDef) -> Vec<PropertyChange> {
    let mut changes = Vec::new();

    if old.property_type != new.property_type {
        changes.push(PropertyChange::TypeChanged {
            old: format_property_type(&old.property_type),
            new: format_property_type(&new.property_type),
        });
    }

    if old.nullable != new.nullable {
        changes.push(PropertyChange::NullabilityChanged {
            old: old.nullable,
            new: new.nullable,
        });
    }

    if old.description != new.description {
        changes.push(PropertyChange::DescriptionChanged {
            old: old.description.clone(),
            new: new.description.clone(),
        });
    }

    if old.default_value != new.default_value {
        changes.push(PropertyChange::DefaultValueChanged {
            old: old.default_value.as_ref().map(|v| format!("{v:?}")),
            new: new.default_value.as_ref().map(|v| format!("{v:?}")),
        });
    }

    changes
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_property_type(pt: &PropertyType) -> String {
    match pt {
        PropertyType::Bool => "bool".to_string(),
        PropertyType::Int => "int".to_string(),
        PropertyType::Float => "float".to_string(),
        PropertyType::String => "string".to_string(),
        PropertyType::Date => "date".to_string(),
        PropertyType::DateTime => "datetime".to_string(),
        PropertyType::Duration => "duration".to_string(),
        PropertyType::Bytes => "bytes".to_string(),
        PropertyType::Map => "map".to_string(),
        PropertyType::List { element } => format!("list<{}>", format_property_type(element)),
    }
}

fn format_constraint(c: &ConstraintDef) -> String {
    match &c.constraint {
        NodeConstraint::Unique { property_ids } => {
            format!(
                "UNIQUE({})",
                property_ids
                    .iter()
                    .map(|p| &**p)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        NodeConstraint::Exists { property_id } => {
            format!("EXISTS({})", property_id)
        }
        NodeConstraint::NodeKey { property_ids } => {
            format!(
                "NODE_KEY({})",
                property_ids
                    .iter()
                    .map(|p| &**p)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

fn resolve_node_label(ontology: &OntologyIR, node_id: &NodeTypeId) -> String {
    ontology
        .node_label(node_id)
        .unwrap_or(&node_id.0)
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{property, test_ontology};

    #[test]
    fn test_no_changes() {
        let ont = test_ontology();
        let diff = compute_diff(&ont, &ont);

        assert!(diff.is_empty());
        assert!(diff.added_nodes.is_empty());
        assert!(diff.removed_nodes.is_empty());
        assert!(diff.modified_nodes.is_empty());
        assert!(diff.added_edges.is_empty());
        assert!(diff.removed_edges.is_empty());
        assert!(diff.modified_edges.is_empty());
        assert_eq!(diff.summary.total_changes, 0);
    }

    #[test]
    fn test_added_node() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Product".to_string(),
            description: Some("A product".to_string()),
            source_table: None,
            properties: vec![property("p10", "product_name")],
            constraints: vec![],
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.added_nodes.len(), 1);
        assert_eq!(diff.added_nodes[0].label, "Product");
        assert!(diff.removed_nodes.is_empty());
        assert_eq!(diff.summary.nodes_added, 1);
        assert_eq!(diff.summary.properties_added, 1);
        assert_eq!(diff.summary.total_changes, 1);
    }

    #[test]
    fn test_removed_node() {
        let old = test_ontology();
        let mut new = old.clone();
        // Remove Company (n2) and the edge referencing it
        new.node_types.retain(|n| n.id != "n2");
        new.edge_types.clear();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.removed_nodes.len(), 1);
        assert_eq!(diff.removed_nodes[0].label, "Company");
        assert_eq!(diff.removed_edges.len(), 1);
        assert_eq!(diff.summary.nodes_removed, 1);
        assert_eq!(diff.summary.edges_removed, 1);
    }

    #[test]
    fn test_modified_node_label() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].label = "Individual".to_string();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert_eq!(diff.modified_nodes[0].node_id, "n1");
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::LabelChanged {
                old,
                new,
            } if old == "Person" && new == "Individual"
        )));
    }

    #[test]
    fn test_added_property() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties.push(PropertyDef {
            id: "p_new".into(),
            name: "email".to_string(),
            property_type: PropertyType::String,
            nullable: true,
            default_value: None,
            description: Some("Email address".to_string()),
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::PropertyAdded { property } if property.name == "email"
        )));
        assert_eq!(diff.summary.properties_added, 1);
    }

    #[test]
    fn test_removed_property() {
        let old = test_ontology();
        let mut new = old.clone();
        // Remove "age" property (p2) from Person (n1)
        new.node_types[0].properties.retain(|p| p.id != "p2");
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::PropertyRemoved { property } if property.name == "age"
        )));
        assert_eq!(diff.summary.properties_removed, 1);
    }

    #[test]
    fn test_property_type_changed() {
        let old = test_ontology();
        let mut new = old.clone();
        // Change "age" from String to Int
        new.node_types[0].properties[1].property_type = PropertyType::Int;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::PropertyModified { property_name, changes }
            if property_name == "age" && changes.iter().any(|pc| matches!(
                pc,
                PropertyChange::TypeChanged { old, new }
                if old == "string" && new == "int"
            ))
        )));
    }

    #[test]
    fn test_added_edge() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types.push(EdgeTypeDef {
            id: "e2".into(),
            label: "KNOWS".to_string(),
            description: None,
            source_node_id: "n1".into(),
            target_node_id: "n1".into(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.added_edges.len(), 1);
        assert_eq!(diff.added_edges[0].label, "KNOWS");
        assert_eq!(diff.summary.edges_added, 1);
    }

    #[test]
    fn test_removed_edge() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types.clear();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.removed_edges.len(), 1);
        assert_eq!(diff.removed_edges[0].label, "WORKS_AT");
        assert_eq!(diff.summary.edges_removed, 1);
    }

    #[test]
    fn test_edge_cardinality_changed() {
        let old = test_ontology();
        let mut new = old.clone();
        new.edge_types[0].cardinality = Cardinality::ManyToMany;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_edges.len(), 1);
        assert!(diff.modified_edges[0].changes.iter().any(|c| matches!(
            c,
            EdgeChange::CardinalityChanged {
                old: Cardinality::ManyToOne,
                new: Cardinality::ManyToMany,
            }
        )));
    }

    #[test]
    fn test_complex_diff() {
        let old = test_ontology();
        let mut new = old.clone();

        // 1. Rename Person -> Individual
        new.node_types[0].label = "Individual".to_string();
        // 2. Add a property to Person
        new.node_types[0].properties.push(PropertyDef {
            id: "p_email".into(),
            name: "email".to_string(),
            property_type: PropertyType::String,
            nullable: true,
            default_value: None,
            description: None,
        });
        // 3. Remove Company (n2)
        new.node_types.retain(|n| n.id != "n2");
        // 4. Add Product (n3)
        new.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Product".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("p_prod", "product_name")],
            constraints: vec![],
        });
        // 5. Remove WORKS_AT edge
        new.edge_types.clear();
        // 6. Add SELLS edge
        new.edge_types.push(EdgeTypeDef {
            id: "e2".into(),
            label: "SELLS".to_string(),
            description: None,
            source_node_id: "n1".into(),
            target_node_id: "n3".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.added_nodes.len(), 1);
        assert_eq!(diff.removed_nodes.len(), 1);
        assert_eq!(diff.modified_nodes.len(), 1);
        assert_eq!(diff.added_edges.len(), 1);
        assert_eq!(diff.removed_edges.len(), 1);
        assert_eq!(diff.summary.total_changes, 5);
    }

    #[test]
    fn test_summary_counts() {
        let old = test_ontology();
        let mut new = old.clone();

        // Add a node with 2 properties
        new.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Product".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("p10", "product_name"), property("p11", "price")],
            constraints: vec![],
        });
        // Remove age property from Person
        new.node_types[0].properties.retain(|p| p.id != "p2");
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.summary.nodes_added, 1);
        assert_eq!(diff.summary.nodes_modified, 1);
        assert_eq!(diff.summary.properties_added, 2); // 2 from new node
        assert_eq!(diff.summary.properties_removed, 1); // age removed
    }

    #[test]
    fn test_description_changed() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].description = Some("A human being".to_string());
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::DescriptionChanged {
                old: Some(o),
                new: Some(n),
            } if o == "A person" && n == "A human being"
        )));
    }

    #[test]
    fn test_edge_source_target_changed() {
        let old = test_ontology();
        let mut new = old.clone();
        // Add n3 and redirect edge target from n2 -> n3
        new.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Department".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("p10", "dept_name")],
            constraints: vec![],
        });
        new.edge_types[0].target_node_id = "n3".into();
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_edges.len(), 1);
        assert!(diff.modified_edges[0].changes.iter().any(|c| matches!(
            c,
            EdgeChange::TargetChanged { old, new }
            if old == "Company" && new == "Department"
        )));
    }

    #[test]
    fn test_property_nullability_changed() {
        let old = test_ontology();
        let mut new = old.clone();
        new.node_types[0].properties[0].nullable = true;
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_nodes.len(), 1);
        assert!(diff.modified_nodes[0].changes.iter().any(|c| matches!(
            c,
            NodeChange::PropertyModified { property_name, changes }
            if property_name == "name" && changes.iter().any(|pc| matches!(
                pc,
                PropertyChange::NullabilityChanged { old: false, new: true }
            ))
        )));
    }

    #[test]
    fn test_edge_property_diff() {
        let old = test_ontology();
        let mut new = old.clone();
        // Add a property to the WORKS_AT edge
        new.edge_types[0].properties.push(PropertyDef {
            id: "ep2".into(),
            name: "role".to_string(),
            property_type: PropertyType::String,
            nullable: true,
            default_value: None,
            description: None,
        });
        new.rebuild_indices();

        let diff = compute_diff(&old, &new);

        assert_eq!(diff.modified_edges.len(), 1);
        assert!(diff.modified_edges[0].changes.iter().any(|c| matches!(
            c,
            EdgeChange::PropertyAdded { property } if property.name == "role"
        )));
        assert_eq!(diff.summary.properties_added, 1);
    }
}
