use super::*;
use ox_core::graph_label::GraphLabel;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;

use ox_core::i18n::LocalizedText;

// Shared fixture helpers. `property_nullable` / `sample_user_ontology`
// live in `crate::test_fixtures` so this file and the sibling
// validation-tests module both draw from one source of truth —
// diverging the two ontology fixtures in the past has caused tests
// to silently pass on one file's shape while failing on the other.
use crate::test_fixtures::{property_nullable as property, sample_user_ontology};

fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("test label literal must be valid")
}

fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("test property name literal must be valid")
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

// Renamed local alias of the shared fixture — keeps call sites in
// this file reading as `base_ontology()` (a widely-used name in the
// tests below) while pointing to the single authoritative builder.
fn base_ontology() -> OntologyIR {
    sample_user_ontology()
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
        errors2.iter().any(|e| e.message.contains("Duplicate edge type")),
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
fn ontology_ir_omits_schema_version_falls_back_to_current() {
    // Payloads that omit `schema_version` decode at the current
    // version — `serde(default = ...)` hands back
    // `ONTOLOGY_IR_SCHEMA_VERSION` and every superstructure
    // collection defaults to empty.
    let blob = serde_json::json!({
        "id": "ont",
        "name": "Sample",
        "version": { "number": 1 },
        "node_types": [],
        "edge_types": [],
        "indexes": [],
    });
    let onto: OntologyIR = serde_json::from_value(blob).expect("payload must parse");
    assert_eq!(onto.schema_version, ONTOLOGY_IR_SCHEMA_VERSION);
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
        term: LocalizedText::new("Customer"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: vec![LocalizedText::new("Client")],
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: crate::glossary::TermLifecycle::default(),
    concept_id: None,
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
        bindings: vec![crate::binding::PropertyBinding::glossary(GlossaryTermId::new("gt-contact-email"),)],
        aliases: vec![LocalizedText::new("e-mail")],
        business_context: LocalizedText::new("contact address; source CRM v3"),
        derived_from: Some(FunctionId::new("fn-lowercase-email")),
        ..Default::default()
    };
    let j = serde_json::to_value(&p).unwrap();
    let back: PropertyDef = serde_json::from_value(j).unwrap();
    assert_eq!(back.glossary_term_id(), p.glossary_term_id());
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
// Referential-integrity validation
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
        errs.iter().any(|e| e.message.contains("if-ghost") && e.message.contains("unknown interface")),
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
        errs.iter().any(|e| e.message.contains("derived_from") && e.message.contains("fn-ghost")),
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
            .any(|e| e.message.contains("both") && e.message.contains("derived_from") && e.message.contains("source_column")),
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
        errs.iter().any(|e| e.message.contains("act-create") || e.message.contains("create_order"))
            && errs.iter().any(|e| e.message.contains("r-ghost")),
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
        bindings: vec![crate::binding::PropertyBinding::glossary(GlossaryTermId::new("gt-email"),)],
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
            rationale: LocalizedText::default(),
        kind: RuleKind::PropertyShape {
            target_node_type_id: "nt-user".into(),
            target_property_id: "p-email".into(),
        },
        severity: Default::default(),
        enforcement: Default::default(),
        activation: Default::default(),
        origin: Default::default(),
        constraints: vec![ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 1,
        }],
        valid_from: None,
        valid_to: None,
            sh_message: None,
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
        term: LocalizedText::new("Contact Email"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: Some("contact".into()),
        aliases: vec![LocalizedText::new("E-mail")],
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: crate::glossary::TermLifecycle::default(),
    concept_id: None,
    }).unwrap();

    let errs = onto.validate();
    assert!(
        errs.is_empty(),
        "fully-resolved ontology should validate clean: {errs:?}"
    );
}

// ---------------------------------------------------------------------------
// Lookup indices + by_id() accessors
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

#[test]
fn as_of_drops_rules_outside_window() {
    use chrono::{Duration, Utc};

    let now = Utc::now();
    let past = now - Duration::hours(2);
    let future = now + Duration::hours(2);

    let mut ontology = sample_user_ontology();

    // Active rule (no window) + a rule that only kicked in last hour.
    let always = crate::rule::RuleDef {
        id: crate::action::RuleId::new("r-always"),
        name: "always".into(),
        description: LocalizedText::default(),
        rationale: LocalizedText::default(),
        kind: crate::rule::RuleKind::CrossEntityShape {
            predicate: "1=1".into(),
        },
        severity: crate::rule::Severity::default(),
        enforcement: crate::rule::EnforcementKind::default(),
        activation: crate::rule::RuleActivationKind::default(),
        origin: crate::rule::RuleOrigin::default(),
        constraints: Vec::new(),
        valid_from: None,
        valid_to: None,
            sh_message: None,
    };
    let recent = crate::rule::RuleDef {
        id: crate::action::RuleId::new("r-recent"),
        valid_from: Some(now - Duration::hours(1)),
        valid_to: None,
        ..always.clone()
    };
    let stale = crate::rule::RuleDef {
        id: crate::action::RuleId::new("r-stale"),
        valid_from: None,
        valid_to: Some(past),
        ..always.clone()
    };
    ontology.add_rule(always).unwrap();
    ontology.add_rule(recent).unwrap();
    ontology.add_rule(stale).unwrap();

    let snapshot_now = ontology.as_of(now).unwrap();
    let ids_now: std::collections::BTreeSet<&str> = snapshot_now
        .rules()
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert!(ids_now.contains("r-always"));
    assert!(ids_now.contains("r-recent"));
    assert!(!ids_now.contains("r-stale"), "stale rule must be filtered");

    let snapshot_past = ontology.as_of(past - Duration::seconds(1)).unwrap();
    let ids_past: std::collections::BTreeSet<&str> = snapshot_past
        .rules()
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert!(ids_past.contains("r-always"));
    assert!(ids_past.contains("r-stale"));
    assert!(
        !ids_past.contains("r-recent"),
        "recent rule must not appear before its valid_from"
    );

    let snapshot_future = ontology.as_of(future).unwrap();
    let ids_future: std::collections::BTreeSet<&str> = snapshot_future
        .rules()
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert!(ids_future.contains("r-always"));
    assert!(ids_future.contains("r-recent"));
    assert!(!ids_future.contains("r-stale"));
}

#[test]
fn as_of_drops_property_bindings_outside_window() {
    use chrono::{Duration, Utc};

    let now = Utc::now();
    let past = now - Duration::hours(2);

    let mut ontology = sample_user_ontology();
    let vs_id = crate::value_set::ValueSetId::new("vs-x");
    ontology
        .add_value_set(crate::value_set::ValueSetDef {
            id: vs_id.clone(),
            name: "x".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: Vec::new(),
        })
        .unwrap();

    // Stamp a stale binding on the first property of the first node.
    let stale = crate::binding::PropertyBinding::value_set(vs_id).with_valid_to(past);
    ontology
        .node_types_mut()
        .iter_mut()
        .next()
        .expect("node")
        .properties
        .iter_mut()
        .next()
        .expect("property")
        .bindings
        .push(stale);
    ontology.rebuild_indices().unwrap();

    let snapshot = ontology.as_of(now).unwrap();
    let prop = &snapshot.node_types()[0].properties[0];
    assert!(
        prop.value_set_binding().is_none(),
        "stale binding must be filtered out at as_of(now)"
    );
}

#[test]
fn required_binding_to_empty_value_set_is_rejected() {
    let mut ontology = sample_user_ontology();
    let vs_id = crate::value_set::ValueSetId::new("vs-empty");
    ontology
        .add_value_set(crate::value_set::ValueSetDef {
            id: vs_id.clone(),
            name: "empty".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            // Empty composition → Required binding must reject.
            composition: Vec::new(),
        })
        .unwrap();

    let required = crate::binding::PropertyBinding::value_set(vs_id)
        .with_strength(crate::binding::BindingStrength::Required);
    ontology
        .node_types_mut()
        .iter_mut()
        .next()
        .expect("node")
        .properties
        .iter_mut()
        .next()
        .expect("property")
        .bindings
        .push(required);
    ontology.rebuild_indices().unwrap();

    let errors = ontology.validate();
    assert!(
        errors.iter().any(|e| e.message.contains("Required binding to value set")
            && e.message.contains("is empty")),
        "expected empty-value-set rejection, got {errors:?}"
    );
}

#[test]
fn preferred_binding_to_empty_value_set_is_accepted() {
    let mut ontology = sample_user_ontology();
    let vs_id = crate::value_set::ValueSetId::new("vs-empty");
    ontology
        .add_value_set(crate::value_set::ValueSetDef {
            id: vs_id.clone(),
            name: "empty".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: Vec::new(),
        })
        .unwrap();

    let preferred = crate::binding::PropertyBinding::value_set(vs_id);
    ontology
        .node_types_mut()
        .iter_mut()
        .next()
        .expect("node")
        .properties
        .iter_mut()
        .next()
        .expect("property")
        .bindings
        .push(preferred);
    ontology.rebuild_indices().unwrap();

    let errors = ontology.validate();
    assert!(
        !errors.iter().any(|e| e.message.contains("Required binding")),
        "Preferred binding should not surface Required-only diagnostic: {errors:?}"
    );
}

#[test]
fn cardinality_numeric_accessors_match_canonical_intent() {
    use crate::ir::Cardinality;

    // OneToOne — both sides singular.
    assert_eq!(Cardinality::OneToOne.source_min(), 1);
    assert_eq!(Cardinality::OneToOne.source_max(), 1);
    assert_eq!(Cardinality::OneToOne.target_min(), 1);
    assert_eq!(Cardinality::OneToOne.target_max(), 1);
    assert!(Cardinality::OneToOne.source_is_singular());
    assert!(Cardinality::OneToOne.target_is_singular());

    // OneToMany — source singular, target many.
    assert_eq!(Cardinality::OneToMany.source_max(), 1);
    assert_eq!(Cardinality::OneToMany.target_max(), u32::MAX);
    assert!(Cardinality::OneToMany.source_is_singular());
    assert!(!Cardinality::OneToMany.target_is_singular());

    // ManyToOne — source many, target singular.
    assert_eq!(Cardinality::ManyToOne.source_max(), u32::MAX);
    assert_eq!(Cardinality::ManyToOne.target_max(), 1);
    assert!(!Cardinality::ManyToOne.source_is_singular());
    assert!(Cardinality::ManyToOne.target_is_singular());

    // ManyToMany — both sides many.
    assert_eq!(Cardinality::ManyToMany.source_max(), u32::MAX);
    assert_eq!(Cardinality::ManyToMany.target_max(), u32::MAX);
    assert!(!Cardinality::ManyToMany.source_is_singular());
    assert!(!Cardinality::ManyToMany.target_is_singular());
}

#[test]
fn advisories_flag_required_dedup_between_node_constraint_and_rule() {
    use crate::action::RuleId;
    use crate::ir::{ConstraintDef, NodeConstraint, NodeTypeDef, PropertyDef};
    use crate::rule::{
        ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, RuleOrigin,
        Severity, ShaclConstraint,
    };

    let prop = PropertyDef {
        id: "p-name".into(),
        name: pk("name"),
        property_type: PropertyType::String,
        nullable: false,
        ..Default::default()
    };
    let mut node = NodeTypeDef {
        id: "nt-x".into(),
        label: gl("X"),
        description: LocalizedText::default(),
        properties: vec![prop],
        constraints: vec![ConstraintDef {
            id: "c-1".into(),
            constraint: NodeConstraint::Exists {
                property_id: "p-name".into(),
            },
        }],
        ..Default::default()
    };
    let rule_id = RuleId::new("r-also-required");
    node.rules.push(rule_id.clone());

    let mut ontology = OntologyIR::try_new(
        "ont".into(),
        "DedupAdvisory".into(),
        LocalizedText::default(),
        1u32,
        vec![node],
        Vec::new(),
        Vec::new(),
    )
    .expect("ir");

    ontology
        .add_rule(RuleDef {
            id: rule_id,
            name: "also_required".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: "nt-x".into(),
                target_property_id: "p-name".into(),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            }],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule");

    // Structurally sound — no `validate()` errors.
    assert!(ontology.validate().is_empty(), "validation must pass");

    // But the dedup advisory fires.
    let advisories = ontology.advisories();
    assert_eq!(
        advisories.len(),
        1,
        "expected one dedup advisory, got {advisories:?}"
    );
    assert!(
        advisories[0].message.contains("Required") && advisories[0].message.contains("source of truth"),
        "advisory text shape: {}",
        advisories[0]
    );
}

#[test]
fn advisories_silent_when_only_one_surface_carries_required() {
    use crate::ir::{ConstraintDef, NodeConstraint, NodeTypeDef, PropertyDef};

    let ontology = OntologyIR::try_new(
        "ont".into(),
        "NoDup".into(),
        LocalizedText::default(),
        1u32,
        vec![NodeTypeDef {
            id: "nt-x".into(),
            label: gl("X"),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "p-name".into(),
                name: pk("name"),
                property_type: PropertyType::String,
                nullable: false,
                ..Default::default()
            }],
            constraints: vec![ConstraintDef {
                id: "c-1".into(),
                constraint: NodeConstraint::Exists {
                    property_id: "p-name".into(),
                },
            }],
            ..Default::default()
        }],
        Vec::new(),
        Vec::new(),
    )
    .expect("ir");

    assert!(ontology.advisories().is_empty());
}

#[test]
fn required_binding_synthesises_inflight_shacl_rule() {
    use crate::derived_rules::DERIVED_BINDING_RULE_PREFIX;
    use crate::ir::{NodeTypeDef, PropertyDef};
    use crate::rule::{ConstraintTarget, RuleKind, ShaclConstraint};

    let mut ontology = OntologyIR::try_new(
        "ont".into(),
        "RequiredBinding".into(),
        LocalizedText::default(),
        1u32,
        vec![NodeTypeDef {
            id: "nt-x".into(),
            label: gl("X"),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "p-status".into(),
                name: pk("status"),
                property_type: PropertyType::String,
                nullable: false,
                ..Default::default()
            }],
            constraints: Vec::new(),
            ..Default::default()
        }],
        Vec::new(),
        Vec::new(),
    )
    .expect("ir");

    // Seed value-set so the binding's referential check passes.
    let vs_id = crate::value_set::ValueSetId::new("vs-status");
    ontology
        .add_code_system(crate::code_system::CodeSystemDef {
            id: crate::code_system::CodeSystemId::new("cs-status"),
            name: "cs".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: crate::code_system::CodeSystemKind::Internal,
            hierarchical: false,
            codes: vec![crate::code_system::CodedValue {
                id: crate::code_system::CodedValueId::new("cv-1"),
                code: "ACTIVE".into(),
                display: LocalizedText::default(),
                definition: LocalizedText::default(),
                aliases: Vec::new(),
                broader_id: None,
                examples: Vec::new(),
                scope_note: LocalizedText::default(),
                valid_from: None,
                valid_to: None,
                deprecated_at: None,
                replaced_by_id: None,
            }],
            deprecated_at: None,
            replaced_by_id: None,
        })
        .unwrap();
    ontology
        .add_value_set(crate::value_set::ValueSetDef {
            id: vs_id.clone(),
            name: "status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![crate::value_set::ValueSetIncludeRule {
                mode: crate::value_set::IncludeMode::Include,
                system_id: crate::code_system::CodeSystemId::new("cs-status"),
                selector: crate::value_set::ValueSetSelector::All,
            }],
        })
        .unwrap();

    // Required binding — no authored rule.
    ontology
        .node_types_mut()
        .iter_mut()
        .next()
        .unwrap()
        .properties[0]
        .bindings
        .push(
            crate::binding::PropertyBinding::value_set(vs_id.clone())
                .with_strength(crate::binding::BindingStrength::Required),
        );
    ontology.rebuild_indices().unwrap();

    // The advisory layer no longer flags this — the derived rule
    // closes the gap.
    assert!(ontology.advisories().is_empty());

    // The IR synthesises a SHACL rule that mirrors the binding so the
    // runtime SHACL validator enforces it without the author copying
    // the constraint into a separate `RuleDef`.
    let derived = ontology.derive_binding_rules();
    assert_eq!(derived.len(), 1);
    let rule = &derived[0];
    assert!(
        rule.id.as_str().starts_with(DERIVED_BINDING_RULE_PREFIX),
        "expected derived prefix, got {}",
        rule.id.as_str(),
    );
    assert!(matches!(
        &rule.kind,
        RuleKind::PropertyShape {
            target_node_type_id,
            target_property_id,
        }
        if target_node_type_id.as_str() == "nt-x"
            && target_property_id.as_str() == "p-status"
    ));
    assert!(matches!(
        rule.constraints.as_slice(),
        [ShaclConstraint::InValueSet {
            target: ConstraintTarget::Inherit,
            value_set_id,
        }] if value_set_id.as_str() == vs_id.as_str()
    ));
}

#[test]
fn add_glossary_term_rejects_self_replacement() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    let mut onto = sample_user_ontology();
    let id = GlossaryTermId::new("gt-self");
    let term = GlossaryTermDef {
        id: id.clone(),
        term: LocalizedText::new("Self"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: Vec::new(),
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::Deprecated {
            replaced_by: Some(id.clone()),
            deprecated_at: Utc::now(),
        },
        concept_id: None,
    };
    let err = onto.add_glossary_term(term).unwrap_err();
    assert!(matches!(
        err,
        OntologyInvariantError::InvalidReference {
            kind: "glossary_term.replaced_by",
            ..
        }
    ));
}

#[test]
fn add_glossary_term_rejects_replacement_pointing_to_missing_term() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    let mut onto = sample_user_ontology();
    let term = GlossaryTermDef {
        id: GlossaryTermId::new("gt-old"),
        term: LocalizedText::new("Old"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: Vec::new(),
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::Deprecated {
            replaced_by: Some(GlossaryTermId::new("gt-phantom")),
            deprecated_at: Utc::now(),
        },
        concept_id: None,
    };
    let err = onto.add_glossary_term(term).unwrap_err();
    assert!(matches!(
        err,
        OntologyInvariantError::InvalidReference {
            kind: "glossary_term.replaced_by",
            ..
        }
    ));
}

#[test]
fn add_glossary_term_accepts_replacement_pointing_to_existing_term() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    fn empty_term(id: &str, label: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::Active,
            concept_id: None,
        }
    }

    let mut onto = sample_user_ontology();
    onto.add_glossary_term(empty_term("gt-new", "New")).unwrap();
    let mut deprecated = empty_term("gt-old", "Old");
    deprecated.lifecycle = TermLifecycle::Deprecated {
        replaced_by: Some(GlossaryTermId::new("gt-new")),
        deprecated_at: Utc::now(),
    };
    onto.add_glossary_term(deprecated).expect("valid replacement reference");
    assert_eq!(onto.glossary().len(), 2);
}

// ---------------------------------------------------------------------------
// ShaclConstraint::LessThan / Equals (property-pair operators)
// ---------------------------------------------------------------------------

#[test]
fn property_pair_constraint_target_resolves_to_inherit() {
    use crate::rule::{ConstraintTarget, ShaclConstraint};
    use crate::ir::PropertyId;

    let lt = ShaclConstraint::LessThan {
        target: ConstraintTarget::Inherit,
        other_property: PropertyId::new("p-other"),
    };
    let eq = ShaclConstraint::Equals {
        target: ConstraintTarget::Inherit,
        other_property: PropertyId::new("p-other"),
    };
    assert!(matches!(lt.target(), Some(ConstraintTarget::Inherit)));
    assert!(matches!(eq.target(), Some(ConstraintTarget::Inherit)));
    // No dedup signature — these are independent shape rules.
    assert!(lt.signature().is_none());
    assert!(eq.signature().is_none());
}

#[test]
fn validate_rejects_property_pair_referencing_unknown_sibling() {
    use crate::rule::{
        ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
        ShaclConstraint,
    };

    let mut onto = sample_user_ontology();
    let node = &onto.node_types[0];
    let target_property = node.properties[0].id.clone();
    let target_node = node.id.clone();

    let rule = RuleDef {
        id: "r-bad-pair".into(),
        name: LocalizedText::new("less-than against missing sibling"),
        description: LocalizedText::default(),
        rationale: LocalizedText::default(),
        severity: Severity::Violation,
        enforcement: EnforcementKind::Write,
        activation: RuleActivationKind::Always,
        origin: crate::rule::RuleOrigin::Authored,
        kind: RuleKind::PropertyShape {
            target_node_type_id: target_node,
            target_property_id: target_property,
        },
        constraints: vec![ShaclConstraint::LessThan {
            target: ConstraintTarget::Inherit,
            other_property: crate::ir::PropertyId::new("p-phantom"),
        }],
        valid_from: None,
        valid_to: None,
            sh_message: None,
    };
    onto.add_rule(rule).unwrap();
    let errors = onto.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.rule.property_pair_unknown_sibling"),
        "expected property_pair_unknown_sibling diagnostic, got: {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// EdgeKind classification
// ---------------------------------------------------------------------------

#[test]
fn edge_type_def_default_kind_is_association() {
    let edge = crate::ir::EdgeTypeDef::default();
    assert_eq!(edge.kind, crate::ir::EdgeKind::Association);
}

#[test]
fn edge_kind_serialises_to_snake_case() {
    use crate::ir::EdgeKind;

    let cases = [
        (EdgeKind::Association, "\"association\""),
        (EdgeKind::Composition, "\"composition\""),
        (EdgeKind::Aggregation, "\"aggregation\""),
    ];
    for (kind, expected) in cases {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, expected);
        let back: EdgeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }
}

#[test]
fn edge_type_def_kind_roundtrips_through_json() {
    let edge = crate::ir::EdgeTypeDef {
        id: "e-owns".into(),
        label: gl("OWNS"),
        source_node_id: "nt-a".into(),
        target_node_id: "nt-b".into(),
        kind: crate::ir::EdgeKind::Composition,
        ..Default::default()
    };
    let json = serde_json::to_value(&edge).unwrap();
    let back: crate::ir::EdgeTypeDef = serde_json::from_value(json).unwrap();
    assert_eq!(back.kind, crate::ir::EdgeKind::Composition);
}

// ---------------------------------------------------------------------------
// Locale-aware glossary alias resolver
// ---------------------------------------------------------------------------

#[test]
fn phrase_resolver_finds_term_via_default_label() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};

    let mut onto = sample_user_ontology();
    let id = GlossaryTermId::new("gt-customer");
    onto.add_glossary_term(GlossaryTermDef {
        id: id.clone(),
        term: LocalizedText::new("Customer"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: Vec::new(),
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::Active,
            concept_id: None,
    })
    .unwrap();

    let resolved = onto.glossary_term_by_phrase("CUSTOMER").unwrap();
    assert_eq!(resolved.id, id);
    assert!(onto.glossary_term_by_phrase("vendor").is_none());
    assert!(onto.glossary_term_by_phrase("").is_none());
}

#[test]
fn phrase_resolver_finds_term_via_korean_alias() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use ox_core::i18n::LanguageTag;

    let mut onto = sample_user_ontology();
    let id = GlossaryTermId::new("gt-customer");
    onto.add_glossary_term(GlossaryTermDef {
        id: id.clone(),
        term: LocalizedText::new("Customer")
            .with_translation(LanguageTag::ko(), "고객"),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: vec![
            LocalizedText::new("Buyer").with_translation(LanguageTag::ko(), "구매자"),
        ],
        related_terms: Vec::new(),
        governance: crate::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::Active,
            concept_id: None,
    })
    .unwrap();

    // Match by Korean translation of the canonical term.
    let by_ko_term = onto.glossary_term_by_phrase("고객").unwrap();
    assert_eq!(by_ko_term.id, id);
    // Match by Korean translation of an alias.
    let by_ko_alias = onto.glossary_term_by_phrase("구매자").unwrap();
    assert_eq!(by_ko_alias.id, id);
    // English alias (default) still works.
    let by_en_alias = onto.glossary_term_by_phrase("buyer").unwrap();
    assert_eq!(by_en_alias.id, id);
}

#[test]
fn phrase_resolver_follows_deprecated_replacement_chain() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    fn empty_term(id: &str, label: &str, lifecycle: TermLifecycle) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle,
            concept_id: None,
        }
    }

    let mut onto = sample_user_ontology();
    onto.add_glossary_term(empty_term("gt-current", "Customer", TermLifecycle::Active))
        .unwrap();
    onto.add_glossary_term(empty_term(
        "gt-old",
        "Client",
        TermLifecycle::Deprecated {
            replaced_by: Some(GlossaryTermId::new("gt-current")),
            deprecated_at: Utc::now(),
        },
    ))
    .unwrap();

    // Looking up the deprecated label should land on the successor.
    let resolved = onto.glossary_term_by_phrase("Client").unwrap();
    assert_eq!(resolved.id.as_str(), "gt-current");
}

#[test]
fn phrase_resolver_prefers_active_term_over_deprecated_with_same_alias() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    fn build(id: &str, label: &str, alias: &str, lifecycle: TermLifecycle) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: vec![LocalizedText::new(alias)],
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle,
            concept_id: None,
        }
    }

    let mut onto = sample_user_ontology();
    // Insert deprecated first so glossary order would otherwise return it.
    onto.add_glossary_term(build(
        "gt-old",
        "OldCustomer",
        "shopper",
        TermLifecycle::Retired { retired_at: Utc::now() },
    ))
    .unwrap();
    onto.add_glossary_term(build(
        "gt-new",
        "Customer",
        "shopper",
        TermLifecycle::Active,
    ))
    .unwrap();

    // Both terms have alias "shopper". Resolver must prefer the
    // active one regardless of insertion order.
    let resolved = onto.glossary_term_by_phrase("shopper").unwrap();
    assert_eq!(resolved.id.as_str(), "gt-new");
}

#[test]
fn phrase_resolver_prefers_canonical_term_over_alias() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};

    fn build(id: &str, label: &str, alias: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: vec![LocalizedText::new(alias)],
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::Active,
            concept_id: None,
        }
    }

    let mut onto = sample_user_ontology();
    // Term A's canonical label is "Buyer". Term B has "Buyer" as an
    // alias of canonical "Customer". A query for "Buyer" must
    // resolve to A (canonical match outranks alias match).
    onto.add_glossary_term(build("gt-customer", "Customer", "Buyer"))
        .unwrap();
    onto.add_glossary_term(build("gt-buyer", "Buyer", "Purchaser"))
        .unwrap();

    let resolved = onto.glossary_term_by_phrase("Buyer").unwrap();
    assert_eq!(resolved.id.as_str(), "gt-buyer");
}

#[test]
fn validate_flags_glossary_broader_cycle() {
    use crate::glossary::{
        GlossaryTermDef, GlossaryTermId, TermLifecycle, TermRelation, TermRelationKind,
    };

    fn term(id: &str, broader: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(id),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: vec![TermRelation {
                kind: TermRelationKind::Broader,
                target: GlossaryTermId::new(broader),
            }],
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::default(),
            concept_id: None,
        }
    }

    let mut onto = OntologyIR::new(
        "ont".into(),
        "Cycles".into(),
        LocalizedText::default(),
        1,
        vec![minimal_node("nt-1", "X")],
        vec![],
        vec![],
    );
    onto.glossary.push(term("gt-a", "gt-b"));
    onto.glossary.push(term("gt-b", "gt-a"));

    let errors = onto.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.glossary.broader_cycle"),
        "validator must flag the Broader 2-cycle: {errors:?}"
    );
}

#[test]
fn validate_flags_glossary_replaced_by_cycle() {
    use crate::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle};
    use chrono::Utc;

    fn deprecated_term(id: &str, replaced_by: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(id),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::Deprecated {
                replaced_by: Some(GlossaryTermId::new(replaced_by)),
                deprecated_at: Utc::now(),
            },
            concept_id: None,
        }
    }

    let mut onto = OntologyIR::new(
        "ont".into(),
        "Cycles".into(),
        LocalizedText::default(),
        1,
        vec![minimal_node("nt-1", "X")],
        vec![],
        vec![],
    );
    onto.glossary.push(deprecated_term("gt-a", "gt-b"));
    onto.glossary.push(deprecated_term("gt-b", "gt-a"));

    let errors = onto.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.glossary.replaced_by_cycle"),
        "validator must flag the deprecation 2-cycle: {errors:?}"
    );
}

#[test]
fn validate_flags_rule_with_inverted_validity_window() {
    use crate::rule::{RuleDef, RuleKind};
    use crate::action::RuleId;
    use chrono::{Duration, Utc};

    let mut onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![minimal_node("nt-1", "Order")],
        vec![],
        vec![],
    );
    let now = Utc::now();
    onto.rules.push(RuleDef {
        id: RuleId::new("r-bad"),
        name: LocalizedText::new("inverted"),
        description: LocalizedText::default(),
        rationale: LocalizedText::default(),
        kind: RuleKind::NodeShape {
            target_node_type_id: "nt-1".into(),
        },
        severity: Default::default(),
        enforcement: Default::default(),
        activation: Default::default(),
        origin: Default::default(),
        constraints: vec![],
        valid_from: Some(now + Duration::days(10)),
        valid_to: Some(now),
        sh_message: None,
    });

    let errors = onto.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.rule.invalid_validity_window"),
        "validator must flag valid_from >= valid_to: {errors:?}"
    );
}

#[test]
fn validate_with_sources_flags_unknown_object_mapping_source() {
    use crate::mapping::{ObjectMappingDef, SourceId};
    use std::collections::HashSet;

    let mut node = minimal_node("nt-1", "Order");
    node.properties.push(PropertyDef {
        id: "p-id".into(),
        name: pk("id"),
        property_type: PropertyType::String,
        nullable: false,
        ..Default::default()
    });

    let mut onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![node],
        vec![],
        vec![],
    );
    onto.object_mappings.push(ObjectMappingDef::new(
        "om-1",
        "nt-1",
        "bigquery:oydp-public-dw",
        "fact.fsc_sal_slip_l",
    ));

    let known = HashSet::<SourceId>::new();
    let errors = onto.validate_with_sources(&known);
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.object_mapping.unknown_source"),
        "validator must flag mapping pointing at an unregistered source: {errors:?}"
    );

    let mut known = HashSet::<SourceId>::new();
    known.insert(SourceId::from("bigquery:oydp-public-dw".to_string()));
    let errors = onto.validate_with_sources(&known);
    assert!(
        !errors
            .iter()
            .any(|e| e.code == "ontology.validate.object_mapping.unknown_source"),
        "validator must not flag mapping when its source is registered: {errors:?}"
    );
}

#[test]
fn validate_flags_property_with_two_glossary_bindings() {
    use crate::binding::PropertyBinding;
    use crate::glossary::GlossaryTermId;

    let mut node = minimal_node("nt-1", "Doc");
    node.properties.push(PropertyDef {
        id: "p-title".into(),
        name: pk("title"),
        property_type: PropertyType::String,
        nullable: false,
        bindings: vec![
            PropertyBinding::Glossary {
                id: GlossaryTermId::new("gt-a"),
                valid_from: None,
                valid_to: None,
            },
            PropertyBinding::Glossary {
                id: GlossaryTermId::new("gt-b"),
                valid_from: None,
                valid_to: None,
            },
        ],
        ..Default::default()
    });

    let onto = OntologyIR::new(
        "ont".into(),
        "X".into(),
        LocalizedText::default(),
        1,
        vec![node],
        vec![],
        vec![],
    );

    let errors = onto.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ontology.validate.property.duplicate_binding_kind"
                && e.params
                    .get("kind")
                    .map(|v| v == "glossary")
                    .unwrap_or(false)),
        "validator must flag duplicate Glossary bindings: {errors:?}"
    );
}

#[test]
fn add_concept_rejects_canonical_term_id_pointing_to_missing_term() {
    use crate::concept::{ConceptDef, ConceptGovernance, ConceptId};
    use crate::glossary::{GlossaryTermId, TermLifecycle};

    let mut onto = sample_user_ontology();
    let concept = ConceptDef {
        id: ConceptId::new("c-customer"),
        canonical_term_id: GlossaryTermId::new("gt-phantom"),
        alias_term_ids: Vec::new(),
        broader: None,
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        realisation: None,
        lifecycle: TermLifecycle::default(),
        replaced_by: None,
        valid_from: None,
        valid_to: None,
        governance: ConceptGovernance::default(),
    };
    let err = onto.add_concept(concept).unwrap_err();
    assert!(matches!(
        err,
        OntologyInvariantError::InvalidReference {
            kind: "concept.canonical_term_id",
            ..
        }
    ));
}

#[test]
fn add_concept_rejects_self_replacement() {
    use crate::concept::{ConceptDef, ConceptGovernance, ConceptId};
    use crate::glossary::{
        GlossaryTermDef, GlossaryTermId, TermGovernance, TermLifecycle,
    };

    let mut onto = sample_user_ontology();
    onto.add_glossary_term(GlossaryTermDef {
        id: GlossaryTermId::new("gt-customer"),
        term: LocalizedText::new("Customer"),
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
    })
    .unwrap();
    let id = ConceptId::new("c-customer");
    let concept = ConceptDef {
        id: id.clone(),
        canonical_term_id: GlossaryTermId::new("gt-customer"),
        alias_term_ids: Vec::new(),
        broader: None,
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        realisation: None,
        lifecycle: TermLifecycle::default(),
        replaced_by: Some(id.clone()),
        valid_from: None,
        valid_to: None,
        governance: ConceptGovernance::default(),
    };
    let err = onto.add_concept(concept).unwrap_err();
    assert!(matches!(
        err,
        OntologyInvariantError::InvalidReference {
            kind: "concept.replaced_by",
            ..
        }
    ));
}

#[test]
fn add_concept_round_trips_with_alias_terms() {
    use crate::concept::{ConceptDef, ConceptGovernance, ConceptId};
    use crate::glossary::{
        GlossaryTermDef, GlossaryTermId, TermGovernance, TermLifecycle,
    };

    fn empty_term(id: &str, label: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
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

    let mut onto = sample_user_ontology();
    onto.add_glossary_term(empty_term("gt-customer-en", "Customer"))
        .unwrap();
    onto.add_glossary_term(empty_term("gt-customer-ko", "고객"))
        .unwrap();
    onto.add_concept(ConceptDef {
        id: ConceptId::new("c-customer"),
        canonical_term_id: GlossaryTermId::new("gt-customer-en"),
        alias_term_ids: vec![GlossaryTermId::new("gt-customer-ko")],
        broader: None,
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        realisation: None,
        lifecycle: TermLifecycle::default(),
        replaced_by: None,
        valid_from: None,
        valid_to: None,
        governance: ConceptGovernance::default(),
    })
    .unwrap();
    assert_eq!(onto.concepts().len(), 1);
    assert_eq!(onto.concept_by_id(&ConceptId::new("c-customer")).unwrap().id.as_str(), "c-customer");
}
