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
    out.push_str("@prefix owl:     <http://www.w3.org/2002/07/owl#> .\n");
    out.push_str("@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str("@prefix xsd:     <http://www.w3.org/2001/XMLSchema#> .\n");
    // dcterms is used for `dcterms:isReplacedBy`, the standard way to point
    // a deprecated entity at its successor. owl:replacedBy is non-standard.
    out.push_str("@prefix dcterms: <http://purl.org/dc/terms/> .\n");
    out.push_str(&format!("@prefix :        <{base_ns}#> .\n"));
    out.push('\n');

    // --- Ontology declaration ---
    out.push_str(&format!("<{base_ns}> a owl:Ontology ;\n"));
    out.push_str(&format!(
        "    rdfs:label {} .\n",
        turtle_literal(&ontology.name),
    ));
    if let Some(desc) = ontology.description.present() {
        // Replace the trailing " .\n" with " ;\n" to chain the comment
        let len = out.len();
        out.truncate(len - 3); // remove " .\n"
        out.push_str(" ;\n");
        out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc)));
    }
    out.push('\n');

    // --- Classes (from NodeTypeDef) ---
    if !ontology.node_types().is_empty() {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Classes\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }
    for node in ontology.node_types() {
        let class_id = local_name(&node.label);
        out.push_str(&format!(":{class_id} a owl:Class ;\n"));
        out.push_str(&format!(
            "    rdfs:label {} .\n",
            turtle_literal(&node.label),
        ));
        if let Some(desc) = node.description.present() {
            chain_triple(&mut out, &format!("rdfs:comment {}", turtle_literal(desc)));
        }
        if node.deprecated_at.is_some() {
            chain_triple(&mut out, "owl:deprecated true");
        }
        if let Some(replaced_by) = &node.replaced_by_id
            && let Some(label) = ontology.node_label(replaced_by.as_ref())
        {
            chain_triple(
                &mut out,
                &format!("dcterms:isReplacedBy :{}", local_name(label)),
            );
        }
        out.push('\n');
    }

    // --- Object Properties (from EdgeTypeDef) ---
    if !ontology.edge_types().is_empty() {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Object Properties\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }
    for edge in ontology.edge_types() {
        let prop_id = local_name(&edge.label);
        let src_label = ontology.node_label(&edge.source_node_id).unwrap_or("Thing");
        let tgt_label = ontology.node_label(&edge.target_node_id).unwrap_or("Thing");
        let src_class = local_name(src_label);
        let tgt_class = local_name(tgt_label);

        out.push_str(&format!(":{prop_id} a owl:ObjectProperty ;\n"));
        out.push_str(&format!(
            "    rdfs:label {} ;\n",
            turtle_literal(&edge.label),
        ));
        out.push_str(&format!("    rdfs:domain :{src_class} ;\n"));
        out.push_str(&format!("    rdfs:range :{tgt_class} .\n"));
        if let Some(desc) = edge.description.present() {
            chain_triple(&mut out, &format!("rdfs:comment {}", turtle_literal(desc)));
        }
        // Surface source/target roles as separate comments so an OWL
        // consumer can distinguish the rdfs:comment (human prose) from
        // the functional role of each endpoint.
        if let Some(role) = &edge.source_role {
            chain_triple(
                &mut out,
                &format!(
                    "rdfs:comment {}",
                    turtle_literal(&format!("Source role: {role}"))
                ),
            );
        }
        if let Some(role) = &edge.target_role {
            chain_triple(
                &mut out,
                &format!(
                    "rdfs:comment {}",
                    turtle_literal(&format!("Target role: {role}"))
                ),
            );
        }
        if edge.deprecated_at.is_some() {
            chain_triple(&mut out, "owl:deprecated true");
        }
        if let Some(replaced_by) = &edge.replaced_by_id
            && let Some(replacement) = ontology.edge_types().iter().find(|e| &e.id == replaced_by)
        {
            chain_triple(
                &mut out,
                &format!("dcterms:isReplacedBy :{}", local_name(&replacement.label)),
            );
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
                turtle_literal(&format!("Property on relationship {}", edge.label)),
            ));
            out.push_str(&format!("    rdfs:domain :{src_class} ;\n"));
            out.push_str(&format!(
                "    rdfs:range {} .\n",
                xsd_type(&prop.property_type),
            ));
            if let Some(desc) = prop.description.present() {
                let len = out.len();
                out.truncate(len - 3);
                out.push_str(" ;\n");
                out.push_str(&format!("    rdfs:comment {} .\n", turtle_literal(desc),));
            }
            out.push('\n');
        }
    }

    // --- Datatype Properties (from PropertyDef on nodes) ---
    if ontology
        .node_types()
        .iter()
        .any(|n| !n.properties.is_empty())
    {
        out.push_str("# ----------------------------------------------------------------\n");
        out.push_str("# Datatype Properties\n");
        out.push_str("# ----------------------------------------------------------------\n\n");
    }

    // Collect unique constraint property ids per node for functional marking
    for node in ontology.node_types() {
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
            if let Some(desc) = prop.description.present() {
                chain_triple(&mut out, &format!("rdfs:comment {}", turtle_literal(desc)));
            }
            if prop.deprecated_at.is_some() {
                chain_triple(&mut out, "owl:deprecated true");
            }
            // replaced_by_id on a property points to another property within
            // the same node — resolve via the ontology's property index.
            if let Some(replaced_by) = &prop.replaced_by_id
                && let Some((_, replacement)) = ontology.property_by_id(replaced_by.as_ref())
            {
                let replacement_dp = format!("{class_id}_{}", local_name(&replacement.name));
                chain_triple(&mut out, &format!("dcterms:isReplacedBy :{replacement_dp}"));
            }
            out.push('\n');

            // Cardinality restriction on the owning class — only emit when
            // the designer explicitly pinned a bound. Defaults stay as plain
            // `owl:DatatypeProperty` so we don't pollute the schema with
            // implicit `0..n` restrictions on every property.
            if prop.min_count.is_some() || prop.max_count.is_some() {
                emit_property_cardinality_restriction(
                    &mut out,
                    &class_id,
                    &dp_id,
                    prop.min_count,
                    prop.max_count,
                );
            }
        }
    }

    out
}

/// Append a triple to an entity definition that currently ends with ` .\n`.
/// Replaces the terminal `.` with a `;`, then writes the new triple as the
/// new terminator. Centralises the trailing-dot juggling so callers can
/// chain annotations (description, deprecation, replacement) without each
/// of them duplicating the truncate dance.
fn chain_triple(out: &mut String, predicate_value: &str) {
    if !out.ends_with(" .\n") {
        // Programmer error — caller invoked us on a buffer that doesn't
        // currently terminate an entity. Skip rather than panic so a
        // mistake in one annotation does not corrupt the whole export.
        return;
    }
    let len = out.len();
    out.truncate(len - 3);
    out.push_str(" ;\n");
    out.push_str(&format!("    {predicate_value} .\n"));
}

/// Emit an OWL cardinality restriction tying a class to a property's min/max
/// count. Either bound may be absent; both absent means the caller should
/// not have invoked us.
fn emit_property_cardinality_restriction(
    out: &mut String,
    class_id: &str,
    prop_id: &str,
    min: Option<u32>,
    max: Option<u32>,
) {
    out.push_str(&format!(":{class_id} rdfs:subClassOf [\n"));
    out.push_str("    a owl:Restriction ;\n");
    out.push_str(&format!("    owl:onProperty :{prop_id} ;\n"));
    match (min, max) {
        (Some(m), Some(n)) if m == n => {
            // Exact cardinality is the OWL idiom for `min == max`.
            out.push_str(&format!(
                "    owl:cardinality \"{m}\"^^xsd:nonNegativeInteger\n"
            ));
        }
        (Some(m), Some(n)) => {
            out.push_str(&format!(
                "    owl:minCardinality \"{m}\"^^xsd:nonNegativeInteger ;\n"
            ));
            out.push_str(&format!(
                "    owl:maxCardinality \"{n}\"^^xsd:nonNegativeInteger\n"
            ));
        }
        (Some(m), None) => {
            out.push_str(&format!(
                "    owl:minCardinality \"{m}\"^^xsd:nonNegativeInteger\n"
            ));
        }
        (None, Some(n)) => {
            out.push_str(&format!(
                "    owl:maxCardinality \"{n}\"^^xsd:nonNegativeInteger\n"
            ));
        }
        (None, None) => {
            // Defensive: if both bounds are absent, emit no restriction body.
            // Caller guards against this, so the block stays empty.
        }
    }
    out.push_str("] .\n\n");
}

/// Emit OWL cardinality restrictions for edges.
fn emit_cardinality_restriction(
    out: &mut String,
    card: &Cardinality,
    class_id: &str,
    prop_id: &str,
) {
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
    use ox_core::GraphLabel;
    use ox_core::LocalizedText;
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, OntologyIR,
        PropertyDef,
    };
    use ox_core::types::PropertyType;

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

    fn sample_ontology() -> OntologyIR {
        OntologyIR::new(
            "test-id".into(),
            "Cosmetics".into(),
            LocalizedText::new("Korean cosmetics ontology"),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: gl("Brand"),
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
                    label: gl("Product"),
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
                label: gl("MANUFACTURED_BY"),
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
    fn deprecated_class_carries_owl_deprecated_and_replacement() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n1".into(), |n| {
                n.deprecated_at = Some(chrono::Utc::now());
                n.replaced_by_id = Some("n2".into());
            })
            .unwrap();
        let ttl = generate_owl_turtle(&ontology);
        // The Brand class definition should carry both annotations.
        let brand_start = ttl.find(":Brand a owl:Class").unwrap();
        // Walk to the trailing `.` of the entity definition.
        let brand_end = ttl[brand_start..]
            .find(" .\n")
            .map(|i| brand_start + i)
            .unwrap();
        let brand_block = &ttl[brand_start..brand_end];
        assert!(
            brand_block.contains("owl:deprecated true"),
            "Brand should carry owl:deprecated true: {brand_block}"
        );
        assert!(
            brand_block.contains("dcterms:isReplacedBy :Product"),
            "Brand should point at successor via dcterms:isReplacedBy: {brand_block}"
        );
        assert!(
            ttl.contains("@prefix dcterms:"),
            "dcterms prefix must be declared when isReplacedBy appears"
        );
    }

    #[test]
    fn deprecated_object_property_carries_owl_deprecated() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let ttl = generate_owl_turtle(&ontology);
        let prop_start = ttl.find(":MANUFACTURED_BY a owl:ObjectProperty").unwrap();
        let prop_end = ttl[prop_start..]
            .find(" .\n")
            .map(|i| prop_start + i)
            .unwrap();
        let prop_block = &ttl[prop_start..prop_end];
        assert!(
            prop_block.contains("owl:deprecated true"),
            "deprecated edge should carry owl:deprecated true: {prop_block}"
        );
    }

    #[test]
    fn deprecated_datatype_property_carries_owl_deprecated() {
        let mut ontology = sample_ontology();
        ontology
            .update_node_type(&"n1".into(), |n| {
                n.properties[0].deprecated_at = Some(chrono::Utc::now());
            })
            .unwrap();
        let ttl = generate_owl_turtle(&ontology);
        let dp_start = ttl.find(":Brand_name a owl:DatatypeProperty").unwrap();
        let dp_end = ttl[dp_start..].find(" .\n").map(|i| dp_start + i).unwrap();
        let dp_block = &ttl[dp_start..dp_end];
        assert!(
            dp_block.contains("owl:deprecated true"),
            "deprecated datatype property should carry owl:deprecated true: {dp_block}"
        );
    }

    #[test]
    fn edge_source_and_target_roles_surface_as_comments() {
        let mut ontology = sample_ontology();
        ontology
            .update_edge_type(&"e1".into(), |e| {
                e.source_role = Some("producer".into());
                e.target_role = Some("brand".into());
            })
            .unwrap();
        let ttl = generate_owl_turtle(&ontology);
        // Both roles emit separate rdfs:comment triples alongside the
        // description comment.
        assert!(
            ttl.contains("\"Source role: producer\""),
            "source role comment missing: {ttl}"
        );
        assert!(
            ttl.contains("\"Target role: brand\""),
            "target role comment missing: {ttl}"
        );
    }

    #[test]
    fn property_min_max_count_emits_cardinality_restriction() {
        let ontology = OntologyIR::new(
            "card-test".into(),
            "CardTest".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "n1".into(),
                label: gl("Box"),
                description: LocalizedText::default(),
                properties: vec![
                    PropertyDef {
                        id: "p1".into(),
                        name: "exact".into(),
                        property_type: PropertyType::String,
                        nullable: true,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        min_count: Some(3),
                        max_count: Some(3),
                        ..Default::default()
                    },
                    PropertyDef {
                        id: "p2".into(),
                        name: "ranged".into(),
                        property_type: PropertyType::String,
                        nullable: true,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        min_count: Some(1),
                        max_count: Some(5),
                        ..Default::default()
                    },
                ],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let ttl = generate_owl_turtle(&ontology);
        // Equal min == max collapses to owl:cardinality.
        assert!(
            ttl.contains("owl:onProperty :Box_exact") && ttl.contains("owl:cardinality \"3\""),
            "exact bound should emit owl:cardinality, got: {ttl}"
        );
        // Distinct bounds expand to owl:minCardinality + owl:maxCardinality.
        assert!(
            ttl.contains("owl:onProperty :Box_ranged")
                && ttl.contains("owl:minCardinality \"1\"")
                && ttl.contains("owl:maxCardinality \"5\""),
            "range bound should emit min + max cardinality: {ttl}"
        );
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
