use super::*;
use crate::graph_label::GraphLabel;
use crate::property_key::PropertyKey;
use crate::types::PropertyType;

use crate::i18n::LocalizedText;

fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("test label literal must be valid")
}

fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("test property name literal must be valid")
}

fn property(id: &str, name: &str, nullable: bool) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: pk(name),
        property_type: PropertyType::String,
        nullable,
        default_value: None,
        description: LocalizedText::default(),
        classification: None,
        ..Default::default()
    }
}

#[test]
fn compact_schema_surfaces_localization_cardinality_and_deprecation_hints() {
    let mut ontology = OntologyIR::new(
        "t".into(),
        "Hints".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "n1".into(),
            label: gl("Doc"),
            description: LocalizedText::default(),
            properties: vec![
                PropertyDef {
                    id: "p-title".into(),
                    name: pk("title"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    is_localized: true,
                    min_count: Some(1),
                    ..Default::default()
                },
                PropertyDef {
                    id: "p-tags".into(),
                    name: pk("tags"),
                    property_type: PropertyType::List {
                        element: Box::new(PropertyType::String),
                    },
                    nullable: true,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    min_count: Some(0),
                    max_count: Some(5),
                    ..Default::default()
                },
                PropertyDef {
                    id: "p-old".into(),
                    name: pk("legacy_field"),
                    property_type: PropertyType::String,
                    nullable: true,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    deprecated_at: Some(chrono::Utc::now()),
                    ..Default::default()
                },
            ],
            constraints: vec![],
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    let _ = &mut ontology;
    let schema = ontology.compact_schema(&["Doc"]);
    let doc = schema.pointer("/nodes/Doc/properties").unwrap();
    let title = doc.get("title").and_then(|v| v.as_str()).unwrap();
    let tags = doc.get("tags").and_then(|v| v.as_str()).unwrap();
    let legacy = doc.get("legacy_field").and_then(|v| v.as_str()).unwrap();
    assert!(
        title.contains("localized") && title.contains("min 1"),
        "title should hint localized + min cardinality: {title}"
    );
    assert!(
        tags.contains("min 0") && tags.contains("max 5"),
        "tags should expose cardinality bounds: {tags}"
    );
    assert!(
        legacy.contains("deprecated"),
        "legacy_field should carry deprecated hint: {legacy}"
    );
}

#[test]
fn compact_schema_surfaces_edge_roles_and_deprecation() {
    let mut ontology = OntologyIR::new(
        "t".into(),
        "Edges".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "n1".into(),
                label: gl("Manager"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            },
            NodeTypeDef {
                id: "n2".into(),
                label: gl("Employee"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e1".into(),
            label: gl("MANAGES"),
            description: LocalizedText::new("Reporting relationship"),
            source_node_id: "n1".into(),
            target_node_id: "n2".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
            source_role: Some("manager".into()),
            target_role: Some("direct_report".into()),
            deprecated_at: Some(chrono::Utc::now()),
            ..Default::default()
        }],
        vec![],
    );
    let _ = &mut ontology;
    let schema = ontology.compact_schema(&["Manager", "Employee"]);
    let edge = schema.pointer("/edges/MANAGES").unwrap();
    assert_eq!(
        edge.get("source_role").and_then(|v| v.as_str()),
        Some("manager")
    );
    assert_eq!(
        edge.get("target_role").and_then(|v| v.as_str()),
        Some("direct_report")
    );
    let desc = edge.get("description").and_then(|v| v.as_str()).unwrap();
    assert!(
        desc.contains("[DEPRECATED]"),
        "edge description should carry deprecation marker: {desc}"
    );
}

fn base_ontology() -> OntologyIR {
    OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: gl("User"),
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
            label: gl("OWNS"),
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
        label: gl("Empty"),
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
        label: gl("FOLLOWS"),
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
            label: gl("User"),
            description: LocalizedText::default(),
            properties: vec![property("prop-id", "id", false)],
            constraints: vec![],
            ..Default::default()
        }],
        vec![
            EdgeTypeDef {
                id: "edge-1".into(),
                label: gl("KNOWS"),
                description: LocalizedText::default(),
                source_node_id: "node-user".into(),
                target_node_id: "node-user".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            },
            EdgeTypeDef {
                id: "edge-2".into(),
                label: gl("KNOWS"),
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
            label: gl("Person"),
            description: LocalizedText::default(),
            properties: vec![property("prop-name", "name", false)],
            constraints: vec![],
            ..Default::default()
        }],
        vec![EdgeTypeDef {
            id: "edge-knows".into(),
            label: gl("KNOWS"),
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
    assert_eq!(json, r#"{"kind":"national_id","value":{"country":"kr"}}"#);
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
        PiiKind::NationalId {
            country: "us".to_string()
        },
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
        name: pk("categories"),
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
        name: pk("name"),
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
        label: gl("MANAGES"),
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
        label: gl("LegacyUser"),
        deprecated_at: Some(now),
        replaced_by_id: Some(NodeTypeId::new("n2")),
        ..Default::default()
    };
    let json = serde_json::to_string(&n).unwrap();
    let back: NodeTypeDef = serde_json::from_str(&json).unwrap();
    assert!(back.deprecated_at.is_some());
    assert_eq!(back.replaced_by_id, Some(NodeTypeId::new("n2")));
}

// ---------------------------------------------------------------------------
// unknown_labels_in_query — Brain's pre-flight label check
// ---------------------------------------------------------------------------

fn ecommerce_ontology() -> OntologyIR {
    use crate::ontology_ir::Cardinality;
    OntologyIR::new(
        "ont".into(),
        "Commerce".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "n1".into(),
                label: gl("Customer"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            },
            NodeTypeDef {
                id: "n2".into(),
                label: gl("Order"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e1".into(),
            label: gl("PLACED"),
            description: LocalizedText::default(),
            source_node_id: "n1".into(),
            target_node_id: "n2".into(),
            properties: vec![],
            cardinality: Cardinality::OneToMany,
            ..Default::default()
        }],
        vec![],
    )
}

fn simple_match_query(node_label: &str, rel_label: Option<&str>) -> crate::query_ir::QueryIR {
    use crate::query_ir::{GraphPattern, QUERY_IR_SCHEMA_VERSION, QueryIR, QueryOp};
    let mut patterns = vec![GraphPattern::Node {
        variable: crate::variable_name::VariableName::new("n").expect("valid"),
        label: Some(node_label.into()),
        property_filters: vec![],
    }];
    if let Some(r) = rel_label {
        patterns.push(GraphPattern::Relationship {
            variable: None,
            label: Some(r.into()),
            source: crate::variable_name::VariableName::new("n").expect("valid"),
            target: crate::variable_name::VariableName::new("m").expect("valid"),
            direction: crate::types::Direction::Outgoing,
            property_filters: vec![],
            var_length: None,
        });
        patterns.push(GraphPattern::Node {
            variable: crate::variable_name::VariableName::new("m").expect("valid"),
            label: Some(node_label.into()),
            property_filters: vec![],
        });
    }
    QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns,
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    }
}

#[test]
fn unknown_labels_in_query_empty_when_clean() {
    let ontology = ecommerce_ontology();
    let query = simple_match_query("Customer", Some("PLACED"));
    assert!(ontology.unknown_labels_in_query(&query).is_empty());
}

#[test]
fn unknown_labels_in_query_flags_node_miss() {
    let ontology = ecommerce_ontology();
    let query = simple_match_query("Userr", None); // typo
    let unknown = ontology.unknown_labels_in_query(&query);
    assert_eq!(unknown.len(), 1);
    assert!(unknown[0].contains("Userr"));
    assert!(unknown[0].starts_with("Node"));
}

#[test]
fn unknown_labels_in_query_flags_edge_miss() {
    let ontology = ecommerce_ontology();
    let query = simple_match_query("Customer", Some("BOUGHT")); // unknown rel
    let unknown = ontology.unknown_labels_in_query(&query);
    assert!(
        unknown
            .iter()
            .any(|u| u.contains("BOUGHT") && u.starts_with("Edge"))
    );
}

#[test]
fn unknown_labels_in_query_flags_both_independently() {
    let ontology = ecommerce_ontology();
    let query = simple_match_query("Userr", Some("BOUGHT"));
    let unknown = ontology.unknown_labels_in_query(&query);
    assert!(unknown.len() >= 2);
    assert!(unknown.iter().any(|u| u.contains("Userr")));
    assert!(unknown.iter().any(|u| u.contains("BOUGHT")));
}

// ---------------------------------------------------------------------------
// schema_version forward-compat gate
// ---------------------------------------------------------------------------

#[test]
fn ontology_ir_rejects_future_schema_version() {
    // Payload tags itself as a version higher than this build supports —
    // the deserializer must refuse rather than silently drop future
    // fields it doesn't know how to parse.
    let blob = serde_json::json!({
        "schema_version": ONTOLOGY_IR_SCHEMA_VERSION + 1,
        "id": "ont",
        "name": "Test",
        "version": { "number": 1 },
        "node_types": [],
        "edge_types": [],
        "indexes": [],
    });
    let err = serde_json::from_value::<OntologyIR>(blob).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("schema_version") && msg.contains("newer"),
        "error should name the version skew: {msg}"
    );
}

#[test]
fn ontology_ir_accepts_missing_schema_version_as_current() {
    // Legacy JSONB rows that predate this field must still load —
    // serde(default = ...) hands back ONTOLOGY_IR_SCHEMA_VERSION.
    let blob = serde_json::json!({
        "id": "ont",
        "name": "Legacy",
        "version": { "number": 1 },
        "node_types": [],
        "edge_types": [],
        "indexes": [],
    });
    let onto: OntologyIR = serde_json::from_value(blob).expect("legacy payload must parse");
    assert_eq!(onto.schema_version, ONTOLOGY_IR_SCHEMA_VERSION);
}
