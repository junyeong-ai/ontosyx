use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Location of a property owner (node or edge) by index.
enum OwnerLocation {
    Node(usize),
    Edge(usize),
}

/// Find the index-based location of a property owner (node or edge).
fn find_owner_location(ontology: &OntologyIR, owner_id: &str) -> Result<OwnerLocation, String> {
    if let Some(idx) = ontology.node_types.iter().position(|n| n.id == owner_id) {
        return Ok(OwnerLocation::Node(idx));
    }
    if let Some(idx) = ontology.edge_types.iter().position(|e| e.id == owner_id) {
        return Ok(OwnerLocation::Edge(idx));
    }
    Err(format!(
        "owner_id '{}' not found in nodes or edges",
        owner_id
    ))
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
                source_table,
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
                    source_table: source_table.clone(),
                    properties: vec![],
                    constraints: vec![],
                });
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteNode {
                        node_id: id.clone(),
                    },
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

                // Build inverse batch: re-add node (with all its properties/constraints) + edges + indexes
                let mut inverse_commands = Vec::new();

                // Re-add the node
                inverse_commands.push(OntologyCommand::AddNode {
                    id: node.id.clone(),
                    label: node.label.clone(),
                    description: node.description.clone(),
                    source_table: node.source_table.clone(),
                });

                // Re-add properties
                for prop in &node.properties {
                    inverse_commands.push(OntologyCommand::AddProperty {
                        owner_id: node.id.to_string(),
                        property: prop.clone(),
                    });
                }

                // Re-add constraints
                for constraint in &node.constraints {
                    inverse_commands.push(OntologyCommand::AddConstraint {
                        node_id: node.id.clone(),
                        constraint: constraint.clone(),
                    });
                }

                // Re-add edges
                for edge in &removed_edges {
                    inverse_commands.push(OntologyCommand::AddEdge {
                        id: edge.id.clone(),
                        label: edge.label.clone(),
                        source_node_id: edge.source_node_id.clone(),
                        target_node_id: edge.target_node_id.clone(),
                        cardinality: edge.cardinality,
                    });
                    // Re-add edge properties
                    for prop in &edge.properties {
                        inverse_commands.push(OntologyCommand::AddProperty {
                            owner_id: edge.id.to_string(),
                            property: prop.clone(),
                        });
                    }
                }

                // Re-add indexes
                for index in &removed_indexes {
                    inverse_commands.push(OntologyCommand::AddIndex {
                        index: index.clone(),
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
                    description: None,
                    source_node_id: source_node_id.clone(),
                    target_node_id: target_node_id.clone(),
                    properties: vec![],
                    cardinality: *cardinality,
                });
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteEdge {
                        edge_id: id.clone(),
                    },
                })
            }

            // ----- DeleteEdge -----
            OntologyCommand::DeleteEdge { edge_id } => {
                let idx = edge_index(&ont, edge_id)?;
                let edge = ont.edge_types.remove(idx);

                // Build inverse: restore edge + its properties
                let mut inverse_cmds = vec![OntologyCommand::AddEdge {
                    id: edge.id.clone(),
                    label: edge.label,
                    source_node_id: edge.source_node_id,
                    target_node_id: edge.target_node_id,
                    cardinality: edge.cardinality,
                }];
                for prop in edge.properties {
                    inverse_cmds.push(OntologyCommand::AddProperty {
                        owner_id: edge.id.to_string(),
                        property: prop,
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
            OntologyCommand::AddProperty { owner_id, property } => {
                let loc = find_owner_location(&ont, owner_id)?;
                let props = owner_properties_mut(&mut ont, &loc);
                if props.iter().any(|p| p.id == property.id) {
                    return Err(format!(
                        "property '{}' already exists on owner '{}'",
                        property.id, owner_id
                    ));
                }
                props.push(property.clone());
                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::DeleteProperty {
                        owner_id: owner_id.clone(),
                        property_id: property.id.clone(),
                    },
                })
            }

            // ----- DeleteProperty -----
            OntologyCommand::DeleteProperty {
                owner_id,
                property_id,
            } => {
                let loc = find_owner_location(&ont, owner_id)?;
                let removed_prop = {
                    let props = owner_properties_mut(&mut ont, &loc);
                    let prop_idx =
                        props
                            .iter()
                            .position(|p| p.id == *property_id)
                            .ok_or_else(|| {
                                format!(
                                    "property '{}' not found on owner '{}'",
                                    property_id, owner_id
                                )
                            })?;
                    props.remove(prop_idx)
                };

                // Remove constraints referencing this property (only on nodes)
                if let Some(node) = ont.node_types.iter_mut().find(|n| n.id == *owner_id) {
                    node.constraints
                        .retain(|c| !constraint_property_ids(c).contains(&&**property_id));
                }

                // Remove indexes on this owner that reference this property
                ont.indexes.retain(|idx| {
                    !(index_node_id(idx) == owner_id.as_str()
                        && index_property_ids(idx).contains(&&**property_id))
                });

                Ok(CommandResult {
                    new_ontology: ont,
                    inverse: OntologyCommand::AddProperty {
                        owner_id: owner_id.clone(),
                        property: removed_prop,
                    },
                })
            }

            // ----- UpdateProperty -----
            OntologyCommand::UpdateProperty {
                owner_id,
                property_id,
                patch,
            } => {
                let loc = find_owner_location(&ont, owner_id)?;
                let props = owner_properties_mut(&mut ont, &loc);
                let prop = props
                    .iter_mut()
                    .find(|p| p.id == *property_id)
                    .ok_or_else(|| {
                        format!(
                            "property '{}' not found on owner '{}'",
                            property_id, owner_id
                        )
                    })?;

                // Build reverse patch from current values before applying
                let reverse_patch = PropertyPatch {
                    name: patch.name.as_ref().map(|_| prop.name.clone()),
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

                // Apply patch
                if let Some(name) = &patch.name {
                    prop.name = name.clone();
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
                        owner_id: owner_id.clone(),
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
