use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ontology_ir::*;

use super::{OntologyCommand, PropertyPatch};

// ---------------------------------------------------------------------------
// Reconcile — compare original ontology with LLM-refined output
// ---------------------------------------------------------------------------

/// Result of reconciling a refined ontology with the original.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    pub ontology: OntologyIR,
    pub batch: OntologyCommand,
    pub report: ReconcileReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Entities where the LLM preserved the id correctly
    pub preserved_ids: Vec<PreservedEntity>,
    /// Entities where id was missing and auto-generated
    pub generated_ids: Vec<GeneratedEntity>,
    /// Label/name fallback matches (id was lost but matched by label)
    pub uncertain_matches: Vec<UncertainMatch>,
    /// Entities present in original but missing from refined
    pub deleted_entities: Vec<DeletedEntity>,
    /// Overall confidence
    pub confidence: ReconcileConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservedEntity {
    pub id: String,
    pub label: String,
    pub entity_kind: EntityKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedEntity {
    pub id: String,
    pub label: String,
    pub entity_kind: EntityKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertainMatch {
    pub original_id: String,
    pub original_label: String,
    pub matched_label: String,
    pub match_reason: String,
    pub entity_kind: EntityKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedEntity {
    pub id: String,
    pub label: String,
    pub entity_kind: EntityKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Node,
    Edge,
    Property,
    Constraint,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileConfidence {
    High,
    Medium,
    Low,
}

/// User decision for an uncertain match during reconcile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDecision {
    /// The original entity id (from UncertainMatch.original_id)
    pub original_id: String,
    /// Accept = confirm match (keep original_id with new data).
    /// Reject = not a match (treat as new entity with generated id).
    pub accept: bool,
}

/// Apply user decisions to a reconciled ontology.
/// For rejected matches: revert the id assignment (generate new UUID).
/// For accepted matches: keep the original id (already assigned by reconcile).
/// Returns the finalized ontology.
pub fn apply_match_decisions(
    mut ontology: OntologyIR,
    decisions: &[MatchDecision],
    uncertain_matches: &[UncertainMatch],
) -> OntologyIR {
    for decision in decisions {
        if decision.accept {
            continue; // Already mapped to original_id by reconcile
        }
        // Rejected: find the entity and assign a new id
        if let Some(um) = uncertain_matches
            .iter()
            .find(|m| m.original_id == decision.original_id)
        {
            let new_uuid = uuid::Uuid::new_v4().to_string();
            match um.entity_kind {
                EntityKind::Node => {
                    let new_id: NodeTypeId = new_uuid.into();
                    if let Some(node) = ontology
                        .node_types
                        .iter_mut()
                        .find(|n| n.id == um.original_id)
                    {
                        let old_id = node.id.clone();
                        node.id = new_id.clone();
                        // Fix edge references
                        for edge in &mut ontology.edge_types {
                            if edge.source_node_id == old_id {
                                edge.source_node_id = new_id.clone();
                            }
                            if edge.target_node_id == old_id {
                                edge.target_node_id = new_id.clone();
                            }
                        }
                        // Fix index references
                        for index in &mut ontology.indexes {
                            match index {
                                IndexDef::Single { node_id, .. }
                                | IndexDef::Composite { node_id, .. }
                                | IndexDef::FullText { node_id, .. }
                                | IndexDef::Vector { node_id, .. } => {
                                    if *node_id == old_id {
                                        *node_id = new_id.clone();
                                    }
                                }
                            }
                        }
                    }
                }
                EntityKind::Edge => {
                    let new_id: EdgeTypeId = new_uuid.into();
                    if let Some(edge) = ontology
                        .edge_types
                        .iter_mut()
                        .find(|e| e.id == um.original_id)
                    {
                        edge.id = new_id;
                    }
                }
                EntityKind::Property => {
                    let new_id: PropertyId = new_uuid.into();
                    // Properties: find in any node or edge
                    let mut found = false;
                    for node in &mut ontology.node_types {
                        if let Some(prop) =
                            node.properties.iter_mut().find(|p| p.id == um.original_id)
                        {
                            prop.id = new_id.clone();
                            // Fix constraint references
                            for c in &mut node.constraints {
                                match &mut c.constraint {
                                    NodeConstraint::Unique { property_ids }
                                    | NodeConstraint::NodeKey { property_ids } => {
                                        for pid in property_ids.iter_mut() {
                                            if *pid == um.original_id {
                                                *pid = new_id.clone();
                                            }
                                        }
                                    }
                                    NodeConstraint::Exists { property_id } => {
                                        if *property_id == um.original_id {
                                            *property_id = new_id.clone();
                                        }
                                    }
                                }
                            }
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        for edge in &mut ontology.edge_types {
                            if let Some(prop) =
                                edge.properties.iter_mut().find(|p| p.id == um.original_id)
                            {
                                prop.id = new_id.clone();
                                break;
                            }
                        }
                    }
                }
                _ => {} // Constraint/Index — less common, skip for now
            }
        }
    }
    ontology.rebuild_indices();
    ontology
}

/// Reconcile a refined ontology against the original.
/// The `refined` ontology has already been normalized (IDs assigned).
/// This function:
/// 1. Matches entities between original and refined by id (exact match)
/// 2. Falls back to label/name matching for entities without matching ids
/// 3. Generates a Batch command representing the diff
/// 4. Reports confidence based on matching quality
pub fn reconcile_refined(original: &OntologyIR, mut refined: OntologyIR) -> ReconcileResult {
    let mut preserved = Vec::new();
    let mut generated = Vec::new();
    let mut uncertain = Vec::new();
    let mut deleted = Vec::new();
    let mut commands = Vec::new();

    // ---- Phase 1: Node matching ----
    let orig_node_ids: HashSet<&str> = original.node_types.iter().map(|n| &*n.id).collect();
    let orig_node_by_label: HashMap<&str, &NodeTypeDef> = original
        .node_types
        .iter()
        .map(|n| (n.label.as_str(), n))
        .collect();

    // Track which original nodes have been matched
    let mut matched_orig_node_ids: HashSet<NodeTypeId> = HashSet::new();
    // Map from refined node's old id to new (original) id, for edge source/target fixup
    let mut node_id_remap: HashMap<NodeTypeId, NodeTypeId> = HashMap::new();

    for node in &mut refined.node_types {
        if orig_node_ids.contains(&*node.id) {
            // Exact id match
            matched_orig_node_ids.insert(node.id.clone());
            preserved.push(PreservedEntity {
                id: node.id.to_string(),
                label: node.label.clone(),
                entity_kind: EntityKind::Node,
            });
        } else if let Some(orig_node) = orig_node_by_label.get(node.label.as_str()) {
            // Label fallback match
            uncertain.push(UncertainMatch {
                original_id: orig_node.id.to_string(),
                original_label: orig_node.label.clone(),
                matched_label: node.label.clone(),
                match_reason: "matched by label".to_string(),
                entity_kind: EntityKind::Node,
            });
            matched_orig_node_ids.insert(orig_node.id.clone());
            let old_id = node.id.clone();
            node.id = orig_node.id.clone();
            node_id_remap.insert(old_id, orig_node.id.clone());
        } else {
            // New node added by LLM
            generated.push(GeneratedEntity {
                id: node.id.to_string(),
                label: node.label.clone(),
                entity_kind: EntityKind::Node,
            });
        }
    }

    // Deleted nodes (in original but not matched)
    for orig_node in &original.node_types {
        if !matched_orig_node_ids.contains(&orig_node.id) {
            deleted.push(DeletedEntity {
                id: orig_node.id.to_string(),
                label: orig_node.label.clone(),
                entity_kind: EntityKind::Node,
            });
        }
    }

    // ---- Phase 2: Fix edge source/target node ids using remap ----
    for edge in &mut refined.edge_types {
        if let Some(new_id) = node_id_remap.get(&edge.source_node_id) {
            edge.source_node_id = new_id.clone();
        }
        if let Some(new_id) = node_id_remap.get(&edge.target_node_id) {
            edge.target_node_id = new_id.clone();
        }
    }

    // ---- Phase 3: Edge matching ----
    let orig_edge_ids: HashSet<&str> = original.edge_types.iter().map(|e| &*e.id).collect();
    let orig_edge_by_sig: HashMap<(&str, &str, &str), &EdgeTypeDef> = original
        .edge_types
        .iter()
        .map(|e| {
            (
                (
                    e.label.as_str(),
                    &*e.source_node_id,
                    &*e.target_node_id,
                ),
                e,
            )
        })
        .collect();

    let mut matched_orig_edge_ids: HashSet<EdgeTypeId> = HashSet::new();

    for edge in &mut refined.edge_types {
        if orig_edge_ids.contains(&*edge.id) {
            matched_orig_edge_ids.insert(edge.id.clone());
            preserved.push(PreservedEntity {
                id: edge.id.to_string(),
                label: edge.label.clone(),
                entity_kind: EntityKind::Edge,
            });
        } else if let Some(orig_edge) = orig_edge_by_sig.get(&(
            edge.label.as_str(),
            &*edge.source_node_id,
            &*edge.target_node_id,
        )) {
            uncertain.push(UncertainMatch {
                original_id: orig_edge.id.to_string(),
                original_label: orig_edge.label.clone(),
                matched_label: edge.label.clone(),
                match_reason: "matched by label+source+target".to_string(),
                entity_kind: EntityKind::Edge,
            });
            matched_orig_edge_ids.insert(orig_edge.id.clone());
            edge.id = orig_edge.id.clone();
        } else {
            generated.push(GeneratedEntity {
                id: edge.id.to_string(),
                label: edge.label.clone(),
                entity_kind: EntityKind::Edge,
            });
        }
    }

    for orig_edge in &original.edge_types {
        if !matched_orig_edge_ids.contains(&orig_edge.id) {
            deleted.push(DeletedEntity {
                id: orig_edge.id.to_string(),
                label: orig_edge.label.clone(),
                entity_kind: EntityKind::Edge,
            });
        }
    }

    // ---- Phase 4: Property matching within matched nodes/edges ----
    {
        let orig_nodes: Vec<(&str, &Vec<PropertyDef>)> = original
            .node_types
            .iter()
            .map(|n| (&*n.id as &str, &n.properties))
            .collect();
        let mut ref_nodes: Vec<(&str, &mut Vec<PropertyDef>)> = refined
            .node_types
            .iter_mut()
            .map(|n| (&*n.id as &str, &mut n.properties))
            .collect();
        reconcile_properties_for_owners(
            &orig_nodes,
            &mut ref_nodes,
            EntityKind::Property,
            &mut preserved,
            &mut generated,
            &mut uncertain,
        );
    }

    {
        let orig_edges: Vec<(&str, &Vec<PropertyDef>)> = original
            .edge_types
            .iter()
            .map(|e| (&*e.id as &str, &e.properties))
            .collect();
        let mut ref_edges: Vec<(&str, &mut Vec<PropertyDef>)> = refined
            .edge_types
            .iter_mut()
            .map(|e| (&*e.id as &str, &mut e.properties))
            .collect();
        reconcile_properties_for_owners(
            &orig_edges,
            &mut ref_edges,
            EntityKind::Property,
            &mut preserved,
            &mut generated,
            &mut uncertain,
        );
    }

    // ---- Phase 5: Build diff commands ----

    // Deleted nodes
    for del in &deleted {
        match del.entity_kind {
            EntityKind::Node => {
                commands.push(OntologyCommand::DeleteNode {
                    node_id: del.id.clone().into(),
                });
            }
            EntityKind::Edge => {
                commands.push(OntologyCommand::DeleteEdge {
                    edge_id: del.id.clone().into(),
                });
            }
            _ => {}
        }
    }

    // Added/modified nodes
    for ref_node in &refined.node_types {
        if let Some(orig_node) = original.node_by_id(&ref_node.id) {
            // Existing node — check for modifications
            if orig_node.label != ref_node.label {
                commands.push(OntologyCommand::RenameNode {
                    node_id: ref_node.id.clone(),
                    new_label: ref_node.label.clone(),
                });
            }
            if orig_node.description != ref_node.description {
                commands.push(OntologyCommand::UpdateNodeDescription {
                    node_id: ref_node.id.clone(),
                    description: ref_node.description.clone(),
                });
            }
            // Property diffs
            diff_properties(
                &orig_node.properties,
                &ref_node.properties,
                &ref_node.id,
                &mut commands,
            );
            // Constraint diffs
            diff_constraints(
                &orig_node.constraints,
                &ref_node.constraints,
                &ref_node.id,
                &mut commands,
            );
        } else {
            // New node
            commands.push(OntologyCommand::AddNode {
                id: ref_node.id.clone(),
                label: ref_node.label.clone(),
                description: ref_node.description.clone(),
                source_table: ref_node.source_table.clone(),
            });
            for prop in &ref_node.properties {
                commands.push(OntologyCommand::AddProperty {
                    owner_id: ref_node.id.to_string(),
                    property: prop.clone(),
                });
            }
            for constraint in &ref_node.constraints {
                commands.push(OntologyCommand::AddConstraint {
                    node_id: ref_node.id.clone(),
                    constraint: constraint.clone(),
                });
            }
        }
    }

    // Added/modified edges
    for ref_edge in &refined.edge_types {
        if let Some(orig_edge) = original.edge_by_id(&ref_edge.id) {
            if orig_edge.label != ref_edge.label {
                commands.push(OntologyCommand::RenameEdge {
                    edge_id: ref_edge.id.clone(),
                    new_label: ref_edge.label.clone(),
                });
            }
            if orig_edge.cardinality != ref_edge.cardinality {
                commands.push(OntologyCommand::UpdateEdgeCardinality {
                    edge_id: ref_edge.id.clone(),
                    cardinality: ref_edge.cardinality,
                });
            }
            if orig_edge.description != ref_edge.description {
                commands.push(OntologyCommand::UpdateEdgeDescription {
                    edge_id: ref_edge.id.clone(),
                    description: ref_edge.description.clone(),
                });
            }
            diff_properties(
                &orig_edge.properties,
                &ref_edge.properties,
                &ref_edge.id,
                &mut commands,
            );
        } else {
            commands.push(OntologyCommand::AddEdge {
                id: ref_edge.id.clone(),
                label: ref_edge.label.clone(),
                source_node_id: ref_edge.source_node_id.clone(),
                target_node_id: ref_edge.target_node_id.clone(),
                cardinality: ref_edge.cardinality,
            });
            for prop in &ref_edge.properties {
                commands.push(OntologyCommand::AddProperty {
                    owner_id: ref_edge.id.to_string(),
                    property: prop.clone(),
                });
            }
        }
    }

    // Index diffs
    diff_indexes(&original.indexes, &refined.indexes, &mut commands);

    let batch = OntologyCommand::Batch {
        description: "LLM refine reconciliation".to_string(),
        commands,
    };

    // ---- Phase 6: Confidence ----
    let uncertain_count = uncertain.len();
    let deleted_count = deleted.len();
    let confidence = if uncertain_count == 0 && deleted_count == 0 {
        ReconcileConfidence::High
    } else if uncertain_count <= 3 && deleted_count <= 3 {
        ReconcileConfidence::Medium
    } else {
        ReconcileConfidence::Low
    };

    // Rebuild indices after ID remapping
    refined.rebuild_indices();

    ReconcileResult {
        ontology: refined,
        batch,
        report: ReconcileReport {
            preserved_ids: preserved,
            generated_ids: generated,
            uncertain_matches: uncertain,
            deleted_entities: deleted,
            confidence,
        },
    }
}

/// Match properties within entities that share the same id.
fn reconcile_properties_for_owners(
    originals: &[(&str, &Vec<PropertyDef>)],
    refineds: &mut [(&str, &mut Vec<PropertyDef>)],
    kind: EntityKind,
    preserved: &mut Vec<PreservedEntity>,
    generated: &mut Vec<GeneratedEntity>,
    uncertain: &mut Vec<UncertainMatch>,
) {
    let orig_map: HashMap<&str, &Vec<PropertyDef>> = originals.iter().copied().collect();

    for (owner_id, ref_props) in refineds.iter_mut() {
        let Some(orig_props) = orig_map.get(*owner_id) else {
            // New owner — all properties are generated (already tracked at entity level)
            continue;
        };

        let orig_prop_ids: HashSet<&str> = orig_props.iter().map(|p| &*p.id).collect();
        let orig_prop_by_name: HashMap<&str, &PropertyDef> =
            orig_props.iter().map(|p| (p.name.as_str(), p)).collect();

        for prop in ref_props.iter_mut() {
            if orig_prop_ids.contains(&*prop.id) {
                preserved.push(PreservedEntity {
                    id: prop.id.to_string(),
                    label: prop.name.clone(),
                    entity_kind: kind,
                });
            } else if let Some(orig_prop) = orig_prop_by_name.get(prop.name.as_str()) {
                uncertain.push(UncertainMatch {
                    original_id: orig_prop.id.to_string(),
                    original_label: orig_prop.name.clone(),
                    matched_label: prop.name.clone(),
                    match_reason: "matched by property name".to_string(),
                    entity_kind: kind,
                });
                prop.id = orig_prop.id.clone();
            } else {
                generated.push(GeneratedEntity {
                    id: prop.id.to_string(),
                    label: prop.name.clone(),
                    entity_kind: kind,
                });
            }
        }
    }
}

/// Generate diff commands for properties between original and refined.
fn diff_properties(
    orig_props: &[PropertyDef],
    ref_props: &[PropertyDef],
    owner_id: &str,
    commands: &mut Vec<OntologyCommand>,
) {
    let ref_prop_ids: HashSet<&str> = ref_props.iter().map(|p| &*p.id).collect();

    // Deleted properties
    for orig_prop in orig_props {
        if !ref_prop_ids.contains(&*orig_prop.id) {
            commands.push(OntologyCommand::DeleteProperty {
                owner_id: owner_id.to_string(),
                property_id: orig_prop.id.clone(),
            });
        }
    }

    // Added or modified properties
    for ref_prop in ref_props {
        if let Some(orig_prop) = orig_props.iter().find(|p| p.id == ref_prop.id) {
            // Check for modifications
            let mut patch = PropertyPatch {
                name: None,
                property_type: None,
                nullable: None,
                default_value: None,
                description: None,
            };
            let mut has_changes = false;

            if orig_prop.name != ref_prop.name {
                patch.name = Some(ref_prop.name.clone());
                has_changes = true;
            }
            if orig_prop.property_type != ref_prop.property_type {
                patch.property_type = Some(ref_prop.property_type.clone());
                has_changes = true;
            }
            if orig_prop.nullable != ref_prop.nullable {
                patch.nullable = Some(ref_prop.nullable);
                has_changes = true;
            }
            if orig_prop.default_value != ref_prop.default_value {
                patch.default_value = Some(ref_prop.default_value.clone());
                has_changes = true;
            }
            if orig_prop.description != ref_prop.description {
                patch.description = Some(ref_prop.description.clone());
                has_changes = true;
            }

            if has_changes {
                commands.push(OntologyCommand::UpdateProperty {
                    owner_id: owner_id.to_string(),
                    property_id: ref_prop.id.clone(),
                    patch,
                });
            }
        } else {
            commands.push(OntologyCommand::AddProperty {
                owner_id: owner_id.to_string(),
                property: ref_prop.clone(),
            });
        }
    }
}

/// Generate diff commands for constraints.
/// Detects additions, removals, and modifications (same id but different content).
fn diff_constraints(
    orig_constraints: &[ConstraintDef],
    ref_constraints: &[ConstraintDef],
    node_id: &NodeTypeId,
    commands: &mut Vec<OntologyCommand>,
) {
    let orig_map: HashMap<&str, &ConstraintDef> = orig_constraints
        .iter()
        .map(|c| (&*c.id as &str, c))
        .collect();
    let ref_map: HashMap<&str, &ConstraintDef> =
        ref_constraints.iter().map(|c| (&*c.id as &str, c)).collect();

    // Removed constraints
    for orig in orig_constraints {
        if !ref_map.contains_key(&*orig.id) {
            commands.push(OntologyCommand::RemoveConstraint {
                node_id: node_id.clone(),
                constraint_id: orig.id.clone(),
            });
        }
    }

    for ref_c in ref_constraints {
        match orig_map.get(&*ref_c.id) {
            None => {
                // Added constraint
                commands.push(OntologyCommand::AddConstraint {
                    node_id: node_id.clone(),
                    constraint: ref_c.clone(),
                });
            }
            Some(orig_c) => {
                // Modified constraint (same id, different content) -> remove + add
                let orig_json = serde_json::to_value(&orig_c.constraint).ok();
                let ref_json = serde_json::to_value(&ref_c.constraint).ok();
                if orig_json != ref_json {
                    commands.push(OntologyCommand::RemoveConstraint {
                        node_id: node_id.clone(),
                        constraint_id: orig_c.id.clone(),
                    });
                    commands.push(OntologyCommand::AddConstraint {
                        node_id: node_id.clone(),
                        constraint: ref_c.clone(),
                    });
                }
            }
        }
    }
}

/// Generate diff commands for indexes.
fn diff_indexes(
    orig_indexes: &[IndexDef],
    ref_indexes: &[IndexDef],
    commands: &mut Vec<OntologyCommand>,
) {
    let ref_ids: HashSet<&str> = ref_indexes.iter().map(index_id).collect();

    for orig in orig_indexes {
        if !ref_ids.contains(index_id(orig)) {
            commands.push(OntologyCommand::RemoveIndex {
                index_id: index_id(orig).to_string(),
            });
        }
    }

    let orig_ids: HashSet<&str> = orig_indexes.iter().map(index_id).collect();
    for ref_i in ref_indexes {
        if !orig_ids.contains(index_id(ref_i)) {
            commands.push(OntologyCommand::AddIndex {
                index: ref_i.clone(),
            });
        }
    }
}

use super::index_id;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{ontologies_equal, property, test_ontology};

    #[test]
    fn reconcile_preserves_ids_when_llm_keeps_them() {
        let original = test_ontology();
        let refined = original.clone();

        let result = reconcile_refined(&original, refined);

        // All entities preserved
        assert!(!result.report.preserved_ids.is_empty());
        assert!(result.report.uncertain_matches.is_empty());
        assert!(result.report.deleted_entities.is_empty());
        assert!(result.report.generated_ids.is_empty());
        assert_eq!(result.report.confidence, ReconcileConfidence::High);

        // Batch should have no commands (no changes)
        if let OntologyCommand::Batch { commands, .. } = &result.batch {
            assert!(
                commands.is_empty(),
                "expected empty batch, got {} commands",
                commands.len()
            );
        } else {
            panic!("expected Batch command");
        }

        // Ontology should be identical
        assert!(ontologies_equal(&original, &result.ontology));
    }

    #[test]
    fn reconcile_fallback_matches_by_label() {
        let original = test_ontology();

        // Create refined with same labels but different ids
        let mut refined = original.clone();
        refined.node_types[0].id = "new-n1".into();
        refined.node_types[0].properties[0].id = "new-p1".into();
        refined.node_types[0].properties[1].id = "new-p2".into();
        refined.node_types[0].constraints[0] = ConstraintDef {
            id: "new-c1".into(),
            constraint: NodeConstraint::Unique {
                property_ids: vec!["new-p1".into()],
            },
        };
        refined.node_types[1].id = "new-n2".into();
        refined.node_types[1].properties[0].id = "new-p3".into();
        // Edge: source/target still reference old ids, but since nodes were remapped
        // we need to reference the new ids that will be fixed up
        refined.edge_types[0].id = "new-e1".into();
        refined.edge_types[0].source_node_id = "new-n1".into();
        refined.edge_types[0].target_node_id = "new-n2".into();
        refined.edge_types[0].properties[0].id = "new-ep1".into();

        let result = reconcile_refined(&original, refined);

        // Should have uncertain matches for nodes and properties
        assert!(!result.report.uncertain_matches.is_empty());
        assert!(result.report.deleted_entities.is_empty());
        assert!(result.report.generated_ids.is_empty());

        // Node ids should be restored to originals
        assert!(result.ontology.node_by_id("n1").is_some());
        assert!(result.ontology.node_by_id("n2").is_some());
        assert!(result.ontology.node_by_id("new-n1").is_none());

        // Edge source/target should be remapped to original node ids
        let edge = &result.ontology.edge_types[0];
        assert_eq!(edge.source_node_id, "n1");
        assert_eq!(edge.target_node_id, "n2");

        // Property ids should be restored
        let person = result.ontology.node_by_id("n1").unwrap();
        assert!(person.properties.iter().any(|p| p.id == "p1"));
        assert!(person.properties.iter().any(|p| p.id == "p2"));

        // Confidence should be Medium or Low (has uncertain matches)
        assert_ne!(result.report.confidence, ReconcileConfidence::High);
    }

    #[test]
    fn reconcile_detects_additions_and_deletions() {
        let original = test_ontology();

        // Create refined: remove Company (n2), add Product (n3)
        let mut refined = original.clone();
        refined.node_types.retain(|n| n.id != "n2");
        refined.node_types.push(NodeTypeDef {
            id: "n3".into(),
            label: "Product".to_string(),
            description: Some("A product".to_string()),
            source_table: None,
            properties: vec![property("p4", "product_name")],
            constraints: vec![],
        });
        // Remove the edge that referenced n2
        refined.edge_types.clear();

        let result = reconcile_refined(&original, refined);

        // n2 should be deleted
        assert!(
            result
                .report
                .deleted_entities
                .iter()
                .any(|d| d.id == "n2" && d.label == "Company")
        );

        // e1 should be deleted (was connected to n2)
        assert!(
            result
                .report
                .deleted_entities
                .iter()
                .any(|d| d.id == "e1" && d.label == "WORKS_AT")
        );

        // n3 should be generated
        assert!(
            result
                .report
                .generated_ids
                .iter()
                .any(|g| g.id == "n3" && g.label == "Product")
        );

        // n1 should be preserved
        assert!(result.report.preserved_ids.iter().any(|p| p.id == "n1"));

        // Ontology should contain n1 and n3 but not n2
        assert!(result.ontology.node_by_id("n1").is_some());
        assert!(result.ontology.node_by_id("n3").is_some());
        assert!(result.ontology.node_by_id("n2").is_none());

        // Confidence should not be High (deletions exist)
        assert_ne!(result.report.confidence, ReconcileConfidence::High);
    }
}
