use ox_core::ontology_ir::{Cardinality, NodeConstraint, OntologyIR};
use ox_core::types::PropertyType;

/// Generate SHACL shapes in Turtle format from an OntologyIR.
///
/// Produces valid Turtle syntax suitable for SHACL validation engines.
/// Mapping:
///   - NodeTypeDef  -> sh:NodeShape with sh:targetClass
///   - PropertyDef  -> sh:property blocks with datatype constraints
///   - EdgeTypeDef  -> sh:property blocks with sh:class + cardinality
///   - UNIQUE constraint -> sh:maxCount 1
///   - NodeKey constraint -> sh:minCount 1 + sh:maxCount 1
///   - non-nullable property -> sh:minCount 1
///   - explicit min_count/max_count → sh:minCount/sh:maxCount (overrides nullability default)
///   - deprecated_at on node/edge/property → owl:deprecated true (annotation)
pub fn generate_shacl(ontology: &OntologyIR) -> String {
    let mut out = String::new();

    let base_ns = format!("http://ontosyx.io/ontology/{}", uri_encode(&ontology.name));

    // --- Prefixes ---
    out.push_str("@prefix sh:   <http://www.w3.org/ns/shacl#> .\n");
    out.push_str("@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n");
    out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    // owl: needed for owl:deprecated annotations on shapes / property blocks.
    out.push_str("@prefix owl:  <http://www.w3.org/2002/07/owl#> .\n");
    out.push_str(&format!("@prefix :     <{base_ns}#> .\n"));
    out.push('\n');

    // --- Node Shapes ---
    for node in ontology.node_types() {
        let class_name = local_name(&node.label);
        let shape_name = format!("{class_name}Shape");

        // Collect property IDs that have a UNIQUE constraint (single-property only)
        let unique_prop_ids: std::collections::HashSet<&str> = node
            .constraints
            .iter()
            .filter_map(|c| match &c.constraint {
                NodeConstraint::Unique { property_ids } if property_ids.len() == 1 => {
                    Some(property_ids[0].as_ref())
                }
                _ => None,
            })
            .collect();

        // Collect property IDs that are part of a NodeKey constraint
        let node_key_prop_ids: std::collections::HashSet<&str> = node
            .constraints
            .iter()
            .filter_map(|c| match &c.constraint {
                NodeConstraint::NodeKey { property_ids } => {
                    Some(property_ids.iter().map(AsRef::as_ref))
                }
                _ => None,
            })
            .flatten()
            .collect();

        out.push_str(&format!(":{shape_name}\n"));
        out.push_str("    a sh:NodeShape ;\n");
        out.push_str(&format!("    sh:targetClass :{class_name} ;\n"));
        out.push_str(&format!(
            "    rdfs:label {} ;\n",
            turtle_literal(&format!("{} shape", node.label)),
        ));
        if node.deprecated_at.is_some() {
            out.push_str("    owl:deprecated true ;\n");
        }
        if let Some(desc) = node.description.present() {
            out.push_str(&format!("    rdfs:comment {} ;\n", turtle_literal(desc)));
        }

        // --- Property shapes for node properties ---
        let has_props = !node.properties.is_empty();
        let edges_for_node: Vec<_> = ontology
            .edge_types()
            .iter()
            .filter(|e| e.source_node_id == node.id)
            .collect();
        let has_edges = !edges_for_node.is_empty();

        for (i, prop) in node.properties.iter().enumerate() {
            let is_last_item = i == node.properties.len() - 1 && !has_edges;
            let terminator = if is_last_item { " ." } else { " ;" };
            let is_unique = unique_prop_ids.contains(prop.id.as_ref());
            let is_node_key = node_key_prop_ids.contains(prop.id.as_ref());

            // Resolve effective cardinality bounds.
            // Explicit `min_count` / `max_count` override the nullability /
            // constraint defaults so an ontology designer can pin a
            // 0..N or 2..5 list-property without changing the constraint set.
            let effective_min = prop
                .min_count
                .or_else(|| (!prop.nullable || is_node_key).then_some(1));
            let effective_max = prop
                .max_count
                .or_else(|| (is_unique || is_node_key).then_some(1));

            // Build the property block as a list of (key, value) lines so
            // the trailing-semicolon discipline is centralised in `emit_block`.
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("sh:path :{}", local_name(&prop.name)));
            lines.push(format!("sh:datatype {}", xsd_type(&prop.property_type)));
            lines.push(format!("sh:name {}", turtle_literal(&prop.name)));
            if let Some(desc) = prop.description.present() {
                lines.push(format!("sh:description {}", turtle_literal(desc)));
            }
            if let Some(min) = effective_min {
                lines.push(format!("sh:minCount {min}"));
            }
            if let Some(max) = effective_max {
                lines.push(format!("sh:maxCount {max}"));
            }
            if prop.deprecated_at.is_some() {
                lines.push("owl:deprecated true".into());
            }

            out.push_str("    sh:property [\n");
            emit_block(&mut out, &lines);
            out.push_str(&format!("    ]{terminator}\n"));
        }

        // --- Property shapes for outgoing edges ---
        for (i, edge) in edges_for_node.iter().enumerate() {
            let tgt_label = ontology.node_label(&edge.target_node_id).unwrap_or("Thing");
            let tgt_class = local_name(tgt_label);
            let is_last = i == edges_for_node.len() - 1;
            let terminator = if is_last { " ." } else { " ;" };

            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("sh:path :{}", local_name(&edge.label)));
            lines.push(format!("sh:class :{tgt_class}"));
            // Prefer the target role as the SHACL property name when
            // available — that is what a human would call this field in
            // context ("subordinates" vs the raw "MANAGES" label).
            let shape_name = edge.target_role.as_deref().unwrap_or(&edge.label);
            lines.push(format!("sh:name {}", turtle_literal(shape_name)));
            if let Some(desc) = build_edge_description(edge) {
                lines.push(format!("sh:description {}", turtle_literal(&desc)));
            }
            push_edge_cardinality(&mut lines, &edge.cardinality);
            if edge.deprecated_at.is_some() {
                lines.push("owl:deprecated true".into());
            }

            out.push_str("    sh:property [\n");
            emit_block(&mut out, &lines);
            out.push_str(&format!("    ]{terminator}\n"));
        }

        // If no properties and no edges, close the shape
        if !has_props && !has_edges {
            // Remove trailing " ;\n" and close with " .\n"
            let len = out.len();
            out.truncate(len - 3);
            out.push_str(" .\n");
        }

        out.push('\n');
    }

    out
}

/// Emit a Turtle predicate-list block where every line gets ` ;` terminator
/// except the last, which gets none. Centralises the trailing-semicolon
/// discipline so callers don't have to truncate.
///
/// Each entry should be a bare `predicate object` string (no leading
/// whitespace, no terminator). The block writes them all indented by 8
/// spaces, ending with `\n` after the final line so the closing bracket
/// can sit on its own line.
fn emit_block(out: &mut String, lines: &[String]) {
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        let suffix = if i == last { "" } else { " ;" };
        out.push_str(&format!("        {line}{suffix}\n"));
    }
}

/// Compose an edge's SHACL description: base description + optional
/// source/target role hint. Roles describe the functional endpoints of
/// the relationship (e.g. MANAGES has "manager" / "direct_report") and
/// are what a data consumer actually needs to interpret the triple.
fn build_edge_description(edge: &ox_core::ontology_ir::EdgeTypeDef) -> Option<String> {
    let base = edge.description.present();
    let role_hint = match (edge.source_role.as_deref(), edge.target_role.as_deref()) {
        (None, None) => None,
        (Some(s), Some(t)) => Some(format!("Roles: {s} → {t}")),
        (Some(s), None) => Some(format!("Source role: {s}")),
        (None, Some(t)) => Some(format!("Target role: {t}")),
    };
    match (base, role_hint) {
        (None, None) => None,
        (Some(b), None) => Some(b.to_string()),
        (None, Some(r)) => Some(r),
        (Some(b), Some(r)) => Some(format!("{b} — {r}")),
    }
}

/// Push edge-cardinality lines into a property-block line list.
///
/// The cardinality describes source→target multiplicity:
///   - OneToOne:   source has exactly 1 target  → minCount 1, maxCount 1
///   - ManyToOne:  each source has 1 target      → minCount 1, maxCount 1
///   - OneToMany:  source can have many targets   → minCount 1 (no upper bound)
///   - ManyToMany: no cardinality constraints
fn push_edge_cardinality(lines: &mut Vec<String>, card: &Cardinality) {
    match card {
        Cardinality::OneToOne | Cardinality::ManyToOne => {
            lines.push("sh:minCount 1".into());
            lines.push("sh:maxCount 1".into());
        }
        Cardinality::OneToMany => {
            lines.push("sh:minCount 1".into());
        }
        Cardinality::ManyToMany => {}
    }
}

/// Map PropertyType to an XSD datatype IRI.
fn xsd_type(pt: &PropertyType) -> &'static str {
    match pt {
        PropertyType::Bool => "xsd:boolean",
        PropertyType::Int => "xsd:integer",
        PropertyType::Float => "xsd:double",
        PropertyType::String => "xsd:string",
        PropertyType::Date => "xsd:date",
        PropertyType::DateTime => "xsd:dateTime",
        PropertyType::Duration => "xsd:duration",
        PropertyType::Bytes => "xsd:base64Binary",
        PropertyType::List { .. } => "xsd:string",
        PropertyType::Map => "xsd:string",
    }
}

/// Produce a Turtle string literal with proper escaping.
fn turtle_literal(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

/// Produce a valid local name (NCName) for use after the `:` prefix.
/// Replaces non-alphanumeric characters (except `_`) with `_`.
fn local_name(label: &str) -> String {
    let mut name: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // NCName cannot start with a digit
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if name.is_empty() {
        name.push_str("_unnamed");
    }
    name
}

/// Minimal percent-encoding for use in IRI paths.
fn uri_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::LocalizedText;
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, OntologyIR,
        PropertyDef,
    };
    use ox_core::types::PropertyType;

    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "test-id".into(),
            "Cosmetics".into(),
            LocalizedText::new("Korean cosmetics ontology"),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Brand".into(),
                    description: LocalizedText::new("Cosmetic brand entity"),
                    properties: vec![
                        PropertyDef {
                            id: "p1".into(),
                            name: "name".into(),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: LocalizedText::new("Brand name in Korean"),
                            classification: None,
                            ..Default::default()
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: "founded_year".into(),
                            property_type: PropertyType::Int,
                            nullable: true,
                            default_value: None,
                            description: LocalizedText::default(),
                            classification: None,
                            ..Default::default()
                        },
                    ],
                    constraints: vec![ConstraintDef {
                        id: "c1".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["p1".into()],
                        },
                    }],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Product".into(),
                    description: LocalizedText::new("A cosmetic product"),
                    properties: vec![PropertyDef {
                        id: "p3".into(),
                        name: "price".into(),
                        property_type: PropertyType::Float,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "MANUFACTURED_BY".into(),
                description: LocalizedText::new("Product manufactured by brand"),
                source_node_id: "n2".into(),
                target_node_id: "n1".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn test_generates_prefixes() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(ttl.contains("@prefix sh:"));
        assert!(ttl.contains("@prefix xsd:"));
        assert!(ttl.contains("@prefix rdfs:"));
        assert!(ttl.contains("@prefix :"));
    }

    #[test]
    fn test_node_shape_generation() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(ttl.contains(":BrandShape"));
        assert!(ttl.contains("a sh:NodeShape"));
        assert!(ttl.contains("sh:targetClass :Brand"));
        assert!(ttl.contains(":ProductShape"));
        assert!(ttl.contains("sh:targetClass :Product"));
    }

    #[test]
    fn test_property_constraints() {
        let ttl = generate_shacl(&sample_ontology());
        // name is non-nullable → sh:minCount 1
        assert!(ttl.contains("sh:path :name"));
        assert!(ttl.contains("sh:datatype xsd:string"));

        // founded_year is nullable → no sh:minCount
        // Check that founded_year block does not have sh:minCount
        let fy_start = ttl.find("sh:path :founded_year").unwrap();
        let fy_block_end = ttl[fy_start..].find(']').unwrap() + fy_start;
        let fy_block = &ttl[fy_start..fy_block_end];
        assert!(!fy_block.contains("sh:minCount"));
    }

    #[test]
    fn test_unique_constraint_max_count() {
        let ttl = generate_shacl(&sample_ontology());
        // name has UNIQUE constraint → sh:maxCount 1
        let name_start = ttl.find("sh:path :name").unwrap();
        let name_block_end = ttl[name_start..].find(']').unwrap() + name_start;
        let name_block = &ttl[name_start..name_block_end];
        assert!(name_block.contains("sh:maxCount 1"));
        assert!(name_block.contains("sh:minCount 1")); // also non-nullable
    }

    #[test]
    fn test_node_key_constraint() {
        let ontology = OntologyIR::new(
            "nk-test".into(),
            "NodeKeyTest".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n1".into(),
                label: "Entity".into(),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p1".into(),
                    name: "code".into(),
                    property_type: PropertyType::String,
                    nullable: true, // nullable but NodeKey should force minCount 1
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                }],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::NodeKey {
                        property_ids: vec!["p1".into()],
                    },
                }],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let ttl = generate_shacl(&ontology);
        let code_start = ttl.find("sh:path :code").unwrap();
        let code_block_end = ttl[code_start..].find(']').unwrap() + code_start;
        let code_block = &ttl[code_start..code_block_end];
        assert!(code_block.contains("sh:minCount 1"));
        assert!(code_block.contains("sh:maxCount 1"));
    }

    #[test]
    fn test_datatype_mapping() {
        let types_and_expected = [
            (PropertyType::Bool, "xsd:boolean"),
            (PropertyType::Int, "xsd:integer"),
            (PropertyType::Float, "xsd:double"),
            (PropertyType::String, "xsd:string"),
            (PropertyType::Date, "xsd:date"),
            (PropertyType::DateTime, "xsd:dateTime"),
            (PropertyType::Duration, "xsd:duration"),
            (PropertyType::Bytes, "xsd:base64Binary"),
        ];

        for (pt, expected) in &types_and_expected {
            assert_eq!(xsd_type(pt), *expected, "Failed for {:?}", pt);
        }
    }

    #[test]
    fn test_edge_as_property_shape() {
        let ttl = generate_shacl(&sample_ontology());
        // Edge MANUFACTURED_BY from Product to Brand
        assert!(ttl.contains("sh:path :MANUFACTURED_BY"));
        assert!(ttl.contains("sh:class :Brand"));
        // ManyToOne → minCount 1, maxCount 1
        let edge_start = ttl.find("sh:path :MANUFACTURED_BY").unwrap();
        let edge_block_end = ttl[edge_start..].find(']').unwrap() + edge_start;
        let edge_block = &ttl[edge_start..edge_block_end];
        assert!(edge_block.contains("sh:minCount 1"));
        assert!(edge_block.contains("sh:maxCount 1"));
    }

    #[test]
    fn test_special_character_escaping() {
        assert_eq!(turtle_literal("hello \"world\""), "\"hello \\\"world\\\"\"");
        assert_eq!(turtle_literal("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(turtle_literal("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(local_name("Hello World"), "Hello_World");
        assert_eq!(local_name("123abc"), "_123abc");
        assert_eq!(local_name(""), "_unnamed");
    }

    #[test]
    fn test_empty_ontology() {
        let ontology = OntologyIR::new(
            "empty".into(),
            "Empty".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        let ttl = generate_shacl(&ontology);
        // Should produce valid prefix block
        assert!(ttl.contains("@prefix sh:"));
        assert!(ttl.contains("@prefix xsd:"));
        // No shapes
        assert!(!ttl.contains("sh:NodeShape"));
    }

    #[test]
    fn test_property_description() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(ttl.contains("sh:description \"Brand name in Korean\""));
    }

    #[test]
    fn test_node_description_as_comment() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(ttl.contains("rdfs:comment \"Cosmetic brand entity\""));
    }

    #[test]
    fn test_edge_description() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(ttl.contains("sh:description \"Product manufactured by brand\""));
    }

    #[test]
    fn test_many_to_many_no_cardinality() {
        let ontology = OntologyIR::new(
            "mm-test".into(),
            "ManyManyTest".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "A".into(),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "B".into(),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "RELATES_TO".into(),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            }],
            vec![],
        );
        let ttl = generate_shacl(&ontology);
        let edge_start = ttl.find("sh:path :RELATES_TO").unwrap();
        let edge_block_end = ttl[edge_start..].find(']').unwrap() + edge_start;
        let edge_block = &ttl[edge_start..edge_block_end];
        assert!(!edge_block.contains("sh:minCount"));
        assert!(!edge_block.contains("sh:maxCount"));
    }

    #[test]
    fn test_owl_prefix_emitted() {
        let ttl = generate_shacl(&sample_ontology());
        assert!(
            ttl.contains("@prefix owl:"),
            "owl: prefix is required for owl:deprecated annotations: {ttl}"
        );
    }

    #[test]
    fn test_explicit_min_max_count_overrides_nullability_default() {
        let ontology = OntologyIR::new(
            "mc-test".into(),
            "MinMax".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n1".into(),
                label: "Bag".into(),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p1".into(),
                    name: "tags".into(),
                    property_type: PropertyType::String,
                    nullable: true,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    min_count: Some(2),
                    max_count: Some(5),
                    ..Default::default()
                }],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let ttl = generate_shacl(&ontology);
        let prop_start = ttl.find("sh:path :tags").unwrap();
        let block_end = ttl[prop_start..].find(']').unwrap() + prop_start;
        let block = &ttl[prop_start..block_end];
        assert!(
            block.contains("sh:minCount 2"),
            "explicit min_count=2 should appear, got: {block}"
        );
        assert!(
            block.contains("sh:maxCount 5"),
            "explicit max_count=5 should appear, got: {block}"
        );
    }

    #[test]
    fn test_deprecated_node_shape_carries_owl_annotation() {
        let mut ontology = sample_ontology();
        // Mark `Brand` deprecated.
        let brand_idx = ontology
            .node_types()
            .iter()
            .position(|n| n.label == "Brand")
            .unwrap();
        ontology
            .update_node_type(&"n1".into(), |n| {
                let _ = brand_idx;
                n.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let ttl = generate_shacl(&ontology);
        let shape_start = ttl.find(":BrandShape").unwrap();
        let shape_end = ttl[shape_start..].find(" .").unwrap() + shape_start;
        let shape = &ttl[shape_start..shape_end];
        assert!(
            shape.contains("owl:deprecated true"),
            "deprecated node shape should carry owl:deprecated true: {shape}"
        );
    }

    #[test]
    fn test_deprecated_property_block_carries_owl_annotation() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n1".into(), |n| {
                n.properties[0].deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let ttl = generate_shacl(&ontology);
        let prop_start = ttl.find("sh:path :name").unwrap();
        let block_end = ttl[prop_start..].find(']').unwrap() + prop_start;
        let block = &ttl[prop_start..block_end];
        assert!(
            block.contains("owl:deprecated true"),
            "deprecated property block should carry owl:deprecated true: {block}"
        );
    }

    #[test]
    fn test_deprecated_edge_block_carries_owl_annotation() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let ttl = generate_shacl(&ontology);
        let edge_start = ttl.find("sh:path :MANUFACTURED_BY").unwrap();
        let block_end = ttl[edge_start..].find(']').unwrap() + edge_start;
        let block = &ttl[edge_start..block_end];
        assert!(
            block.contains("owl:deprecated true"),
            "deprecated edge block should carry owl:deprecated true: {block}"
        );
    }

    #[test]
    fn edge_roles_flow_into_sh_name_and_description() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.source_role = Some("producer".into());
                e.target_role = Some("brand".into());
            })
            .unwrap();
        let ttl = generate_shacl(&ontology);
        let edge_start = ttl.find("sh:path :MANUFACTURED_BY").unwrap();
        let block_end = ttl[edge_start..].find(']').unwrap() + edge_start;
        let block = &ttl[edge_start..block_end];
        assert!(
            block.contains("sh:name \"brand\""),
            "sh:name should prefer target_role: {block}"
        );
        assert!(
            block.contains("Roles: producer → brand"),
            "sh:description should carry role hint: {block}"
        );
    }

    #[test]
    fn test_one_to_many_cardinality() {
        let ontology = OntologyIR::new(
            "otm-test".into(),
            "OneToManyTest".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Parent".into(),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Child".into(),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "HAS_CHILD".into(),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
                ..Default::default()
            }],
            vec![],
        );
        let ttl = generate_shacl(&ontology);
        let edge_start = ttl.find("sh:path :HAS_CHILD").unwrap();
        let edge_block_end = ttl[edge_start..].find(']').unwrap() + edge_start;
        let edge_block = &ttl[edge_start..edge_block_end];
        // OneToMany → minCount 1 (source must have at least one), no maxCount
        assert!(edge_block.contains("sh:minCount 1"));
        assert!(!edge_block.contains("sh:maxCount"));
    }
}
