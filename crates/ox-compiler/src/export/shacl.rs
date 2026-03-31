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
pub fn generate_shacl(ontology: &OntologyIR) -> String {
    let mut out = String::new();

    let base_ns = format!("http://ontosyx.io/ontology/{}", uri_encode(&ontology.name));

    // --- Prefixes ---
    out.push_str("@prefix sh:   <http://www.w3.org/ns/shacl#> .\n");
    out.push_str("@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n");
    out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str(&format!("@prefix :     <{base_ns}#> .\n"));
    out.push('\n');

    // --- Node Shapes ---
    for node in &ontology.node_types {
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
        if let Some(desc) = &node.description {
            out.push_str(&format!("    rdfs:comment {} ;\n", turtle_literal(desc)));
        }

        // --- Property shapes for node properties ---
        let has_props = !node.properties.is_empty();
        let edges_for_node: Vec<_> = ontology
            .edge_types
            .iter()
            .filter(|e| e.source_node_id == node.id)
            .collect();
        let has_edges = !edges_for_node.is_empty();

        for (i, prop) in node.properties.iter().enumerate() {
            let is_last_item = i == node.properties.len() - 1 && !has_edges;
            let terminator = if is_last_item { " ." } else { " ;" };
            let is_unique = unique_prop_ids.contains(prop.id.as_ref());
            let is_node_key = node_key_prop_ids.contains(prop.id.as_ref());

            out.push_str("    sh:property [\n");
            out.push_str(&format!(
                "        sh:path :{} ;\n",
                local_name(&prop.name),
            ));
            out.push_str(&format!(
                "        sh:datatype {} ;\n",
                xsd_type(&prop.property_type),
            ));
            out.push_str(&format!(
                "        sh:name {} ;\n",
                turtle_literal(&prop.name),
            ));
            if let Some(desc) = &prop.description {
                out.push_str(&format!(
                    "        sh:description {} ;\n",
                    turtle_literal(desc),
                ));
            }

            // Cardinality from nullability and constraints
            if !prop.nullable || is_node_key {
                out.push_str("        sh:minCount 1 ;\n");
            }
            if is_unique || is_node_key {
                // Last property in the block — no trailing semicolon
                out.push_str("        sh:maxCount 1\n");
            } else {
                // Remove trailing " ;\n" from the last line and close without semicolon
                let len = out.len();
                out.truncate(len - 3); // remove " ;\n"
                out.push('\n');
            }
            out.push_str(&format!("    ]{terminator}\n"));
        }

        // --- Property shapes for outgoing edges ---
        for (i, edge) in edges_for_node.iter().enumerate() {
            let tgt_label = ontology
                .node_label(&edge.target_node_id)
                .unwrap_or("Thing");
            let tgt_class = local_name(tgt_label);
            let is_last = i == edges_for_node.len() - 1;
            let terminator = if is_last { " ." } else { " ;" };

            out.push_str("    sh:property [\n");
            out.push_str(&format!(
                "        sh:path :{} ;\n",
                local_name(&edge.label),
            ));
            out.push_str(&format!("        sh:class :{tgt_class} ;\n"));
            out.push_str(&format!(
                "        sh:name {} ;\n",
                turtle_literal(&edge.label),
            ));
            if let Some(desc) = &edge.description {
                out.push_str(&format!(
                    "        sh:description {} ;\n",
                    turtle_literal(desc),
                ));
            }

            // Cardinality constraints from edge
            emit_edge_cardinality(&mut out, &edge.cardinality);

            // Remove trailing " ;\n" from the last constraint line
            let len = out.len();
            out.truncate(len - 3);
            out.push('\n');

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

/// Emit sh:minCount/sh:maxCount for an edge based on its cardinality.
///
/// The cardinality describes source→target multiplicity:
///   - OneToOne:   source has exactly 1 target  → minCount 1, maxCount 1
///   - ManyToOne:  each source has 1 target      → minCount 1, maxCount 1
///   - OneToMany:  source can have many targets   → (no upper bound)
///   - ManyToMany: no cardinality constraints     → (no constraints)
fn emit_edge_cardinality(out: &mut String, card: &Cardinality) {
    match card {
        Cardinality::OneToOne | Cardinality::ManyToOne => {
            out.push_str("        sh:minCount 1 ;\n");
            out.push_str("        sh:maxCount 1 ;\n");
        }
        Cardinality::OneToMany => {
            out.push_str("        sh:minCount 1 ;\n");
        }
        Cardinality::ManyToMany => {
            // No cardinality constraints
        }
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
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
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
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, OntologyIR,
        PropertyDef,
    };
    use ox_core::types::PropertyType;

    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "test-id".into(),
            "Cosmetics".into(),
            Some("Korean cosmetics ontology".into()),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Brand".into(),
                    description: Some("Cosmetic brand entity".into()),
                    source_table: None,
                    properties: vec![
                        PropertyDef {
                            id: "p1".into(),
                            name: "name".into(),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: Some("Brand name in Korean".into()),
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: "founded_year".into(),
                            property_type: PropertyType::Int,
                            nullable: true,
                            default_value: None,
                            description: None,
                        },
                    ],
                    constraints: vec![ConstraintDef {
                        id: "c1".into(),
                        constraint: NodeConstraint::Unique {
                            property_ids: vec!["p1".into()],
                        },
                    }],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Product".into(),
                    description: Some("A cosmetic product".into()),
                    source_table: None,
                    properties: vec![PropertyDef {
                        id: "p3".into(),
                        name: "price".into(),
                        property_type: PropertyType::Float,
                        nullable: false,
                        default_value: None,
                        description: None,
                    }],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "MANUFACTURED_BY".into(),
                description: Some("Product manufactured by brand".into()),
                source_node_id: "n2".into(),
                target_node_id: "n1".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
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
            None,
            1,
            vec![NodeTypeDef {
                id: "n1".into(),
                label: "Entity".into(),
                description: None,
                source_table: None,
                properties: vec![PropertyDef {
                    id: "p1".into(),
                    name: "code".into(),
                    property_type: PropertyType::String,
                    nullable: true, // nullable but NodeKey should force minCount 1
                    default_value: None,
                    description: None,
                }],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::NodeKey {
                        property_ids: vec!["p1".into()],
                    },
                }],
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
            None,
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
            None,
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "A".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "B".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "RELATES_TO".into(),
                description: None,
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
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
    fn test_one_to_many_cardinality() {
        let ontology = OntologyIR::new(
            "otm-test".into(),
            "OneToManyTest".into(),
            None,
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Parent".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Child".into(),
                    description: None,
                    source_table: None,
                    properties: vec![],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "HAS_CHILD".into(),
                description: None,
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::OneToMany,
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
