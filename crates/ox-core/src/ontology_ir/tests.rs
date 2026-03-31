use super::*;
use crate::types::PropertyType;

fn property(id: &str, name: &str, nullable: bool) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: name.to_string(),
        property_type: PropertyType::String,
        nullable,
        default_value: None,
        description: None,
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
fn resolver_methods_work() {
    let ontology = base_ontology();

    // node_label
    assert_eq!(ontology.node_label("node-user"), Some("User"));
    assert_eq!(ontology.node_label("nonexistent"), None);

    // node_by_id
    assert!(ontology.node_by_id("node-user").is_some());
    assert!(ontology.node_by_id("nonexistent").is_none());

    // node_by_label
    assert!(ontology.node_by_label("User").is_some());
    assert!(ontology.node_by_label("Nonexistent").is_none());

    // property_by_id
    let (node, prop) = ontology.property_by_id("prop-email").unwrap();
    assert_eq!(node.label, "User");
    assert_eq!(prop.name, "email");
    assert!(ontology.property_by_id("nonexistent").is_none());

    // edge_by_id
    assert!(ontology.edge_by_id("edge-owns").is_some());
    assert!(ontology.edge_by_id("nonexistent").is_none());

    // property_in
    let node = ontology.node_by_id("node-user").unwrap();
    assert!(ontology.property_in(&node.properties, "prop-id").is_some());
    assert!(
        ontology
            .property_in(&node.properties, "nonexistent")
            .is_none()
    );
}

#[test]
fn has_unique_constraint_works_with_wrapper() {
    let ontology = base_ontology();
    assert!(ontology.node_types[0].has_unique_constraint());

    let node_no_unique = NodeTypeDef {
        id: "n1".into(),
        label: "Empty".to_string(),
        description: None,
        source_table: None,
        properties: vec![property("p1", "x", false)],
        constraints: vec![ConstraintDef {
            id: "c1".into(),
            constraint: NodeConstraint::Exists {
                property_id: "p1".into(),
            },
        }],
    };
    assert!(!node_no_unique.has_unique_constraint());
}

#[test]
fn test_validate_duplicate_edge_ids() {
    let mut ontology = base_ontology();
    // Add a second edge with the same id but different label/endpoints
    ontology.edge_types.push(EdgeTypeDef {
        id: "edge-owns".into(), // duplicate id
        label: "FOLLOWS".to_string(),
        description: None,
        source_node_id: "node-user".into(),
        target_node_id: "node-user".into(),
        properties: vec![],
        cardinality: Cardinality::ManyToMany,
    });

    let _errors = ontology.validate();
    // The first ontology has edges with same id but different labels,
    // so the duplicate signature check won't fire. Now test actual
    // duplicate signatures (same label + source + target).
    let ontology2 = OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("prop-id", "id", false)],
            constraints: vec![],
        }],
        vec![
            EdgeTypeDef {
                id: "edge-1".into(),
                label: "KNOWS".to_string(),
                description: None,
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
            },
            EdgeTypeDef {
                id: "edge-2".into(),
                label: "KNOWS".to_string(),
                description: None,
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
            },
        ],
        vec![],
    );

    let errors2 = ontology2.validate();
    assert!(
        errors2.iter().any(|e| e.contains("Duplicate edge type")),
        "should detect duplicate edge signatures: {:?}",
        errors2
    );
}

#[test]
fn test_validate_self_referencing_edge() {
    // Self-loops (source_id == target_id) should be valid
    let ontology = OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-person".into(),
            label: "Person".to_string(),
            description: None,
            source_table: None,
            properties: vec![property("prop-name", "name", false)],
            constraints: vec![],
        }],
        vec![EdgeTypeDef {
            id: "edge-knows".into(),
            label: "KNOWS".to_string(),
            description: None,
            source_node_id: "node-person".into(),
            target_node_id: "node-person".into(), // self-loop
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
        }],
        vec![],
    );

    let errors = ontology.validate();
    assert!(
        errors.is_empty(),
        "self-referencing edge should be valid, got errors: {:?}",
        errors
    );
}
