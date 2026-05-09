use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Location of a property owner (node or edge) by index.
enum OwnerLocation {
    Node(usize),
    Edge(usize),
}

/// Find the index-based location of a property owner.
fn find_owner_location(
    ontology: &OntologyIR,
    owner: &PropertyOwner,
) -> Result<OwnerLocation, String> {
    match owner {
        PropertyOwner::Node { type_id } => ontology
            .node_types
            .iter()
            .position(|n| n.id == *type_id)
            .map(OwnerLocation::Node)
            .ok_or_else(|| format!("node '{}' not found", type_id)),
        PropertyOwner::Edge { type_id } => ontology
            .edge_types
            .iter()
            .position(|e| e.id == *type_id)
            .map(OwnerLocation::Edge)
            .ok_or_else(|| format!("edge '{}' not found", type_id)),
    }
}

/// Get mutable reference to the property list of an owner by location.
fn owner_properties_mut<'a>(
    ontology: &'a mut OntologyIR,
    loc: &OwnerLocation,
) -> &'a mut Vec<PropertyDef> {
    match loc {
        OwnerLocation::Node(idx) => &mut ontology.node_types[*idx].properties,
        OwnerLocation::Edge(idx) => &mut ontology.edge_types[*idx].properties,
    }
}

/// Returns the index of a node by id, or error.
fn node_index(ontology: &OntologyIR, node_id: &str) -> Result<usize, String> {
    ontology
        .node_types
        .iter()
        .position(|n| n.id == node_id)
        .ok_or_else(|| format!("node '{}' not found", node_id))
}

/// Returns the index of an edge by id, or error.
fn edge_index(ontology: &OntologyIR, edge_id: &str) -> Result<usize, String> {
    ontology
        .edge_types
        .iter()
        .position(|e| e.id == edge_id)
        .ok_or_else(|| format!("edge '{}' not found", edge_id))
}

use super::index_id;

/// Extract the node_id from an IndexDef.
fn index_node_id(index: &IndexDef) -> &str {
    match index {
        IndexDef::Single { node_id, .. }
        | IndexDef::Composite { node_id, .. }
        | IndexDef::FullText { node_id, .. }
        | IndexDef::Vector { node_id, .. } => node_id,
    }
}

/// Collect all property_ids referenced by an IndexDef.
fn index_property_ids(index: &IndexDef) -> Vec<&str> {
    match index {
        IndexDef::Single { property_id, .. } | IndexDef::Vector { property_id, .. } => {
            vec![&**property_id]
        }
        IndexDef::Composite { property_ids, .. } | IndexDef::FullText { property_ids, .. } => {
            property_ids.iter().map(|s| &**s).collect()
        }
    }
}

/// Collect all property_ids referenced by a ConstraintDef.
fn constraint_property_ids(constraint: &ConstraintDef) -> Vec<&str> {
    match &constraint.constraint {
        NodeConstraint::Unique { property_ids } | NodeConstraint::NodeKey { property_ids } => {
            property_ids.iter().map(|s| &**s).collect()
        }
        NodeConstraint::Exists { property_id } => vec![&**property_id],
    }
}

// ---------------------------------------------------------------------------
// Execute inner
// ---------------------------------------------------------------------------

impl OntologyCommand {
    /// Inner execution — no validation or index rebuild.
    /// Used by Batch to avoid per-sub-command overhead.
    pub(super) fn execute_inner(&self, mut ont: OntologyIR) -> Result<CommandResult, String> {
        match self {
            // ----- AddNode -----
            OntologyCommand::AddNode {
                id,
                label,
                description,
            } => {
                if ont.node_types.iter().any(|n| n.id == *id) {
                    return Err(format!("node with id '{}' already exists", id));
                }
                if ont.node_types.iter().any(|n| n.label == *label) {
                    return Err(format!("node with label '{}' already exists", label));
                }
                ont.node_types.push(NodeTypeDef {
                    id: id.clone(),
                    label: label.clone(),
                    description: description.clone(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                });
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteNode {
                        node_id: id.clone(),
                    },
                })
            }

            // ----- CreateNodeType -----
            OntologyCommand::CreateNodeType { node } => {
                if ont.node_types.iter().any(|n| n.id == node.id) {
                    return Err(format!("node with id '{}' already exists", node.id));
                }
                if ont.node_types.iter().any(|n| n.label == node.label) {
                    return Err(format!("node with label '{}' already exists", node.label));
                }
                let node_id = node.id.clone();
                ont.node_types.push((**node).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteNode { node_id },
                })
            }

            // ----- DeleteNode -----
            OntologyCommand::DeleteNode { node_id } => {
                let idx = node_index(&ont, node_id)?;
                let node = ont.node_types.remove(idx);

                // Collect edges referencing this node
                let mut removed_edges = Vec::new();
                ont.edge_types.retain(|e| {
                    if e.source_node_id == *node_id || e.target_node_id == *node_id {
                        removed_edges.push(e.clone());
                        false
                    } else {
                        true
                    }
                });

                let removed_edge_ids = removed_edges
                    .iter()
                    .map(|edge| edge.id.as_str())
                    .collect::<std::collections::HashSet<_>>();
                let mut removed_link_mappings = Vec::new();
                ont.link_mappings.retain(|mapping| {
                    if removed_edge_ids.contains(mapping.edge_type_id.as_str()) {
                        removed_link_mappings.push(mapping.clone());
                        false
                    } else {
                        true
                    }
                });

                // Collect indexes referencing this node
                let mut removed_indexes = Vec::new();
                ont.indexes.retain(|idx| {
                    if *node_id == *index_node_id(idx) {
                        removed_indexes.push(idx.clone());
                        false
                    } else {
                        true
                    }
                });

                // Collect object mappings bound to this node.
                let mut removed_object_mappings = Vec::new();
                ont.object_mappings.retain(|mapping| {
                    if mapping.node_type_id == *node_id {
                        removed_object_mappings.push(mapping.clone());
                        false
                    } else {
                        true
                    }
                });

                // Build inverse batch: re-add full node/edge definitions + mappings/indexes.
                let mut inverse_commands = Vec::new();

                inverse_commands.push(OntologyCommand::CreateNodeType {
                    node: Box::new(node.clone()),
                });

                for edge in &removed_edges {
                    inverse_commands.push(OntologyCommand::CreateEdgeType {
                        edge: Box::new(edge.clone()),
                    });
                }

                // Re-add indexes
                for index in &removed_indexes {
                    inverse_commands.push(OntologyCommand::AddIndex {
                        index: index.clone(),
                    });
                }

                // Re-add object mappings after the node and its properties exist.
                for mapping in &removed_object_mappings {
                    inverse_commands.push(OntologyCommand::CreateObjectMapping {
                        mapping: Box::new(mapping.clone()),
                    });
                }
                for mapping in &removed_link_mappings {
                    inverse_commands.push(OntologyCommand::CreateLinkMapping {
                        mapping: Box::new(mapping.clone()),
                    });
                }

                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::Batch {
                        description: format!("restore deleted node '{}'", node.label),
                        commands: inverse_commands,
                    },
                })
            }

            // ----- RenameNode -----
            OntologyCommand::RenameNode { node_id, new_label } => {
                let idx = node_index(&ont, node_id)?;
                // Check label collision
                if ont
                    .node_types
                    .iter()
                    .any(|n| n.label == *new_label && n.id != *node_id)
                {
                    return Err(format!(
                        "Cannot rename node '{}': label '{}' is already in use",
                        node_id, new_label
                    ));
                }
                let old_label = ont.node_types[idx].label.clone();
                ont.node_types[idx].label = new_label.clone();
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::RenameNode {
                        node_id: node_id.clone(),
                        new_label: old_label,
                    },
                })
            }

            // ----- UpdateNodeDescription -----
            OntologyCommand::UpdateNodeDescription {
                node_id,
                description,
            } => {
                let idx = node_index(&ont, node_id)?;
                let old_desc = ont.node_types[idx].description.clone();
                ont.node_types[idx].description = description.clone();
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateNodeDescription {
                        node_id: node_id.clone(),
                        description: old_desc,
                    },
                })
            }

            // ----- AddEdge -----
            OntologyCommand::AddEdge {
                id,
                label,
                source_node_id,
                target_node_id,
                cardinality,
            } => {
                // Validate endpoints exist
                if ont.node_types.iter().all(|n| n.id != *source_node_id) {
                    return Err(format!(
                        "source node '{}' not found for edge",
                        source_node_id
                    ));
                }
                if ont.node_types.iter().all(|n| n.id != *target_node_id) {
                    return Err(format!(
                        "target node '{}' not found for edge",
                        target_node_id
                    ));
                }
                if ont.edge_types.iter().any(|e| e.id == *id) {
                    return Err(format!("edge with id '{}' already exists", id));
                }
                // Check (label, source, target) uniqueness
                if ont.edge_types.iter().any(|e| {
                    e.label == *label
                        && e.source_node_id == *source_node_id
                        && e.target_node_id == *target_node_id
                }) {
                    return Err(format!(
                        "edge '{}' between '{}' and '{}' already exists",
                        label, source_node_id, target_node_id
                    ));
                }
                ont.edge_types.push(EdgeTypeDef {
                    id: id.clone(),
                    label: label.clone(),
                    description: LocalizedText::default(),
                    source_node_id: source_node_id.clone(),
                    target_node_id: target_node_id.clone(),
                    properties: vec![],
                    cardinality: *cardinality,
                    ..Default::default()
                });
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteEdge {
                        edge_id: id.clone(),
                    },
                })
            }

            // ----- CreateEdgeType -----
            OntologyCommand::CreateEdgeType { edge } => {
                if ont.node_types.iter().all(|n| n.id != edge.source_node_id) {
                    return Err(format!(
                        "source node '{}' not found for edge",
                        edge.source_node_id
                    ));
                }
                if ont.node_types.iter().all(|n| n.id != edge.target_node_id) {
                    return Err(format!(
                        "target node '{}' not found for edge",
                        edge.target_node_id
                    ));
                }
                if ont.edge_types.iter().any(|e| e.id == edge.id) {
                    return Err(format!("edge with id '{}' already exists", edge.id));
                }
                if ont.edge_types.iter().any(|e| {
                    e.label == edge.label
                        && e.source_node_id == edge.source_node_id
                        && e.target_node_id == edge.target_node_id
                }) {
                    return Err(format!(
                        "edge '{}' between '{}' and '{}' already exists",
                        edge.label, edge.source_node_id, edge.target_node_id
                    ));
                }
                let edge_id = edge.id.clone();
                ont.edge_types.push((**edge).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteEdge { edge_id },
                })
            }

            // ----- DeleteEdge -----
            OntologyCommand::DeleteEdge { edge_id } => {
                let idx = edge_index(&ont, edge_id)?;
                let edge = ont.edge_types.remove(idx);

                let mut removed_link_mappings = Vec::new();
                ont.link_mappings.retain(|mapping| {
                    if mapping.edge_type_id == *edge_id {
                        removed_link_mappings.push(mapping.clone());
                        false
                    } else {
                        true
                    }
                });

                let mut inverse_cmds = vec![OntologyCommand::CreateEdgeType {
                    edge: Box::new(edge.clone()),
                }];
                for mapping in &removed_link_mappings {
                    inverse_cmds.push(OntologyCommand::CreateLinkMapping {
                        mapping: Box::new(mapping.clone()),
                    });
                }

                let inverse = if inverse_cmds.len() == 1 {
                    inverse_cmds.remove(0)
                } else {
                    OntologyCommand::Batch {
                        description: format!("Restore deleted edge '{}'", edge.id),
                        commands: inverse_cmds,
                    }
                };

                Ok(CommandResult {
                    new_ontology: ont,
                    inverse,
                })
            }

            // ----- RenameEdge -----
            OntologyCommand::RenameEdge { edge_id, new_label } => {
                let idx = edge_index(&ont, edge_id)?;
                // Check (label, source, target) uniqueness
                let src = &ont.edge_types[idx].source_node_id;
                let tgt = &ont.edge_types[idx].target_node_id;
                if ont.edge_types.iter().any(|e| {
                    e.id != *edge_id
                        && e.label == *new_label
                        && e.source_node_id == *src
                        && e.target_node_id == *tgt
                }) {
                    return Err(format!(
                        "Cannot rename edge '{}': label '{}' with same endpoints already exists",
                        edge_id, new_label
                    ));
                }
                let old_label = ont.edge_types[idx].label.clone();
                ont.edge_types[idx].label = new_label.clone();
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::RenameEdge {
                        edge_id: edge_id.clone(),
                        new_label: old_label,
                    },
                })
            }

            // ----- UpdateEdgeCardinality -----
            OntologyCommand::UpdateEdgeCardinality {
                edge_id,
                cardinality,
            } => {
                let idx = edge_index(&ont, edge_id)?;
                let old_cardinality = ont.edge_types[idx].cardinality;
                ont.edge_types[idx].cardinality = *cardinality;
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateEdgeCardinality {
                        edge_id: edge_id.clone(),
                        cardinality: old_cardinality,
                    },
                })
            }

            // ----- UpdateEdgeDescription -----
            OntologyCommand::UpdateEdgeDescription {
                edge_id,
                description,
            } => {
                let idx = edge_index(&ont, edge_id)?;
                let old_desc = ont.edge_types[idx].description.clone();
                ont.edge_types[idx].description = description.clone();
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateEdgeDescription {
                        edge_id: edge_id.clone(),
                        description: old_desc,
                    },
                })
            }

            // ----- AddProperty -----
            OntologyCommand::AddProperty { owner, property } => {
                let loc = find_owner_location(&ont, owner)?;
                let props = owner_properties_mut(&mut ont, &loc);
                if props.iter().any(|p| p.id == property.id) {
                    return Err(format!(
                        "property '{}' already exists on owner '{}'",
                        property.id, owner
                    ));
                }
                props.push((**property).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteProperty {
                        owner: owner.clone(),
                        property_id: property.id.clone(),
                    },
                })
            }

            // ----- DeleteProperty -----
            OntologyCommand::DeleteProperty { owner, property_id } => {
                let loc = find_owner_location(&ont, owner)?;
                let removed_prop = {
                    let props = owner_properties_mut(&mut ont, &loc);
                    let prop_idx =
                        props
                            .iter()
                            .position(|p| p.id == *property_id)
                            .ok_or_else(|| {
                                format!("property '{}' not found on owner '{}'", property_id, owner)
                            })?;
                    props.remove(prop_idx)
                };

                // Remove constraints referencing this property (only on nodes)
                let mut removed_constraints = Vec::new();
                if let PropertyOwner::Node { type_id: node_id } = owner
                    && let Some(node) = ont.node_types.iter_mut().find(|n| n.id == *node_id)
                {
                    node.constraints.retain(|constraint| {
                        if constraint_property_ids(constraint).contains(&&**property_id) {
                            removed_constraints.push(constraint.clone());
                            false
                        } else {
                            true
                        }
                    });
                }

                // Remove indexes on this owner that reference this property
                let mut removed_indexes = Vec::new();
                ont.indexes.retain(|idx| {
                    if index_node_id(idx) == owner.as_str()
                        && index_property_ids(idx).contains(&&**property_id)
                    {
                        removed_indexes.push(idx.clone());
                        false
                    } else {
                        true
                    }
                });

                let mut restore_mapping_commands = Vec::new();
                if matches!(owner, PropertyOwner::Node { .. }) {
                    for mapping in &mut ont.object_mappings {
                        if mapping.node_type_id.as_str() != owner.as_str() {
                            continue;
                        }

                        let old_mapping = mapping.clone();
                        mapping
                            .property_mappings
                            .retain(|m| m.property_id != *property_id);
                        if mapping.property_mappings != old_mapping.property_mappings {
                            restore_mapping_commands.push(OntologyCommand::UpdateObjectMapping {
                                id: old_mapping.id.clone(),
                                mapping: Box::new(old_mapping),
                            });
                        }
                    }
                }

                let add_property = OntologyCommand::AddProperty {
                    owner: owner.clone(),
                    property: Box::new(removed_prop),
                };
                let inverse = if restore_mapping_commands.is_empty() {
                    if removed_constraints.is_empty() && removed_indexes.is_empty() {
                        add_property
                    } else {
                        let mut commands = Vec::with_capacity(
                            1 + removed_constraints.len() + removed_indexes.len(),
                        );
                        commands.push(add_property);
                        if let PropertyOwner::Node { type_id: node_id } = owner {
                            for constraint in removed_constraints {
                                commands.push(OntologyCommand::AddConstraint {
                                    node_id: node_id.clone(),
                                    constraint,
                                });
                            }
                        }
                        for index in removed_indexes {
                            commands.push(OntologyCommand::AddIndex { index });
                        }
                        OntologyCommand::Batch {
                            description: format!(
                                "restore deleted property '{}' on '{}'",
                                property_id, owner
                            ),
                            commands,
                        }
                    }
                } else {
                    let mut commands = Vec::with_capacity(
                        1 + removed_constraints.len()
                            + removed_indexes.len()
                            + restore_mapping_commands.len(),
                    );
                    commands.push(add_property);
                    if let PropertyOwner::Node { type_id: node_id } = owner {
                        for constraint in removed_constraints {
                            commands.push(OntologyCommand::AddConstraint {
                                node_id: node_id.clone(),
                                constraint,
                            });
                        }
                    }
                    for index in removed_indexes {
                        commands.push(OntologyCommand::AddIndex { index });
                    }
                    commands.extend(restore_mapping_commands);
                    OntologyCommand::Batch {
                        description: format!(
                            "restore deleted property '{}' on '{}'",
                            property_id, owner
                        ),
                        commands,
                    }
                };

                Ok(CommandResult {
                    new_ontology: ont,
                    inverse,
                })
            }

            // ----- UpdateProperty -----
            OntologyCommand::UpdateProperty {
                owner,
                property_id,
                patch,
            } => {
                let loc = find_owner_location(&ont, owner)?;
                let props = owner_properties_mut(&mut ont, &loc);
                let prop = props
                    .iter_mut()
                    .find(|p| p.id == *property_id)
                    .ok_or_else(|| {
                        format!("property '{}' not found on owner '{}'", property_id, owner)
                    })?;

                // Build reverse patch from current values before applying
                let reverse_patch = PropertyPatch {
                    name: patch.name.as_ref().map(|_| prop.name.to_string()),
                    property_type: patch
                        .property_type
                        .as_ref()
                        .map(|_| prop.property_type.clone()),
                    nullable: patch.nullable.map(|_| prop.nullable),
                    default_value: patch
                        .default_value
                        .as_ref()
                        .map(|_| prop.default_value.clone()),
                    description: patch.description.as_ref().map(|_| prop.description.clone()),
                };

                // Apply patch. Name updates flow through
                // `PropertyKey::new` so an invalid user-supplied name
                // is rejected here rather than slipping into the IR.
                if let Some(name) = &patch.name {
                    prop.name = ox_core::property_key::PropertyKey::new(name.clone())
                        .map_err(|e| format!("property '{property_id}': invalid name: {e}"))?;
                }
                if let Some(pt) = &patch.property_type {
                    prop.property_type = pt.clone();
                }
                if let Some(nullable) = patch.nullable {
                    prop.nullable = nullable;
                }
                if let Some(dv) = &patch.default_value {
                    prop.default_value = dv.clone();
                }
                if let Some(desc) = &patch.description {
                    prop.description = desc.clone();
                }

                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateProperty {
                        owner: owner.clone(),
                        property_id: property_id.clone(),
                        patch: reverse_patch,
                    },
                })
            }

            // ----- AddConstraint -----
            OntologyCommand::AddConstraint {
                node_id,
                constraint,
            } => {
                let idx = node_index(&ont, node_id)?;
                if ont.node_types[idx]
                    .constraints
                    .iter()
                    .any(|c| c.id == constraint.id)
                {
                    return Err(format!(
                        "constraint '{}' already exists on node '{}'",
                        constraint.id, node_id
                    ));
                }
                ont.node_types[idx].constraints.push(constraint.clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::RemoveConstraint {
                        node_id: node_id.clone(),
                        constraint_id: constraint.id.clone(),
                    },
                })
            }

            // ----- RemoveConstraint -----
            OntologyCommand::RemoveConstraint {
                node_id,
                constraint_id,
            } => {
                let idx = node_index(&ont, node_id)?;
                let c_idx = ont.node_types[idx]
                    .constraints
                    .iter()
                    .position(|c| c.id == *constraint_id)
                    .ok_or_else(|| {
                        format!(
                            "constraint '{}' not found on node '{}'",
                            constraint_id, node_id
                        )
                    })?;
                let removed = ont.node_types[idx].constraints.remove(c_idx);
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::AddConstraint {
                        node_id: node_id.clone(),
                        constraint: removed,
                    },
                })
            }

            // ----- AddIndex -----
            OntologyCommand::AddIndex { index } => {
                let id = index_id(index);
                if ont.indexes.iter().any(|i| index_id(i) == id) {
                    return Err(format!("index '{}' already exists", id));
                }
                let inverse_id = id.to_string();
                ont.indexes.push(index.clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::RemoveIndex {
                        index_id: inverse_id,
                    },
                })
            }

            // ----- RemoveIndex -----
            OntologyCommand::RemoveIndex { index_id: rid } => {
                let idx = ont
                    .indexes
                    .iter()
                    .position(|i| index_id(i) == rid.as_str())
                    .ok_or_else(|| format!("index '{}' not found", rid))?;
                let removed = ont.indexes.remove(idx);
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::AddIndex { index: removed },
                })
            }

            // ----- CreateObjectMapping -----
            OntologyCommand::CreateObjectMapping { mapping } => {
                if ont.object_mappings.iter().any(|m| m.id == mapping.id) {
                    return Err(format!("object mapping '{}' already exists", mapping.id));
                }
                let mapping_id = mapping.id.clone();
                ont.object_mappings.push((**mapping).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteObjectMapping { id: mapping_id },
                })
            }

            // ----- UpdateObjectMapping -----
            OntologyCommand::UpdateObjectMapping { id, mapping } => {
                if mapping.id != *id {
                    return Err(format!(
                        "update object mapping id mismatch: payload '{}' does not match path '{}'",
                        mapping.id, id
                    ));
                }
                let idx = ont
                    .object_mappings
                    .iter()
                    .position(|m| m.id == *id)
                    .ok_or_else(|| format!("object mapping '{}' not found", id))?;
                let old = std::mem::replace(&mut ont.object_mappings[idx], (**mapping).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateObjectMapping {
                        id: id.clone(),
                        mapping: Box::new(old),
                    },
                })
            }

            // ----- DeleteObjectMapping -----
            OntologyCommand::DeleteObjectMapping { id } => {
                let idx = ont
                    .object_mappings
                    .iter()
                    .position(|m| m.id == *id)
                    .ok_or_else(|| format!("object mapping '{}' not found", id))?;
                let removed = ont.object_mappings.remove(idx);
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::CreateObjectMapping {
                        mapping: Box::new(removed),
                    },
                })
            }

            // ----- CreateLinkMapping -----
            OntologyCommand::CreateLinkMapping { mapping } => {
                if ont.link_mappings.iter().any(|m| m.id == mapping.id) {
                    return Err(format!("link mapping '{}' already exists", mapping.id));
                }
                let mapping_id = mapping.id.clone();
                ont.link_mappings.push((**mapping).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteLinkMapping { id: mapping_id },
                })
            }

            // ----- UpdateLinkMapping -----
            OntologyCommand::UpdateLinkMapping { id, mapping } => {
                if mapping.id != *id {
                    return Err(format!(
                        "update link mapping id mismatch: payload '{}' does not match path '{}'",
                        mapping.id, id
                    ));
                }
                let idx = ont
                    .link_mappings
                    .iter()
                    .position(|m| m.id == *id)
                    .ok_or_else(|| format!("link mapping '{}' not found", id))?;
                let old = std::mem::replace(&mut ont.link_mappings[idx], (**mapping).clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::UpdateLinkMapping {
                        id: id.clone(),
                        mapping: Box::new(old),
                    },
                })
            }

            // ----- DeleteLinkMapping -----
            OntologyCommand::DeleteLinkMapping { id } => {
                let idx = ont
                    .link_mappings
                    .iter()
                    .position(|m| m.id == *id)
                    .ok_or_else(|| format!("link mapping '{}' not found", id))?;
                let removed = ont.link_mappings.remove(idx);
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::CreateLinkMapping {
                        mapping: Box::new(removed),
                    },
                })
            }

            // ----- Batch -----
            OntologyCommand::Batch {
                description,
                commands,
            } => {
                let mut current = ont;
                let mut inverses = Vec::with_capacity(commands.len());

                for (i, cmd) in commands.iter().enumerate() {
                    match cmd.execute_inner(current) {
                        Ok(result) => {
                            current = result.new_ontology;
                            inverses.push(result.inverse);
                        }
                        Err(e) => {
                            return Err(format!("batch command #{} failed: {}", i, e));
                        }
                    }
                }

                // Reverse the inverses so undo applies in reverse order
                inverses.reverse();

                Ok(CommandResult {
                    new_ontology: current,
                    inverse: OntologyCommand::Batch {
                        description: format!("undo: {}", description),
                        commands: inverses,
                    },
                })
            }
        }
    }
}
