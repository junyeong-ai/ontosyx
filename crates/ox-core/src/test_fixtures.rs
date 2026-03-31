use crate::ontology_ir::{
    Cardinality, ConstraintDef, EdgeTypeDef, IndexDef, NodeConstraint, NodeTypeDef, OntologyIR,
    PropertyDef,
};
use crate::types::PropertyType;

/// Create a simple non-nullable string property with the given id and name.
pub fn property(id: &str, name: &str) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: name.to_string(),
        property_type: PropertyType::String,
        nullable: false,
        default_value: None,
        description: None,
    }
}

/// Build a standard test ontology with Person, Company, WORKS_AT edge, and one index.
pub fn test_ontology() -> OntologyIR {
    OntologyIR::new(
        "ont-1".to_string(),
        "Test Ontology".to_string(),
        None,
        1,
        vec![
            NodeTypeDef {
                id: "n1".into(),
                label: "Person".to_string(),
                description: Some("A person".to_string()),
                source_table: None,
                properties: vec![property("p1", "name"), property("p2", "age")],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p1".into()],
                    },
                }],
            },
            NodeTypeDef {
                id: "n2".into(),
                label: "Company".to_string(),
                description: None,
                source_table: None,
                properties: vec![property("p3", "company_name")],
                constraints: vec![],
            },
        ],
        vec![EdgeTypeDef {
            id: "e1".into(),
            label: "WORKS_AT".to_string(),
            description: None,
            source_node_id: "n1".into(),
            target_node_id: "n2".into(),
            properties: vec![property("ep1", "since")],
            cardinality: Cardinality::ManyToOne,
        }],
        vec![IndexDef::Single {
            id: "idx1".to_string(),
            node_id: "n1".into(),
            property_id: "p1".into(),
        }],
    )
}

/// Compare ontologies structurally via serialization.
pub fn ontologies_equal(a: &OntologyIR, b: &OntologyIR) -> bool {
    let a_json = serde_json::to_string(a).unwrap();
    let b_json = serde_json::to_string(b).unwrap();
    a_json == b_json
}
