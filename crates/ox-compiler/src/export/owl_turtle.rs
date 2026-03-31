use ox_core::ontology_ir::{Cardinality, NodeConstraint, OntologyIR};
use ox_core::types::PropertyType;

/// Generate an OWL ontology in Turtle format from an OntologyIR.
///
/// Produces valid Turtle syntax importable into Protege and other OWL tools.
/// Mapping:
///   - NodeTypeDef  -> owl:Class
///   - EdgeTypeDef  -> owl:ObjectProperty (+ cardinality restrictions)
///   - PropertyDef  -> owl:DatatypeProperty
///   - UNIQUE constraint -> owl:FunctionalProperty marker
pub fn generate_owl_turtle(ontology: &OntologyIR) -> String {
    let mut out = String::new();

    let base_ns = format!("http://ontosyx.io/ontology/{}", uri_encode(&ontology.name));

    // --- Prefixes ---
    out.push_str("@prefix owl:  <http://www.w3.org/2002/07/owl#> .\n");
    out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str("@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n");
    out.push_str(&format!("@prefix :     <{base_ns}#> .\n"));
    out.push('\n');

    // --- Ontology declaration ---
    out.push_str(&format!("<{base_ns}> a owl:Ontology ;\n"));
    out.push_str(&format!(
        "    rdfs:label {} .\n",
        turtle_literal(&ontology.name),
    ));
    if let Some(desc) = &ontology.description {
        // Replace the trailing " .\n" with " ;\n" to chain the comment
        let len = out.len();
        out.truncate(len - 3); // remove " .\n"
        out.push_str(" ;\n");
        out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc)));
    }
    out.push('\n');

    // --- Classes (from NodeTypeDef) ---
    if !ontology.node_types.is_empty() {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Classes\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }
    for node in &ontology.node_types {
        let class_id = local_name(&node.label);
        out.push_str(&format!(":{class_id} a owl:Class ;\n"));
        out.push_str(&format!(
            "    rdfs:label {} .\n",
            turtle_literal(&node.label),
        ));
        if let Some(desc) = &node.description {
            let len = out.len();
            out.truncate(len - 3);
            out.push_str(" ;\n");
            out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc)));
        }
        out.push('\n');
    }

    // --- Object Properties (from EdgeTypeDef) ---
    if !ontology.edge_types.is_empty() {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Object Properties\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }
    for edge in &ontology.edge_types {
        let prop_id = local_name(&edge.label);
        let src_label = ontology
            .node_label(&edge.source_node_id)
            .unwrap_or("Thing");
        let tgt_label = ontology
            .node_label(&edge.target_node_id)
            .unwrap_or("Thing");
        let src_class = local_name(src_label);
        let tgt_class = local_name(tgt_label);

        out.push_str(&format!(":{prop_id} a owl:ObjectProperty ;\n"));
        out.push_str(&format!(
            "    rdfs:label {} ;\n",
            turtle_literal(&edge.label),
        ));
        out.push_str(&format!("    rdfs:domain :{src_class} ;\n"));
        out.push_str(&format!("    rdfs:range :{tgt_class} .\n"));
        if let Some(desc) = &edge.description {
            let len = out.len();
            out.truncate(len - 3);
            out.push_str(" ;\n");
            out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc)));
        }
        out.push('\n');

        // Cardinality restrictions on source class
        emit_cardinality_restriction(&mut out, &edge.cardinality, &src_class, &prop_id);

        // Edge datatype properties — modeled as owl:DatatypeProperty with a note
        for prop in &edge.properties {
            let edge_prop_id = format!("{}_{}", prop_id, local_name(&prop.name));
            out.push_str(&format!(":{edge_prop_id} a owl:DatatypeProperty ;\n"));
            out.push_str(&format!(
                "    rdfs:label {} ;\n",
                turtle_literal(&prop.name),
            ));
            out.push_str(&format!(
                "    rdfs:comment {} ;\n",
                turtle_literal(&format!(
                    "Property on relationship {}",
                    edge.label
                )),
            ));
            out.push_str(&format!("    rdfs:domain :{src_class} ;\n"));
            out.push_str(&format!(
                "    rdfs:range {} .\n",
                xsd_type(&prop.property_type),
            ));
            if let Some(desc) = &prop.description {
                let len = out.len();
                out.truncate(len - 3);
                out.push_str(" ;\n");
                out.push_str(&format!(
                    "    rdfs:comment {} .\n",
                    turtle_literal(desc),
                ));
            }
            out.push('\n');
        }
    }

    // --- Datatype Properties (from PropertyDef on nodes) ---
    if ontology
        .node_types
        .iter()
        .any(|n| !n.properties.is_empty())
    {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Datatype Properties\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }

    // Collect unique constraint property ids per node for functional marking
    for node in &ontology.node_types {
        let class_id = local_name(&node.label);
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

        for prop in &node.properties {
            let dp_id = format!("{class_id}_{}", local_name(&prop.name));
            let is_functional = unique_prop_ids.contains(prop.id.as_ref());

            if is_functional {
                out.push_str(&format!(
                    ":{dp_id} a owl:DatatypeProperty , owl:FunctionalProperty ;\n"
                ));
            } else {
                out.push_str(&format!(":{dp_id} a owl:DatatypeProperty ;\n"));
            }
            out.push_str(&format!(
                "    rdfs:label {} ;\n",
                turtle_literal(&prop.name),
            ));
            out.push_str(&format!("    rdfs:domain :{class_id} ;\n"));
            out.push_str(&format!(
                "    rdfs:range {} .\n",
                xsd_type(&prop.property_type),
            ));
            if let Some(desc) = &prop.description {
                let len = out.len();
                out.truncate(len - 3);
                out.push_str(" ;\n");
                out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc)));
            }
            out.push('\n');
        }
    }

    out
}

/// Emit OWL cardinality restrictions for edges.
fn emit_cardinality_restriction(out: &mut String, card: &Cardinality, class_id: &str, prop_id: &str) {
    // source-side cardinality (how many targets a source can have)
    match card {
        Cardinality::OneToOne | Cardinality::ManyToOne => {
            // Source can have at most 1 target -> maxCardinality 1
            out.push_str(&format!(":{class_id} rdfs:subClassOf [\n"));
            out.push_str("    a owl:Restriction ;\n");
            out.push_str(&format!("    owl:onProperty :{prop_id} ;\n"));
            out.push_str("    owl:maxCardinality \"1\"^^xsd:nonNegativeInteger\n");
            out.push_str("] .\n\n");
        }
        Cardinality::OneToMany | Cardinality::ManyToMany => {
            // No upper bound restriction needed
        }
    }
}

/// Map PropertyType to an XSD datatype IRI.
fn xsd_type(pt: &PropertyType) -> &'static str {
    match pt {
        PropertyType::String => "xsd:string",
        PropertyType::Int => "xsd:integer",
        PropertyType::Float => "xsd:decimal",
        PropertyType::Bool => "xsd:boolean",
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
    fn generates_valid_prefixes() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains("@prefix owl:"));
        assert!(ttl.contains("@prefix rdfs:"));
        assert!(ttl.contains("@prefix xsd:"));
        assert!(ttl.contains("@prefix :"));
    }

    #[test]
    fn generates_ontology_declaration() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains("a owl:Ontology"));
        assert!(ttl.contains("\"Cosmetics\""));
        assert!(ttl.contains("\"Korean cosmetics ontology\""));
    }

    #[test]
    fn generates_classes() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains(":Brand a owl:Class"));
        assert!(ttl.contains(":Product a owl:Class"));
        assert!(ttl.contains("\"Cosmetic brand entity\""));
    }

    #[test]
    fn generates_object_properties() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains(":MANUFACTURED_BY a owl:ObjectProperty"));
        assert!(ttl.contains("rdfs:domain :Product"));
        assert!(ttl.contains("rdfs:range :Brand"));
    }

    #[test]
    fn generates_datatype_properties() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains(":Brand_name a owl:DatatypeProperty , owl:FunctionalProperty"));
        assert!(ttl.contains("rdfs:range xsd:string"));
        assert!(ttl.contains(":Product_price a owl:DatatypeProperty"));
        assert!(ttl.contains("rdfs:range xsd:decimal"));
    }

    #[test]
    fn generates_cardinality_restriction() {
        let ttl = generate_owl_turtle(&sample_ontology());
        // ManyToOne should produce maxCardinality 1 on source (Product)
        assert!(ttl.contains("owl:maxCardinality"));
        assert!(ttl.contains("owl:onProperty :MANUFACTURED_BY"));
    }

    #[test]
    fn functional_property_for_unique_constraint() {
        let ttl = generate_owl_turtle(&sample_ontology());
        assert!(ttl.contains(":Brand_name a owl:DatatypeProperty , owl:FunctionalProperty"));
        // founded_year has no unique constraint, should NOT be functional
        assert!(ttl.contains(":Brand_founded_year a owl:DatatypeProperty ;"));
    }

    #[test]
    fn escapes_special_characters() {
        assert_eq!(turtle_literal("hello \"world\""), "\"hello \\\"world\\\"\"");
        assert_eq!(turtle_literal("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(turtle_literal("back\\slash"), "\"back\\\\slash\"");
    }

    #[test]
    fn local_name_sanitizes() {
        assert_eq!(local_name("Hello World"), "Hello_World");
        assert_eq!(local_name("123abc"), "_123abc");
        assert_eq!(local_name(""), "_unnamed");
    }
}
