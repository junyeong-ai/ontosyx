use super::*;
use ox_core::graph_label::GraphLabel;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;

use ox_core::i18n::LocalizedText;

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

// NOTE: unknown_labels_in_query tests moved to
// `crates/ox-query-ir/tests/ontology_conformance.rs` in Phase 3-B
// (the helper crosses the ontology × query boundary and lives in
// the downstream crate that owns QueryIR).

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

// ---------------------------------------------------------------------------
// Phase 5-B — semantic superstructure wiring
// ---------------------------------------------------------------------------

#[test]
fn v1_payload_deserialises_with_empty_superstructure_collections() {
    // Backwards compatibility: a pre-5-B JSONB snapshot lacks every
    // new collection. Every new field carries `#[serde(default)]` so
    // the payload must still parse and every collection must come
    // back empty rather than failing to round-trip.
    let blob = serde_json::json!({
        "schema_version": 1,
        "id": "ont",
        "name": "Legacy",
        "version": { "number": 1 },
        "node_types": [],
        "edge_types": [],
        "indexes": [],
    });
    let onto: OntologyIR = serde_json::from_value(blob).expect("v1 payload must parse");
    assert!(onto.interfaces().is_empty());
    assert!(onto.rules().is_empty());
    assert!(onto.actions().is_empty());
    assert!(onto.functions().is_empty());
    assert!(onto.metrics().is_empty());
    assert!(onto.enrichments().is_empty());
    assert!(onto.glossary().is_empty());
    assert!(onto.data_quality().is_empty());
    assert!(onto.provenance().is_empty());
}

#[test]
fn superstructure_add_methods_populate_the_public_accessors() {
    // Round-trip every add_*/accessor pair through one ontology to
    // prove they are wired symmetrically.
    let mut onto = OntologyIR::new(
        "ont-1".into(),
        "Sample".into(),
        LocalizedText::default(),
        1,
        vec![],
        vec![],
        vec![],
    );

    onto.add_interface(crate::interface::InterfaceDef {
        id: crate::interface::InterfaceId::new("if-1"),
        label: gl("HasAddress"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        required_properties: vec![],
        required_edges: vec![],
    }).unwrap();

    onto.add_glossary_term(crate::glossary::GlossaryTermDef {
        id: crate::glossary::GlossaryTermId::new("gt-customer"),
        term: "Customer".into(),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        category: None,
        aliases: vec!["Client".into()],
        parent_term_id: None,
    }).unwrap();

    assert_eq!(onto.interfaces().len(), 1);
    assert_eq!(onto.glossary().len(), 1);
    assert_eq!(onto.interfaces()[0].label.as_str(), "HasAddress");
    assert!(onto.glossary()[0].matches_text("client"));
}

#[test]
fn property_def_carries_phase_5b_semantic_links_through_json() {
    use crate::function::FunctionId;
    use crate::glossary::GlossaryTermId;

    let p = PropertyDef {
        id: "prop-email".into(),
        name: pk("email"),
        property_type: PropertyType::String,
        nullable: false,
        default_value: None,
        description: LocalizedText::default(),
        glossary_term_id: Some(GlossaryTermId::new("gt-contact-email")),
        aliases: vec![LocalizedText::new("e-mail")],
        business_context: LocalizedText::new("contact address; source CRM v3"),
        derived_from: Some(FunctionId::new("fn-lowercase-email")),
        ..Default::default()
    };
    let j = serde_json::to_value(&p).unwrap();
    let back: PropertyDef = serde_json::from_value(j).unwrap();
    assert_eq!(back.glossary_term_id, p.glossary_term_id);
    assert_eq!(back.aliases.len(), 1);
    assert_eq!(back.derived_from, p.derived_from);
}

#[test]
fn node_type_def_carries_phase_5b_semantic_links_through_json() {
    use crate::action::{ActionId, RuleId};
    use crate::interface::InterfaceId;
    use crate::metric::MetricId;

    let n = NodeTypeDef {
        id: "nt-customer".into(),
        label: gl("Customer"),
        implements: vec![InterfaceId::new("if-has-address")],
        actions: vec![ActionId::new("act-upsert-customer")],
        metrics: vec![MetricId::new("m-customer-count")],
        rules: vec![RuleId::new("r-email-required")],
        ..Default::default()
    };
    let j = serde_json::to_value(&n).unwrap();
    let back: NodeTypeDef = serde_json::from_value(j).unwrap();
    assert_eq!(back.implements.len(), 1);
    assert_eq!(back.actions.len(), 1);
    assert_eq!(back.metrics.len(), 1);
    assert_eq!(back.rules.len(), 1);
    assert_eq!(back.implements[0].as_str(), "if-has-address");
}

// ---------------------------------------------------------------------------
// Phase 5-C — referential-integrity validation
// ---------------------------------------------------------------------------

fn minimal_node(id: &str, label: &str) -> NodeTypeDef {
    NodeTypeDef {
        id: id.into(),
        label: gl_dynamic(label),
        description: LocalizedText::default(),
        properties: vec![],
        constraints: vec![],
        ..Default::default()
    }
}

fn gl_dynamic(s: &str) -> GraphLabel {
    GraphLabel::new(s).expect("test label")
}

#[test]
fn validate_flags_node_implementing_unknown_interface() {
    use crate::interface::InterfaceId;

    let mut n = minimal_node("nt-1", "Customer");
    n.implements = vec![InterfaceId::new("if-ghost")];
    let onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![n],
        vec![],
        vec![],
    );
    let errs = onto.validate();
    assert!(
        errs.iter().any(|e| e.contains("if-ghost") && e.contains("unknown interface")),
        "validator must flag dangling implements: {errs:?}"
    );
}

#[test]
fn validate_flags_property_derived_from_unknown_function() {
    use crate::function::FunctionId;

    let prop = PropertyDef {
        id: "p-1".into(),
        name: pk("total"),
        property_type: PropertyType::Float,
        derived_from: Some(FunctionId::new("fn-ghost")),
        ..Default::default()
    };
    let n = NodeTypeDef {
        id: "nt-1".into(),
        label: gl_dynamic("Order"),
        properties: vec![prop],
        ..Default::default()
    };
    let onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![n],
        vec![],
        vec![],
    );
    let errs = onto.validate();
    assert!(
        errs.iter().any(|e| e.contains("derived_from") && e.contains("fn-ghost")),
        "validator must flag dangling derived_from: {errs:?}"
    );
}

#[test]
fn validate_flags_property_with_both_derived_from_and_source_column() {
    use crate::function::{FunctionDef, FunctionExpression, FunctionId, FunctionPurity};

    let prop = PropertyDef {
        id: "p-1".into(),
        name: pk("total"),
        property_type: PropertyType::Float,
        source_column: Some("total_raw".into()),
        derived_from: Some(FunctionId::new("fn-total")),
        ..Default::default()
    };
    let n = NodeTypeDef {
        id: "nt-1".into(),
        label: gl_dynamic("Order"),
        properties: vec![prop],
        ..Default::default()
    };
    let mut onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![n],
        vec![],
        vec![],
    );
    // Satisfy the derived_from reference so the check we care about
    // — mutual exclusion — is the one that fires.
    onto.add_function(FunctionDef {
        id: FunctionId::new("fn-total"),
        name: "total".into(),
        description: LocalizedText::default(),
        expression: FunctionExpression::SqlExpr {
            expression: "qty * price".into(),
        },
        return_type: PropertyType::Float,
        purity: FunctionPurity::Pure,
        property_dependencies: vec![],
        edge_dependencies: vec![],
    }).unwrap();
    let errs = onto.validate();
    assert!(
        errs.iter()
            .any(|e| e.contains("both") && e.contains("derived_from") && e.contains("source_column")),
        "validator must flag derived_from + source_column collision: {errs:?}"
    );
}

#[test]
fn validate_flags_action_referencing_unknown_rule() {
    use crate::action::{ActionDef, ActionId, ActionKind, ActionTarget, RuleId};

    let onto = {
        let mut o = OntologyIR::new(
            "ont".into(),
            "X".into(),
            LocalizedText::default(),
            1,
            vec![minimal_node("nt-1", "Order")],
            vec![],
            vec![],
        );
        o.add_action(ActionDef {
            id: ActionId::new("act-create"),
            name: "create_order".into(),
            description: LocalizedText::default(),
            target: ActionTarget::NodeType {
                node_type_id: "nt-1".into(),
            },
            kind: ActionKind::Create,
            preconditions: vec![RuleId::new("r-ghost")],
            postconditions: vec![],
            idempotency: Default::default(),
            approval: Default::default(),
        }).unwrap();
        o
    };
    let errs = onto.validate();
    assert!(
        errs.iter().any(|e| e.contains("act-create") || e.contains("create_order"))
            && errs.iter().any(|e| e.contains("r-ghost")),
        "validator must flag unknown rule in action.preconditions: {errs:?}"
    );
}

#[test]
fn validate_passes_when_all_phase_5b_references_resolve() {
    use crate::action::{ActionDef, ActionId, ActionKind, ActionTarget, RuleId};
    use crate::function::{FunctionDef, FunctionExpression, FunctionId, FunctionPurity};
    use crate::glossary::{GlossaryTermDef, GlossaryTermId};
    use crate::interface::{InterfaceDef, InterfaceId};
    use crate::rule::{ConstraintTarget, RuleDef, RuleKind, ShaclConstraint};

    let prop = PropertyDef {
        id: "p-email".into(),
        name: pk("email"),
        property_type: PropertyType::String,
        nullable: false,
        glossary_term_id: Some(GlossaryTermId::new("gt-email")),
        derived_from: Some(FunctionId::new("fn-lower")),
        ..Default::default()
    };
    let mut node = minimal_node("nt-user", "User");
    node.properties = vec![prop];
    node.implements = vec![InterfaceId::new("if-contactable")];
    node.actions = vec![ActionId::new("act-create-user")];
    node.rules = vec![RuleId::new("r-email-required")];

    let mut onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![node],
        vec![],
        vec![],
    );

    onto.add_interface(InterfaceDef {
        id: InterfaceId::new("if-contactable"),
        label: gl_dynamic("Contactable"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        required_properties: vec![],
        required_edges: vec![],
    }).unwrap();
    onto.add_action(ActionDef {
        id: ActionId::new("act-create-user"),
        name: "create_user".into(),
        description: LocalizedText::default(),
        target: ActionTarget::NodeType {
            node_type_id: "nt-user".into(),
        },
        kind: ActionKind::Create,
        preconditions: vec![RuleId::new("r-email-required")],
        postconditions: vec![],
        idempotency: Default::default(),
        approval: Default::default(),
    }).unwrap();
    onto.add_rule(RuleDef {
        id: RuleId::new("r-email-required"),
        name: "email_required".into(),
        description: LocalizedText::default(),
        kind: RuleKind::PropertyShape {
            target_node_type_id: "nt-user".into(),
            target_property_id: "p-email".into(),
        },
        severity: Default::default(),
        enforcement: Default::default(),
        activation: Default::default(),
        constraints: vec![ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 1,
        }],
    }).unwrap();
    onto.add_function(FunctionDef {
        id: FunctionId::new("fn-lower"),
        name: "lower".into(),
        description: LocalizedText::default(),
        expression: FunctionExpression::SqlExpr {
            expression: "LOWER(email)".into(),
        },
        return_type: PropertyType::String,
        purity: FunctionPurity::Pure,
        property_dependencies: vec![],
        edge_dependencies: vec![],
    }).unwrap();
    onto.add_glossary_term(GlossaryTermDef {
        id: GlossaryTermId::new("gt-email"),
        term: "Contact Email".into(),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        category: Some("contact".into()),
        aliases: vec!["E-mail".into()],
        parent_term_id: None,
    }).unwrap();

    let errs = onto.validate();
    assert!(
        errs.is_empty(),
        "fully-resolved ontology should validate clean: {errs:?}"
    );
}

// ---------------------------------------------------------------------------
// Phase 5-D — lookup indices + by_id() accessors
// ---------------------------------------------------------------------------

#[test]
fn by_id_accessors_resolve_against_rebuilt_indices() {
    use crate::action::{ActionDef, ActionId, ActionKind, ActionTarget};
    use crate::interface::{InterfaceDef, InterfaceId};

    let mut onto = OntologyIR::new(
        "ont".into(),
        "Sample".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-1".into(),
            label: gl("Customer"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );

    onto.add_interface(InterfaceDef {
        id: InterfaceId::new("if-1"),
        label: gl("HasAddress"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        required_properties: vec![],
        required_edges: vec![],
    })
    .unwrap();
    onto.add_action(ActionDef {
        id: ActionId::new("act-1"),
        name: "create".into(),
        description: LocalizedText::default(),
        target: ActionTarget::NodeType {
            node_type_id: "nt-1".into(),
        },
        kind: ActionKind::Create,
        preconditions: vec![],
        postconditions: vec![],
        idempotency: Default::default(),
        approval: Default::default(),
    })
    .unwrap();

    // Positive lookup.
    assert_eq!(
        onto.interface_by_id(&InterfaceId::new("if-1"))
            .unwrap()
            .label
            .as_str(),
        "HasAddress",
    );
    assert_eq!(
        onto.action_by_id(&ActionId::new("act-1")).unwrap().name,
        "create",
    );
    // Negative lookup returns None (not an error).
    assert!(onto.interface_by_id(&InterfaceId::new("if-ghost")).is_none());
}

#[test]
fn add_interface_rejects_duplicate_id() {
    use crate::interface::{InterfaceDef, InterfaceId};

    let mut onto = OntologyIR::new(
        "ont".into(),
        "Sample".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-1".into(),
            label: gl("X"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    let first = InterfaceDef {
        id: InterfaceId::new("if-1"),
        label: gl("HasAddress"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        required_properties: vec![],
        required_edges: vec![],
    };
    let second = first.clone();
    onto.add_interface(first).unwrap();
    let err = onto
        .add_interface(second)
        .expect_err("second insert must be rejected");
    match err {
        OntologyInvariantError::DuplicateCollectionId { kind, id } => {
            assert_eq!(kind, "interface");
            assert_eq!(id, "if-1");
        }
        other => panic!("expected DuplicateCollectionId, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Ω-1 terminology registry
// ---------------------------------------------------------------------------

#[test]
fn add_code_system_round_trips_through_by_id_lookup() {
    use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId};

    let mut onto = OntologyIR::new(
        "t".into(),
        "t".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-1".into(),
            label: gl("X"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    let system = CodeSystemDef {
        id: CodeSystemId::new("cs-order-status"),
        name: "OrderStatus".into(),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        uri: Some("urn:ox:order-status".into()),
        version: "1".into(),
        kind: CodeSystemKind::Internal,
        hierarchical: false,
        codes: vec![CodedValue {
            id: CodedValueId::new("cv-active"),
            code: "A".into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec!["ACT".into()],
            broader_id: None,
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }],
        deprecated_at: None,
        replaced_by_id: None,
    };
    onto.add_code_system(system.clone()).unwrap();

    // code_system_by_id hits the fast-path index.
    let back = onto
        .code_system_by_id(&CodeSystemId::new("cs-order-status"))
        .expect("round-trip");
    assert_eq!(back.name, "OrderStatus");

    // coded_value_by_id crosses system boundary — global index.
    let (_sys, cv) = onto
        .coded_value_by_id(&CodedValueId::new("cv-active"))
        .expect("coded value present");
    assert_eq!(cv.code, "A");
}

#[test]
fn add_code_system_rejects_duplicate_coded_value_id_across_systems() {
    use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId};

    let mut onto = OntologyIR::new(
        "t".into(),
        "t".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-1".into(),
            label: gl("X"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    // Two systems each carrying a code with the same CodedValueId.
    let make_sys = |sys_id: &str, cv_id: &str| CodeSystemDef {
        id: CodeSystemId::new(sys_id),
        name: sys_id.into(),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        uri: None,
        version: "1".into(),
        kind: CodeSystemKind::Internal,
        hierarchical: false,
        codes: vec![CodedValue {
            id: CodedValueId::new(cv_id),
            code: "A".into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec![],
            broader_id: None,
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }],
        deprecated_at: None,
        replaced_by_id: None,
    };

    onto.add_code_system(make_sys("cs-1", "cv-shared")).unwrap();
    let err = onto
        .add_code_system(make_sys("cs-2", "cv-shared"))
        .expect_err("duplicate CodedValueId must be rejected");
    match err {
        OntologyInvariantError::DuplicateCollectionId { kind, id } => {
            assert_eq!(kind, "coded_value");
            assert_eq!(id, "cv-shared");
        }
        other => panic!("expected DuplicateCollectionId, got {other:?}"),
    }
}
