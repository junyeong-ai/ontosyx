use crate::types::is_valid_graph_identifier;

use super::{
    IndexDef, NodeConstraint, NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId,
};

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_property_defs(
    owner_kind: &str,
    owner_label: &str,
    properties: &[PropertyDef],
    errors: &mut Vec<String>,
) {
    let mut seen_ids = std::collections::HashSet::<&PropertyId>::new();
    let mut seen_names = std::collections::HashSet::new();

    for property in properties {
        if property.id.trim().is_empty() {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has a property with an empty id"
            ));
        } else if !seen_ids.insert(&property.id) {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has duplicate property id '{}'",
                property.id
            ));
        }

        let name = property.name.trim();
        if name.is_empty() {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has a property with an empty name"
            ));
            continue;
        }

        if !is_valid_graph_identifier(name) {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has invalid property name '{name}': must contain only alphanumeric characters, underscores, or spaces"
            ));
        }

        if !seen_names.insert(name.to_string()) {
            errors.push(format!(
                "{owner_kind} '{owner_label}' has duplicate property '{name}'"
            ));
        }
    }
}

fn property_def_by_id<'a>(properties: &'a [PropertyDef], id: &str) -> Option<&'a PropertyDef> {
    properties.iter().find(|property| property.id == id)
}

fn validate_constraint_fields(
    node: &NodeTypeDef,
    property_ids: &[PropertyId],
    constraint_name: &str,
    require_non_nullable: bool,
    errors: &mut Vec<String>,
) {
    if property_ids.is_empty() {
        errors.push(format!(
            "Node '{}' has an empty {constraint_name} constraint",
            node.label
        ));
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(format!(
                "Node '{}' has a {constraint_name} constraint with an empty property id",
                node.label
            ));
            continue;
        }

        if !seen.insert(id) {
            errors.push(format!(
                "Node '{}' has duplicate property id '{}' in a {constraint_name} constraint",
                node.label, id
            ));
        }

        match property_def_by_id(&node.properties, id) {
            Some(def) => {
                if require_non_nullable && def.nullable {
                    errors.push(format!(
                        "Node '{}' constraint '{}' requires non-nullable property '{}'",
                        node.label, constraint_name, def.name
                    ));
                }
            }
            None => errors.push(format!(
                "Node '{}' constraint references unknown property id '{}'",
                node.label, id
            )),
        }
    }
}

fn validate_index_target(
    node_types: &[NodeTypeDef],
    node_id: &NodeTypeId,
    property_ids: &[PropertyId],
    index_name: &str,
    errors: &mut Vec<String>,
) {
    let Some(node) = node_types.iter().find(|node| node.id == *node_id) else {
        errors.push(format!(
            "Index '{}' references unknown node id '{}'",
            index_name, node_id
        ));
        return;
    };

    if property_ids.is_empty() {
        errors.push(format!(
            "Index '{}' on node '{}' must reference at least one property",
            index_name, node.label
        ));
        return;
    }

    let mut seen = std::collections::HashSet::<&str>::new();
    for prop_id in property_ids {
        let id = prop_id.trim();
        if id.is_empty() {
            errors.push(format!(
                "Index '{}' on node '{}' contains an empty property id",
                index_name, node.label
            ));
            continue;
        }

        if !seen.insert(id) {
            errors.push(format!(
                "Index '{}' on node '{}' contains duplicate property id '{}'",
                index_name, node.label, id
            ));
        }

        if property_def_by_id(&node.properties, id).is_none() {
            errors.push(format!(
                "Index '{}' references unknown property id '{}' on node '{}'",
                index_name, id, node.label
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Validate internal consistency of the ontology.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.id.trim().is_empty() {
            errors.push("Ontology id must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            errors.push("Ontology name must not be empty".to_string());
        }
        if self.node_types.is_empty() {
            errors.push("Ontology must define at least one node type".to_string());
        }

        let mut seen_node_ids = std::collections::HashSet::<NodeTypeId>::new();
        let mut seen_node_labels = std::collections::HashSet::new();

        for node in &self.node_types {
            // Validate node id
            if node.id.trim().is_empty() {
                errors.push("Node type id must not be empty".to_string());
            } else if !seen_node_ids.insert(node.id.clone()) {
                errors.push(format!("Duplicate node type id: '{}'", node.id));
            }

            let label = node.label.trim();
            if label.is_empty() {
                errors.push("Node type label must not be empty".to_string());
                continue;
            }

            if !is_valid_graph_identifier(label) {
                errors.push(format!(
                    "Invalid node label '{label}': must contain only alphanumeric characters, underscores, or spaces"
                ));
            }

            if !seen_node_labels.insert(label.to_string()) {
                errors.push(format!("Duplicate node type label: '{label}'"));
            }

            validate_property_defs("Node", label, &node.properties, &mut errors);

            for constraint_def in &node.constraints {
                if constraint_def.id.trim().is_empty() {
                    errors.push(format!(
                        "Node '{}' has a constraint with an empty id",
                        node.label
                    ));
                }

                match &constraint_def.constraint {
                    NodeConstraint::Unique { property_ids } => {
                        validate_constraint_fields(
                            node,
                            property_ids,
                            "unique",
                            false,
                            &mut errors,
                        );
                    }
                    NodeConstraint::NodeKey { property_ids } => {
                        validate_constraint_fields(
                            node,
                            property_ids,
                            "node_key",
                            true,
                            &mut errors,
                        );
                    }
                    NodeConstraint::Exists { property_id } => {
                        validate_constraint_fields(
                            node,
                            std::slice::from_ref(property_id),
                            "exists",
                            true,
                            &mut errors,
                        );
                    }
                }
            }
        }

        // Check edge types reference valid node IDs
        let mut seen_edge_signatures = std::collections::HashSet::new();
        for edge in &self.edge_types {
            // Validate edge id
            if edge.id.trim().is_empty() {
                errors.push("Edge type id must not be empty".to_string());
            }

            let label = edge.label.trim();
            if label.is_empty() {
                errors.push("Edge type label must not be empty".to_string());
            } else if !is_valid_graph_identifier(label) {
                errors.push(format!(
                    "Invalid edge label '{label}': must contain only alphanumeric characters, underscores, or spaces"
                ));
            }
            if edge.source_node_id.trim().is_empty() || edge.target_node_id.trim().is_empty() {
                errors.push(format!(
                    "Edge '{}' must define both source_node_id and target_node_id",
                    edge.label
                ));
            }
            if !seen_edge_signatures.insert((
                edge.label.clone(),
                edge.source_node_id.clone(),
                edge.target_node_id.clone(),
            )) {
                errors.push(format!(
                    "Duplicate edge type definition: '{}({}->{})'",
                    edge.label, edge.source_node_id, edge.target_node_id
                ));
            }

            validate_property_defs("Edge", &edge.label, &edge.properties, &mut errors);

            if !seen_node_ids.contains::<str>(&edge.source_node_id) {
                errors.push(format!(
                    "Edge '{}' references unknown source node id '{}'",
                    edge.label, edge.source_node_id
                ));
            }
            if !seen_node_ids.contains::<str>(&edge.target_node_id) {
                errors.push(format!(
                    "Edge '{}' references unknown target node id '{}'",
                    edge.label, edge.target_node_id
                ));
            }
        }

        for index in &self.indexes {
            match index {
                IndexDef::Single {
                    id: _,
                    node_id,
                    property_id,
                } => validate_index_target(
                    &self.node_types,
                    node_id,
                    std::slice::from_ref(property_id),
                    "single",
                    &mut errors,
                ),
                IndexDef::Composite {
                    id: _,
                    node_id,
                    property_ids,
                } => validate_index_target(
                    &self.node_types,
                    node_id,
                    property_ids,
                    "composite",
                    &mut errors,
                ),
                IndexDef::FullText {
                    id: _,
                    name,
                    node_id,
                    property_ids,
                } => {
                    if name.trim().is_empty() {
                        errors.push("Full-text index name must not be empty".to_string());
                    }
                    validate_index_target(
                        &self.node_types,
                        node_id,
                        property_ids,
                        name,
                        &mut errors,
                    );
                }
                IndexDef::Vector {
                    id: _,
                    node_id,
                    property_id,
                    dimensions,
                    ..
                } => {
                    if *dimensions == 0 {
                        errors.push(format!(
                            "Vector index on node '{}' property '{}' must have dimensions > 0",
                            node_id, property_id
                        ));
                    }
                    validate_index_target(
                        &self.node_types,
                        node_id,
                        std::slice::from_ref(property_id),
                        "vector",
                        &mut errors,
                    );
                }
            }
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use crate::ontology_ir::*;
    use crate::types::PropertyType;

    fn property(id: &str, name: &str, nullable: bool) -> PropertyDef {
        PropertyDef {
            id: id.into(),
            name: name.to_string(),
            property_type: PropertyType::String,
            nullable,
            default_value: None,
            description: None,
            classification: None,
        }
    }

    fn base_ontology() -> OntologyIR {
        OntologyIR::new(
            "test".to_string(),
            "Test".to_string(),
            None,
            1,
            vec![NodeTypeDef {
                id: "node-user".into(),
                label: "User".to_string(),
                description: None,
                source_table: None,
                properties: vec![
                    property("prop-id", "id", false),
                    property("prop-email", "email", false),
                ],
                constraints: vec![
                    ConstraintDef {
                        id: "cst-unique-email".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["prop-email".into()],
                        },
                    },
                    ConstraintDef {
                        id: "cst-exists-id".into(),
                        constraint: NodeConstraint::Exists {
                            property_id: "prop-id".into(),
                        },
                    },
                ],
            }],
            vec![EdgeTypeDef {
                id: "edge-owns".into(),
                label: "OWNS".to_string(),
                description: None,
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
            }],
            vec![IndexDef::Single {
                id: "idx-user-email".to_string(),
                node_id: "node-user".into(),
                property_id: "prop-email".into(),
            }],
        )
    }

    #[test]
    fn validate_accepts_well_formed_ontology() {
        let ontology = base_ontology();
        assert!(ontology.validate().is_empty());
    }

    #[test]
    fn validate_rejects_duplicate_properties_and_bad_indexes() {
        let mut ontology = base_ontology();
        ontology.node_types[0]
            .properties
            .push(property("prop-email-dup", "email", false));
        ontology.indexes.push(IndexDef::Composite {
            id: "idx-composite".to_string(),
            node_id: "node-user".into(),
            property_ids: vec!["prop-email".into(), "prop-missing".into()],
        });

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate property 'email'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown property id 'prop-missing'"))
        );
    }

    #[test]
    fn validate_rejects_nullable_required_constraints() {
        let mut ontology = base_ontology();
        ontology.node_types[0].properties[0].nullable = true;

        let errors = ontology.validate();

        assert!(errors.iter().any(|error| {
            error.contains("constraint 'exists' requires non-nullable property 'id'")
        }));
    }

    #[test]
    fn validate_rejects_empty_id_name_and_no_node_types() {
        let ontology = OntologyIR::new(
            "  ".to_string(),
            String::new(),
            None,
            1,
            vec![],
            vec![],
            vec![],
        );

        let errors = ontology.validate();

        assert!(errors.iter().any(|e| e.contains("id must not be empty")));
        assert!(errors.iter().any(|e| e.contains("name must not be empty")));
        assert!(errors.iter().any(|e| e.contains("at least one node type")));
    }

    #[test]
    fn validate_rejects_edge_referencing_unknown_node_id() {
        let mut ontology = base_ontology();
        ontology.edge_types[0].source_node_id = "node-nonexistent".into();

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.contains("unknown source node id 'node-nonexistent'"))
        );
    }

    #[test]
    fn validate_rejects_constraint_referencing_unknown_property_id() {
        let mut ontology = base_ontology();
        ontology.node_types[0].constraints.push(ConstraintDef {
            id: "cst-bad".into(),
            constraint: NodeConstraint::Unique {
                property_ids: vec!["prop-nonexistent".into()],
            },
        });

        let errors = ontology.validate();

        assert!(
            errors
                .iter()
                .any(|e| e.contains("unknown property id 'prop-nonexistent'"))
        );
    }
}
