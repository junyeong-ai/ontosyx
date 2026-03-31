use ox_core::ontology_ir::{Cardinality, NodeConstraint, NodeTypeDef, OntologyIR};
use ox_core::types::PropertyType;

/// Generate a Mermaid ER diagram from an OntologyIR.
pub fn generate_mermaid(ontology: &OntologyIR) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("%% {}", ontology.name));
    lines.push("erDiagram".to_string());

    // Entity definitions
    for node in &ontology.node_types {
        let pk_name = find_pk_property(node);
        let id = mermaid_id(&node.label);
        lines.push(format!("    {id} {{"));
        for prop in &node.properties {
            let pk_marker = if Some(prop.name.as_str()) == pk_name {
                " PK"
            } else {
                ""
            };
            lines.push(format!(
                "        {} {}{}",
                mermaid_type(&prop.property_type),
                mermaid_id(&prop.name),
                pk_marker,
            ));
        }
        lines.push("    }".to_string());
    }

    // Relationships
    for edge in &ontology.edge_types {
        let src = ontology
            .node_label(&edge.source_node_id)
            .map(mermaid_id)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let tgt = ontology
            .node_label(&edge.target_node_id)
            .map(mermaid_id)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let cardinality = mermaid_cardinality(&edge.cardinality);
        lines.push(format!(
            "    {src} {cardinality} {tgt} : \"{}\"",
            edge.label
        ));
    }

    lines.join("\n")
}

fn mermaid_type(pt: &PropertyType) -> &'static str {
    match pt {
        PropertyType::Bool => "boolean",
        PropertyType::Int => "int",
        PropertyType::Float => "float",
        PropertyType::String => "string",
        PropertyType::Date => "date",
        PropertyType::DateTime => "datetime",
        PropertyType::Duration => "string",
        PropertyType::Bytes => "bytes",
        PropertyType::List { .. } => "list",
        PropertyType::Map => "map",
    }
}

/// Sanitize a label for Mermaid: replace spaces/special chars with underscores.
fn mermaid_id(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Determine the PK property name for a node (first property in a Unique or NodeKey constraint).
fn find_pk_property(node: &NodeTypeDef) -> Option<&str> {
    for cdef in &node.constraints {
        match &cdef.constraint {
            NodeConstraint::Unique { property_ids } | NodeConstraint::NodeKey { property_ids } => {
                if let Some(pid) = property_ids.first()
                    && let Some(prop) = node.properties.iter().find(|p| p.id == *pid)
                {
                    return Some(&prop.name);
                }
            }
            NodeConstraint::Exists { .. } => {}
        }
    }
    None
}

fn mermaid_cardinality(c: &Cardinality) -> &'static str {
    match c {
        Cardinality::OneToOne => "||--||",
        Cardinality::OneToMany => "||--|{",
        Cardinality::ManyToOne => "}|--||",
        Cardinality::ManyToMany => "}|--|{",
    }
}
