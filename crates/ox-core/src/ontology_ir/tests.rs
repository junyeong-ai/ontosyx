use super::*;
use crate::types::PropertyType;

use crate::i18n::LocalizedText;
fn property(id: &str, name: &str, nullable: bool) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: name.to_string(),
        property_type: PropertyType::String,
        nullable,
        default_value: None,
        description: LocalizedText::default(),
        classification: None,
    ..Default::default()
    }
}

fn base_ontology() -> OntologyIR {
    OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: LocalizedText::default(),
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
            ..Default::default()
        }],
        vec![EdgeTypeDef {
            id: "edge-owns".into(),
            label: "OWNS".to_string(),
            description: LocalizedText::default(),
            source_node_id: "node-user".into(),
            target_node_id: "node-user".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
            ..Default::default()
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
        description: LocalizedText::default(),
        properties: vec![property("p1", "x", false)],
        constraints: vec![ConstraintDef {
            id: "c1".into(),
            constraint: NodeConstraint::Exists {
                property_id: "p1".into(),
            },
        }],
        ..Default::default()
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
        description: LocalizedText::default(),
        source_node_id: "node-user".into(),
        target_node_id: "node-user".into(),
        properties: vec![],
        cardinality: Cardinality::ManyToMany,
        ..Default::default()
    });

    let _errors = ontology.validate();
    // The first ontology has edges with same id but different labels,
    // so the duplicate signature check won't fire. Now test actual
    // duplicate signatures (same label + source + target).
    let ontology2 = OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: "User".to_string(),
            description: LocalizedText::default(),
            properties: vec![property("prop-id", "id", false)],
            constraints: vec![],
            ..Default::default()
        }],
        vec![
            EdgeTypeDef {
                id: "edge-1".into(),
                label: "KNOWS".to_string(),
                description: LocalizedText::default(),
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            },
            EdgeTypeDef {
                id: "edge-2".into(),
                label: "KNOWS".to_string(),
                description: LocalizedText::default(),
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
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
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-person".into(),
            label: "Person".to_string(),
            description: LocalizedText::default(),
            properties: vec![property("prop-name", "name", false)],
            constraints: vec![],
            ..Default::default()
        }],
        vec![EdgeTypeDef {
            id: "edge-knows".into(),
            label: "KNOWS".to_string(),
            description: LocalizedText::default(),
            source_node_id: "node-person".into(),
            target_node_id: "node-person".into(), // self-loop
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
            ..Default::default()
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

// ---------------------------------------------------------------------------
// SchemaView tiers (Phase 1.5)
// ---------------------------------------------------------------------------

#[cfg(feature = "test-fixtures")]
fn korean_labels() -> Vec<&'static str> {
    vec!["고객", "주문", "상품"]
}

#[test]
#[cfg(feature = "test-fixtures")]
fn schema_view_labels_returns_node_and_edge_lists() {
    let ont = crate::test_fixtures::korean_ecommerce_ontology();
    let view = ont.schema_view(SchemaView::Labels, &korean_labels());
    let nodes = view["nodes"].as_array().expect("nodes array");
    let edges = view["edges"].as_array().expect("edges array");
    assert!(nodes.iter().any(|v| v == "고객"));
    assert!(nodes.iter().any(|v| v == "주문"));
    // 주문함, 포함 should appear because their endpoints are in the selection
    assert!(edges.iter().any(|v| v == "주문함"));
    assert!(edges.iter().any(|v| v == "포함"));
    // No property/type metadata at this tier
    assert!(view.get("properties").is_none());
}

#[test]
#[cfg(feature = "test-fixtures")]
fn schema_view_structural_includes_property_names_no_types() {
    let ont = crate::test_fixtures::korean_ecommerce_ontology();
    let view = ont.schema_view(SchemaView::Structural, &korean_labels());
    let customer_props = view["nodes"]["고객"]["properties"]
        .as_array()
        .expect("customer properties array");
    assert!(customer_props.iter().any(|v| v == "이름"));
    assert!(customer_props.iter().any(|v| v == "이메일"));
    // Structural tier does NOT include type info
    let customer_obj = view["nodes"]["고객"].as_object().expect("object");
    assert!(
        !customer_obj.contains_key("description"),
        "structural tier should be type-free"
    );
}

#[test]
#[cfg(feature = "test-fixtures")]
fn schema_view_detailed_matches_compact_schema() {
    let ont = crate::test_fixtures::korean_ecommerce_ontology();
    let detailed = ont.schema_view(SchemaView::Detailed, &korean_labels());
    let compact = ont.compact_schema(&korean_labels());
    assert_eq!(detailed, compact);
}

// ---------------------------------------------------------------------------
// Palantir-grade semantic field tests (Phase A Day 5)
// ---------------------------------------------------------------------------

#[test]
fn semantic_type_canonical_variants_roundtrip() {
    let email = SemanticType::Email;
    let json = serde_json::to_string(&email).unwrap();
    assert_eq!(json, r#"{"kind":"email"}"#);
    let back: SemanticType = serde_json::from_str(&json).unwrap();
    assert_eq!(back, email);
}

#[test]
fn semantic_type_localized_text_variant() {
    let t = SemanticType::LocalizedText;
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(json, r#"{"kind":"localized_text"}"#);
}

#[test]
fn semantic_type_other_open_extension() {
    let t = SemanticType::Other("isbn".to_string());
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(json, r#"{"kind":"other","value":"isbn"}"#);
    let back: SemanticType = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t);
}

#[test]
fn pii_kind_national_id_carries_country_code() {
    let kr_rrn = PiiKind::NationalId {
        country: "kr".to_string(),
    };
    let json = serde_json::to_string(&kr_rrn).unwrap();
    // Tag + content layout: struct variants nest payload under `value`.
    assert_eq!(
        json,
        r#"{"kind":"national_id","value":{"country":"kr"}}"#
    );
    let back: PiiKind = serde_json::from_str(&json).unwrap();
    assert_eq!(back, kr_rrn);
}

#[test]
fn pii_kind_custom_open_extension() {
    let t = PiiKind::Custom("employee_badge".to_string());
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(json, r#"{"kind":"custom","value":"employee_badge"}"#);
    let back: PiiKind = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t);
}

#[test]
fn pii_kind_hipaa_variants_distinct_from_national_id() {
    assert_ne!(
        PiiKind::MedicalRecordNumber,
        PiiKind::NationalId { country: "us".to_string() },
    );
    let mrn_json = serde_json::to_string(&PiiKind::MedicalRecordNumber).unwrap();
    assert_eq!(mrn_json, r#"{"kind":"medical_record_number"}"#);
}

#[test]
fn property_def_default_has_new_fields_unset() {
    let p = PropertyDef::default();
    assert_eq!(p.min_count, None);
    assert_eq!(p.max_count, None);
    assert!(!p.is_localized);
    assert_eq!(p.deprecated_at, None);
    assert_eq!(p.replaced_by_id, None);
}

#[test]
fn edge_def_default_has_new_fields_unset() {
    let e = EdgeTypeDef::default();
    assert_eq!(e.source_role, None);
    assert_eq!(e.target_role, None);
    assert!(e.tags.is_empty());
    assert_eq!(e.deprecated_at, None);
    assert_eq!(e.replaced_by_id, None);
}

#[test]
fn property_def_roundtrip_with_cardinality_and_localized() {
    let p = PropertyDef {
        id: "p1".into(),
        name: "categories".to_string(),
        property_type: PropertyType::List {
            element: Box::new(PropertyType::String),
        },
        min_count: Some(1),
        max_count: Some(10),
        is_localized: false,
        semantic_type: Some(SemanticType::Other("category_code".to_string())),
        ..Default::default()
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: PropertyDef = serde_json::from_str(&json).unwrap();
    assert_eq!(back.min_count, Some(1));
    assert_eq!(back.max_count, Some(10));
    assert!(!back.is_localized);
    assert_eq!(back.semantic_type, p.semantic_type);
}

#[test]
fn property_def_omits_new_optional_fields_when_unset() {
    let p = PropertyDef {
        id: "p1".into(),
        name: "name".to_string(),
        property_type: PropertyType::String,
        ..Default::default()
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(!json.contains("min_count"));
    assert!(!json.contains("max_count"));
    assert!(!json.contains("is_localized"));
    assert!(!json.contains("deprecated_at"));
    assert!(!json.contains("replaced_by_id"));
}

#[test]
fn edge_def_roundtrip_with_roles_and_tags() {
    let e = EdgeTypeDef {
        id: "e1".into(),
        label: "MANAGES".to_string(),
        source_node_id: "n1".into(),
        target_node_id: "n1".into(),
        source_role: Some("manager".to_string()),
        target_role: Some("direct_report".to_string()),
        tags: vec!["org_chart".to_string(), "derived".to_string()],
        ..Default::default()
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: EdgeTypeDef = serde_json::from_str(&json).unwrap();
    assert_eq!(back.source_role.as_deref(), Some("manager"));
    assert_eq!(back.target_role.as_deref(), Some("direct_report"));
    assert_eq!(back.tags, vec!["org_chart", "derived"]);
}

#[test]
fn node_def_deprecation_fields_roundtrip() {
    let now = chrono::Utc::now();
    let n = NodeTypeDef {
        id: "n1".into(),
        label: "LegacyUser".to_string(),
        deprecated_at: Some(now),
        replaced_by_id: Some(NodeTypeId::new("n2")),
        ..Default::default()
    };
    let json = serde_json::to_string(&n).unwrap();
    let back: NodeTypeDef = serde_json::from_str(&json).unwrap();
    assert!(back.deprecated_at.is_some());
    assert_eq!(back.replaced_by_id, Some(NodeTypeId::new("n2")));
}
