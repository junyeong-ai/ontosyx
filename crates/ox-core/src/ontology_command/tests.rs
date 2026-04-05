use super::*;
use crate::test_fixtures::{ontologies_equal, test_ontology};
use crate::types::PropertyType;

#[test]
fn add_and_delete_node_roundtrip() {
    let ontology = test_ontology();

    // Add a node
    let cmd = OntologyCommand::AddNode {
        id: "n3".into(),
        label: "Product".to_string(),
        description: Some("A product".to_string()),
        source_table: None,
    };
    let result = cmd.execute(&ontology).unwrap();
    assert_eq!(result.new_ontology.node_types.len(), 3);
    assert!(
        result
            .new_ontology
            .node_types
            .iter()
            .any(|n| n.id == "n3" && n.label == "Product")
    );

    // Execute inverse (DeleteNode) to get back to original
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(ontologies_equal(&ontology, &restored.new_ontology));
}

#[test]
fn rename_node_preserves_edges() {
    let ontology = test_ontology();

    let cmd = OntologyCommand::RenameNode {
        node_id: "n1".into(),
        new_label: "Individual".to_string(),
    };
    let result = cmd.execute(&ontology).unwrap();

    // Label changed
    assert_eq!(
        result.new_ontology.node_by_id("n1").unwrap().label,
        "Individual"
    );

    // Edge still references same node_id (not label-based)
    let edge = result.new_ontology.edge_by_id("e1").unwrap();
    assert_eq!(edge.source_node_id, "n1");
    assert_eq!(edge.target_node_id, "n2");

    // Inverse restores original label
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert_eq!(
        restored.new_ontology.node_by_id("n1").unwrap().label,
        "Person"
    );
}

#[test]
fn delete_node_cascades_edges() {
    let ontology = test_ontology();

    // Delete n1 (Person) — should cascade WORKS_AT edge and idx1 index
    let cmd = OntologyCommand::DeleteNode {
        node_id: "n1".into(),
    };
    let result = cmd.execute(&ontology).unwrap();

    assert_eq!(result.new_ontology.node_types.len(), 1);
    assert!(result.new_ontology.edge_types.is_empty());
    assert!(result.new_ontology.indexes.is_empty());

    // Inverse restores everything
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert_eq!(restored.new_ontology.node_types.len(), 2);
    assert_eq!(restored.new_ontology.edge_types.len(), 1);
    assert_eq!(restored.new_ontology.indexes.len(), 1);

    // Verify the restored node has its properties and constraints
    let person = restored.new_ontology.node_by_id("n1").unwrap();
    assert_eq!(person.label, "Person");
    assert_eq!(person.properties.len(), 2);
    assert_eq!(person.constraints.len(), 1);

    // Verify the restored edge has its properties
    let edge = restored.new_ontology.edge_by_id("e1").unwrap();
    assert_eq!(edge.properties.len(), 1);
}

#[test]
fn add_delete_property() {
    let ontology = test_ontology();

    // Add property to node n2
    let new_prop = PropertyDef {
        id: "p4".into(),
        name: "industry".to_string(),
        property_type: PropertyType::String,
        nullable: true,
        default_value: None,
        description: Some("Industry sector".to_string()),
        classification: None,
    };
    let add_cmd = OntologyCommand::AddProperty {
        owner_id: "n2".to_string(),
        property: new_prop,
    };
    let add_result = add_cmd.execute(&ontology).unwrap();
    assert_eq!(
        add_result
            .new_ontology
            .node_by_id("n2")
            .unwrap()
            .properties
            .len(),
        2
    );

    // Delete it via inverse
    let del_result = add_result
        .inverse
        .execute(&add_result.new_ontology)
        .unwrap();
    assert!(ontologies_equal(&ontology, &del_result.new_ontology));

    // Also test AddProperty on an edge
    let edge_prop = PropertyDef {
        id: "ep2".into(),
        name: "role".to_string(),
        property_type: PropertyType::String,
        nullable: true,
        default_value: None,
        description: None,
        classification: None,
    };
    let add_edge_cmd = OntologyCommand::AddProperty {
        owner_id: "e1".to_string(),
        property: edge_prop,
    };
    let edge_result = add_edge_cmd.execute(&ontology).unwrap();
    assert_eq!(
        edge_result
            .new_ontology
            .edge_by_id("e1")
            .unwrap()
            .properties
            .len(),
        2
    );
}

#[test]
fn batch_execute_and_inverse() {
    let ontology = test_ontology();

    let batch = OntologyCommand::Batch {
        description: "add node and edge".to_string(),
        commands: vec![
            OntologyCommand::AddNode {
                id: "n3".into(),
                label: "Project".to_string(),
                description: None,
                source_table: None,
            },
            OntologyCommand::AddEdge {
                id: "e2".into(),
                label: "MANAGES".to_string(),
                source_node_id: "n1".into(),
                target_node_id: "n3".into(),
                cardinality: Cardinality::OneToMany,
            },
            OntologyCommand::RenameNode {
                node_id: "n2".into(),
                new_label: "Organization".to_string(),
            },
        ],
    };

    let result = batch.execute(&ontology).unwrap();
    assert_eq!(result.new_ontology.node_types.len(), 3);
    assert_eq!(result.new_ontology.edge_types.len(), 2);
    assert_eq!(
        result.new_ontology.node_by_id("n2").unwrap().label,
        "Organization"
    );

    // Inverse undoes everything
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(ontologies_equal(&ontology, &restored.new_ontology));
}

#[test]
fn update_property_roundtrip() {
    let ontology = test_ontology();

    let patch = PropertyPatch {
        name: Some("full_name".to_string()),
        property_type: Some(PropertyType::String),
        nullable: Some(true),
        default_value: None,
        description: Some(Some("Full name of person".to_string())),
    };
    let cmd = OntologyCommand::UpdateProperty {
        owner_id: "n1".to_string(),
        property_id: "p1".into(),
        patch,
    };
    let result = cmd.execute(&ontology).unwrap();
    let updated_prop = result
        .new_ontology
        .node_by_id("n1")
        .unwrap()
        .properties
        .iter()
        .find(|p| p.id == "p1")
        .unwrap();
    assert_eq!(updated_prop.name, "full_name");
    assert!(updated_prop.nullable);
    assert_eq!(
        updated_prop.description,
        Some("Full name of person".to_string())
    );

    // Inverse restores original
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(ontologies_equal(&ontology, &restored.new_ontology));
}

#[test]
fn add_remove_constraint_roundtrip() {
    let ontology = test_ontology();

    let constraint = ConstraintDef {
        id: "c2".into(),
        constraint: NodeConstraint::Exists {
            property_id: "p2".into(),
        },
    };
    let cmd = OntologyCommand::AddConstraint {
        node_id: "n1".into(),
        constraint,
    };
    let result = cmd.execute(&ontology).unwrap();
    assert_eq!(
        result
            .new_ontology
            .node_by_id("n1")
            .unwrap()
            .constraints
            .len(),
        2
    );

    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(ontologies_equal(&ontology, &restored.new_ontology));
}

#[test]
fn add_remove_index_roundtrip() {
    let ontology = test_ontology();

    let index = IndexDef::Composite {
        id: "idx2".to_string(),
        node_id: "n1".into(),
        property_ids: vec!["p1".into(), "p2".into()],
    };
    let cmd = OntologyCommand::AddIndex { index };
    let result = cmd.execute(&ontology).unwrap();
    assert_eq!(result.new_ontology.indexes.len(), 2);

    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(ontologies_equal(&ontology, &restored.new_ontology));
}

#[test]
fn error_on_invalid_references() {
    let ontology = test_ontology();

    // Delete nonexistent node
    let cmd = OntologyCommand::DeleteNode {
        node_id: "nonexistent".into(),
    };
    assert!(cmd.execute(&ontology).is_err());

    // Add edge with invalid source
    let cmd = OntologyCommand::AddEdge {
        id: "e99".into(),
        label: "BAD".to_string(),
        source_node_id: "nonexistent".into(),
        target_node_id: "n2".into(),
        cardinality: Cardinality::OneToOne,
    };
    assert!(cmd.execute(&ontology).is_err());

    // Delete property from nonexistent owner
    let cmd = OntologyCommand::DeleteProperty {
        owner_id: "nonexistent".to_string(),
        property_id: "p1".into(),
    };
    assert!(cmd.execute(&ontology).is_err());

    // Add duplicate node id
    let cmd = OntologyCommand::AddNode {
        id: "n1".into(),
        label: "Duplicate".to_string(),
        description: None,
        source_table: None,
    };
    assert!(cmd.execute(&ontology).is_err());
}

#[test]
fn delete_property_cascades_constraints_and_indexes() {
    let ontology = test_ontology();

    // Delete p1 (which is referenced by constraint c1 and index idx1)
    let cmd = OntologyCommand::DeleteProperty {
        owner_id: "n1".to_string(),
        property_id: "p1".into(),
    };
    let result = cmd.execute(&ontology).unwrap();

    let node = result.new_ontology.node_by_id("n1").unwrap();
    assert_eq!(node.properties.len(), 1);
    assert!(node.constraints.is_empty()); // c1 removed
    assert!(result.new_ontology.indexes.is_empty()); // idx1 removed
}
