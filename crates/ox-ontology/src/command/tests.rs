use super::*;
use ox_core::graph_label::GraphLabel;
use ox_core::property_key::PropertyKey;
use crate::glossary::{GlossaryTermDef, TermGovernance, TermLifecycle};
use crate::test_fixtures::{ontologies_equal, test_ontology};
use ox_core::types::PropertyType;

use ox_core::i18n::LocalizedText;

fn glossary_term(id: &str, label: &str) -> GlossaryTermDef {
    GlossaryTermDef {
        id: id.into(),
        term: LocalizedText::new(label),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: Vec::new(),
        related_terms: Vec::new(),
        governance: TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::default(),
    concept_id: None,
    }
}

fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("test label literal must be valid")
}

fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("test property name literal must be valid")
}
#[test]
fn add_and_delete_node_roundtrip() {
    let ontology = test_ontology();

    // Add a node
    let cmd = OntologyCommand::AddNode {
        id: "n3".into(),
        label: gl("Product"),
        description: LocalizedText::new("A product"),
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
        new_label: gl("Individual"),
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
        name: pk("industry"),
        property_type: PropertyType::String,
        nullable: true,
        default_value: None,
        description: LocalizedText::new("Industry sector"),
        classification: None,
        ..Default::default()
    };
    let add_cmd = OntologyCommand::AddProperty {
        owner: PropertyOwner::Node("n2".into()),
        property: Box::new(new_prop),
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
        name: pk("role"),
        property_type: PropertyType::String,
        nullable: true,
        default_value: None,
        description: LocalizedText::default(),
        classification: None,
        ..Default::default()
    };
    let add_edge_cmd = OntologyCommand::AddProperty {
        owner: PropertyOwner::Edge("e1".into()),
        property: Box::new(edge_prop),
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
                label: gl("Project"),
                description: LocalizedText::default(),
            },
            OntologyCommand::AddEdge {
                id: "e2".into(),
                label: gl("MANAGES"),
                source_node_id: "n1".into(),
                target_node_id: "n3".into(),
                cardinality: Cardinality::OneToMany,
            },
            OntologyCommand::RenameNode {
                node_id: "n2".into(),
                new_label: gl("Organization"),
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
        description: Some(LocalizedText::new("Full name of person")),
    };
    let cmd = OntologyCommand::UpdateProperty {
        owner: PropertyOwner::Node("n1".into()),
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
        LocalizedText::new("Full name of person")
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
        label: gl("BAD"),
        source_node_id: "nonexistent".into(),
        target_node_id: "n2".into(),
        cardinality: Cardinality::OneToOne,
    };
    assert!(cmd.execute(&ontology).is_err());

    // Delete property from nonexistent owner
    let cmd = OntologyCommand::DeleteProperty {
        owner: PropertyOwner::Node("nonexistent".into()),
        property_id: "p1".into(),
    };
    assert!(cmd.execute(&ontology).is_err());

    // Add duplicate node id
    let cmd = OntologyCommand::AddNode {
        id: "n1".into(),
        label: gl("Duplicate"),
        description: LocalizedText::default(),
    };
    assert!(cmd.execute(&ontology).is_err());
}

#[test]
fn set_node_glossary_anchors_replaces_list_atomically() {
    let mut ontology = test_ontology();
    // Validation requires every anchor id to resolve in the glossary.
    ontology
        .add_glossary_term(glossary_term("gt-customer", "Customer"))
        .expect("seed glossary term");
    ontology
        .add_glossary_term(glossary_term("gt-loyalty", "Loyalty"))
        .expect("seed glossary term");

    let cmd = OntologyCommand::SetNodeGlossaryAnchors {
        node_id: "n1".into(),
        anchors: vec!["gt-customer".into(), "gt-loyalty".into()],
    };
    let result = cmd.execute(&ontology).unwrap();

    let node = result.new_ontology.node_by_id("n1").unwrap();
    assert_eq!(node.glossary_anchors.len(), 2);
    assert_eq!(node.glossary_anchors[0].as_str(), "gt-customer");
    assert_eq!(node.glossary_anchors[1].as_str(), "gt-loyalty");

    // Inverse restores the empty list
    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(
        restored
            .new_ontology
            .node_by_id("n1")
            .unwrap()
            .glossary_anchors
            .is_empty(),
    );
}

#[test]
fn set_node_glossary_anchors_rejects_unknown_id() {
    let ontology = test_ontology();
    let cmd = OntologyCommand::SetNodeGlossaryAnchors {
        node_id: "n1".into(),
        anchors: vec!["gt-orphan".into()],
    };
    assert!(cmd.execute(&ontology).is_err());
}

#[test]
fn set_edge_glossary_anchors_replaces_list_atomically() {
    let mut ontology = test_ontology();
    ontology
        .add_glossary_term(glossary_term("gt-employment", "Employment"))
        .expect("seed glossary term");

    let cmd = OntologyCommand::SetEdgeGlossaryAnchors {
        edge_id: "e1".into(),
        anchors: vec!["gt-employment".into()],
    };
    let result = cmd.execute(&ontology).unwrap();
    let edge = result.new_ontology.edge_by_id("e1").unwrap();
    assert_eq!(edge.glossary_anchors.len(), 1);

    let restored = result.inverse.execute(&result.new_ontology).unwrap();
    assert!(
        restored
            .new_ontology
            .edge_by_id("e1")
            .unwrap()
            .glossary_anchors
            .is_empty(),
    );
}

#[test]
fn object_mapping_create_update_delete_roundtrip() {
    use crate::mapping::ObjectMappingDef;

    let ontology = test_ontology();
    let mapping = ObjectMappingDef::new("om-1", "n2", "pg-main", "public.companies");

    // Create
    let create = OntologyCommand::CreateObjectMapping {
        mapping: Box::new(mapping.clone()),
    };
    let create_result = create.execute(&ontology).unwrap();
    assert_eq!(create_result.new_ontology.object_mappings.len(), 1);
    assert_eq!(
        create_result.new_ontology.object_mappings[0].relation,
        "public.companies",
    );

    // Update
    let mut updated = mapping.clone();
    updated.relation = "warehouse.companies".into();
    let update = OntologyCommand::UpdateObjectMapping {
        id: "om-1".into(),
        mapping: Box::new(updated),
    };
    let update_result = update.execute(&create_result.new_ontology).unwrap();
    assert_eq!(
        update_result.new_ontology.object_mappings[0].relation,
        "warehouse.companies",
    );

    // Inverse of update restores the previous mapping
    let restored = update_result
        .inverse
        .execute(&update_result.new_ontology)
        .unwrap();
    assert_eq!(
        restored.new_ontology.object_mappings[0].relation,
        "public.companies",
    );

    // Delete
    let delete = OntologyCommand::DeleteObjectMapping {
        id: "om-1".into(),
    };
    let delete_result = delete.execute(&create_result.new_ontology).unwrap();
    assert!(delete_result.new_ontology.object_mappings.is_empty());

    // Inverse of delete restores the mapping
    let resurrected = delete_result
        .inverse
        .execute(&delete_result.new_ontology)
        .unwrap();
    assert_eq!(resurrected.new_ontology.object_mappings.len(), 1);
}

#[test]
fn create_object_mapping_rejects_duplicate_id() {
    use crate::mapping::ObjectMappingDef;

    let ontology = test_ontology();
    let mapping = ObjectMappingDef::new("om-1", "n2", "pg-main", "public.companies");
    let first = OntologyCommand::CreateObjectMapping {
        mapping: Box::new(mapping.clone()),
    };
    let after_first = first.execute(&ontology).unwrap().new_ontology;

    let duplicate = OntologyCommand::CreateObjectMapping {
        mapping: Box::new(mapping),
    };
    assert!(duplicate.execute(&after_first).is_err());
}

#[test]
fn delete_property_cascades_constraints_and_indexes() {
    let ontology = test_ontology();

    // Delete p1 (which is referenced by constraint c1 and index idx1)
    let cmd = OntologyCommand::DeleteProperty {
        owner: PropertyOwner::Node("n1".into()),
        property_id: "p1".into(),
    };
    let result = cmd.execute(&ontology).unwrap();

    let node = result.new_ontology.node_by_id("n1").unwrap();
    assert_eq!(node.properties.len(), 1);
    assert!(node.constraints.is_empty()); // c1 removed
    assert!(result.new_ontology.indexes.is_empty()); // idx1 removed
}
